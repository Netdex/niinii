//! Live integration tests for the Chat Completions API.
//!
//! These hit a real OpenAI-compatible server. They skip (with a printed notice)
//! unless `niinii.toml` has `[chat].api_endpoint` and `[chat].model`. See
//! [`common`] for details.

mod common;

use openai::chat::{
    FunctionDef, Message, Request, Role, Tool, ToolCallAccumulator, ToolChoice, ToolChoiceMode,
};
use tokio_stream::StreamExt;
use tracing_test::traced_test;

fn weather_tool() -> Tool {
    Tool::function(FunctionDef {
        name: "get_weather".into(),
        description: Some("Get the current weather for a city".into()),
        parameters: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "location": { "type": "string" },
                "unit": { "type": "string", "enum": ["celsius", "fahrenheit"] }
            },
            "required": ["location"],
            "additionalProperties": false,
        })),
        strict: Some(true),
    })
}

fn user(content: &str) -> Message {
    Message {
        role: Role::User,
        content: Some(content.into()),
        ..Default::default()
    }
}

#[tokio::test]
#[traced_test]
async fn chat_basic() {
    let (client, model) = fixture!();
    let request = Request::builder()
        .model(model)
        .messages(vec![user("What is the capital city of Canada?")])
        .build();
    let response = client.chat(request).await.unwrap();
    let content = response.choices[0].message.content.as_deref().unwrap_or("");
    println!("{}", content);
    assert!(content.contains("Ottawa"));
}

#[tokio::test]
#[traced_test]
async fn chat_stream_basic() {
    let (client, model) = fixture!();
    let request = Request::builder()
        .model(model)
        .messages(vec![user("What is the capital city of Canada?")])
        .build();
    let mut stream = client.stream(request).await.unwrap();
    while let Some(chunk) = stream.next().await {
        println!("{:?}", chunk);
    }
}

#[tokio::test]
#[traced_test]
async fn chat_tool_call_loop() {
    let (client, model) = fixture!();
    let tools = vec![weather_tool()];
    let mut messages = vec![user("What is the weather in Tokyo? Use the tool.")];

    // Turn 1: expect the model to call the tool.
    let req = Request::builder()
        .model(model.clone())
        .messages(messages.clone())
        .tools(tools.clone())
        .tool_choice(ToolChoice::Mode(ToolChoiceMode::Auto))
        .build();
    let resp = client.chat(req).await.unwrap();
    let choice = resp.choices.into_iter().next().unwrap();
    assert_eq!(choice.finish_reason.as_deref(), Some("tool_calls"));
    let assistant = choice.message;
    let tool_calls = assistant.tool_calls.clone().expect("tool_calls present");
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].function.name, "get_weather");
    messages.push(assistant);

    // Turn 2: return a canned result and let the model compose a reply.
    for call in &tool_calls {
        messages.push(Message::tool_result(
            &call.id,
            r#"{"location":"Tokyo","temp_c":22,"conditions":"clear"}"#,
        ));
    }
    let req = Request::builder()
        .model(model)
        .messages(messages)
        .tools(tools)
        .build();
    let resp = client.chat(req).await.unwrap();
    let choice = resp.choices.into_iter().next().unwrap();
    let content = choice.message.content.unwrap_or_default();
    println!("final: {}", content);
    assert!(content.to_lowercase().contains("tokyo"));
}

#[tokio::test]
#[traced_test]
async fn stream_tool_call_accumulates() {
    let (client, model) = fixture!();
    let req = Request::builder()
        .model(model)
        .messages(vec![user("What is the weather in Tokyo? Use the tool.")])
        .tools(vec![weather_tool()])
        .tool_choice(ToolChoice::Mode(ToolChoiceMode::Required))
        .build();
    let mut stream = client.stream(req).await.unwrap();
    let mut acc = ToolCallAccumulator::new();
    let mut finish = None;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.unwrap();
        for choice in chunk.choices {
            if let Some(calls) = choice.delta.tool_calls {
                acc.extend(calls);
            }
            if choice.finish_reason.is_some() {
                finish = choice.finish_reason;
            }
        }
    }
    assert_eq!(finish.as_deref(), Some("tool_calls"));
    let calls = acc.finish();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].function.name, "get_weather");
    let v: serde_json::Value = serde_json::from_str(&calls[0].function.arguments).unwrap();
    assert!(v.get("location").is_some());
}
