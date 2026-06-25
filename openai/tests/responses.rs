//! Live integration tests for the Responses API.
//!
//! These hit a real OpenAI-compatible server. They skip (with a printed notice)
//! unless `niinii.toml` has `openai_api_endpoint` + `openai_model`. See
//! [`common`] for details.

mod common;

use openai::responses::{Input, Request, StreamEvent};
use tokio_stream::StreamExt;
use tracing_test::traced_test;

fn text_input(content: &str) -> Input {
    Input::Text(content.into())
}

#[tokio::test]
#[traced_test]
async fn responses_basic() {
    let (client, model) = fixture!();
    let request = Request::builder()
        .model(model)
        .input(text_input("What is the capital city of Canada?"))
        .build();
    let response = client.responses(request).await.unwrap();
    let text = response.output_text();
    println!("{}", text);
    assert!(text.contains("Ottawa"));
}

#[tokio::test]
#[traced_test]
async fn responses_stream_basic() {
    let (client, model) = fixture!();
    let request = Request::builder()
        .model(model)
        .input(text_input("What is the capital city of Canada?"))
        .build();
    let mut stream = client.stream_responses(request).await.unwrap();
    let mut text = String::new();
    while let Some(event) = stream.next().await {
        match event.unwrap() {
            StreamEvent::OutputTextDelta { delta } => text.push_str(&delta),
            other => println!("{:?}", other),
        }
    }
    println!("streamed: {}", text);
    assert!(text.contains("Ottawa"));
}

/// Server-side conversation state: turn 2 references turn 1 via
/// `previous_response_id` and recalls information that was never resent. This is
/// the mechanism behind the UI's "Reset conversation" -- omitting
/// `previous_response_id` (as a reset does) starts a fresh, memory-less chain.
#[tokio::test]
#[traced_test]
async fn responses_conversation_chain() {
    let (client, model) = fixture!();

    // Turn 1: state something and persist the response (`store: true`).
    let req1 = Request::builder()
        .model(model.clone())
        .input(text_input(
            "Remember this secret word: bluebird. Reply with just 'ok'.",
        ))
        .store(true)
        .build();
    let resp1 = client.responses(req1).await.unwrap();
    let id1 = resp1.id;
    assert!(
        !id1.is_empty(),
        "server must return a response id to chain on"
    );

    // Turn 2: continue the chain without resending the word.
    let req2 = Request::builder()
        .model(model.clone())
        .input(text_input(
            "What was the secret word? Reply with just the word.",
        ))
        .store(true)
        .previous_response_id(id1)
        .build();
    let resp2 = client.responses(req2).await.unwrap();
    let chained = resp2.output_text().to_lowercase();
    println!("chained reply: {}", chained);
    assert!(
        chained.contains("bluebird"),
        "chained turn should recall state from the previous response"
    );

    // Control: a fresh request (no previous_response_id, like after a reset)
    // has no access to that state.
    let req3 = Request::builder()
        .model(model)
        .input(text_input(
            "What was the secret word? Reply with just the word.",
        ))
        .build();
    let resp3 = client.responses(req3).await.unwrap();
    let fresh = resp3.output_text().to_lowercase();
    println!("fresh reply: {}", fresh);
    assert!(
        !fresh.contains("bluebird"),
        "a chain-less request must not recall the previous conversation"
    );
}
