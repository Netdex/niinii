//! Live integration test for the Realtime API over WebSocket.
//!
//! Hits a real OpenAI server. Skips (with a printed notice) unless `niinii.toml`
//! has `openai_api_endpoint` + `openai_model`. See [`common`].
//!
//! The Realtime API needs a realtime-capable model, which is usually NOT the
//! same as the configured chat/responses model, so this test reuses the
//! configured endpoint + key but connects with [`REALTIME_MODEL`] rather than
//! the configured model.

mod common;

use openai::{
    realtime::{ClientEvent, Item, ServerEvent, SessionConfig},
    ModelId,
};
use tracing_test::traced_test;

/// GA realtime model. The configured `openai_model` (e.g. a chat model) is not
/// realtime-capable, so the test pins this instead.
const REALTIME_MODEL: &str = "gpt-realtime";

#[tokio::test]
#[traced_test]
async fn realtime_text_basic() {
    let (client, _model) = fixture!();
    let model = ModelId(REALTIME_MODEL.to_string());

    let mut session = client.connect_realtime(&model).await.unwrap();

    // Configure a text-only session.
    session
        .send(&ClientEvent::SessionUpdate {
            session: SessionConfig::text(
                "You are a helpful assistant. Answer in one short sentence.",
            ),
        })
        .await
        .unwrap();

    // Push a user message and ask for a response.
    session
        .send(&ClientEvent::ConversationItemCreate {
            item: Item::user_text("What is the capital city of Canada?"),
        })
        .await
        .unwrap();
    session
        .send(&ClientEvent::ResponseCreate { response: None })
        .await
        .unwrap();

    let mut text = String::new();
    while let Some(event) = session.next_event().await {
        match event.unwrap() {
            ServerEvent::OutputTextDelta { delta, .. } => text.push_str(&delta),
            ServerEvent::OutputTextDone { text: done } => {
                // `done` carries the full text for this content part.
                if text.is_empty() {
                    text = done;
                }
            }
            ServerEvent::ResponseDone { response } => {
                println!("usage: {:?}", response.usage);
                break;
            }
            ServerEvent::Error { error } => panic!("realtime error: {error:?}"),
            other => println!("{other:?}"),
        }
    }

    println!("streamed: {text}");
    assert!(text.contains("Ottawa"));
}
