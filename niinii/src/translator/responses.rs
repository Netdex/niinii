use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use enclose::enclose;
use openai::{
    conversations::Conversation,
    responses::{
        self, ContextManagementEntry, ContextManagementType, Message, OutputItem, StreamEvent,
    },
    ConnectionPolicy, ModelId, Role,
};
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use tokio_util::sync::{CancellationToken, DropGuard};
use tracing::Instrument;

use crate::{
    settings::{ResponsesSettings, Settings},
    view::{
        translator::{
            ViewResponsesTranslation, ViewResponsesTranslationUsage, ViewResponsesTranslator,
        },
        View,
    },
};

use super::{Error, Translation, Translator};

#[derive(Clone, Debug)]
pub struct ConversationInfo {
    pub id: String,
    pub created_at: u64,
}

impl From<Conversation> for ConversationInfo {
    fn from(value: Conversation) -> Self {
        Self {
            id: value.id,
            created_at: value.created_at,
        }
    }
}

pub struct ResponsesTranslator {
    client: openai::Client,
    pub models: Vec<ModelId>,
    conversation: Mutex<Option<ConversationInfo>>,
}

impl ResponsesTranslator {
    pub async fn new(settings: &Settings) -> Self {
        let client = openai::Client::new(
            &settings.openai_api_key,
            &settings.chat.api_endpoint,
            ConnectionPolicy {
                timeout: Duration::from_millis(settings.chat.timeout),
                connect_timeout: Duration::from_millis(settings.chat.connection_timeout),
            },
        );

        let mut models = client
            .models()
            .await
            .inspect_err(|err| {
                tracing::error!(
                    ?err,
                    "failed to query OpenAI models, no models will be available"
                );
            })
            .unwrap_or_default();
        models.sort();

        Self {
            client,
            models,
            conversation: Mutex::new(None),
        }
    }

    async fn ensure_conversation(&self) -> Result<ConversationInfo, Error> {
        {
            let guard = self.conversation.lock().await;
            if let Some(existing) = guard.clone() {
                return Ok(existing);
            }
        }
        let conversation = self.client.create_conversation().await?;
        let info = ConversationInfo::from(conversation);
        *self.conversation.lock().await = Some(info.clone());
        Ok(info)
    }

    fn build_request(
        &self,
        settings: &ResponsesSettings,
        text: &str,
        conversation_id: Option<String>,
    ) -> responses::Request {
        let mut request = responses::Request::builder()
            .model(settings.model.clone())
            .input(vec![Message {
                role: Role::User,
                content: Some(text.to_owned()),
            }])
            .maybe_max_output_tokens(settings.max_output_tokens)
            .maybe_temperature(settings.temperature)
            .maybe_top_p(settings.top_p)
            .maybe_verbosity(settings.verbosity)
            .build();

        request.conversation = conversation_id;
        request.store = Some(settings.store);
        if !settings.system_prompt.trim().is_empty() {
            request.instructions = Some(settings.system_prompt.clone());
        }
        if let Some(effort) = settings.reasoning_effort {
            request.reasoning = Some(responses::ReasoningOptions::with_effort(effort));
        }
        if let Some(compact_threshold) = settings.compact_threshold {
            let threshold = compact_threshold.max(1000);
            if threshold != compact_threshold {
                tracing::warn!(
                    compact_threshold,
                    "compact_threshold must be >= 1000; clamping"
                );
            }
            request.context_management = Some(vec![ContextManagementEntry {
                entry_type: ContextManagementType::Compaction,
                compact_threshold: Some(threshold),
            }]);
        }

        request
    }

    fn collect_text(output: &[OutputItem]) -> String {
        let mut text = String::new();
        for item in output {
            if let OutputItem::Message(message) = item {
                if message.role == Role::Assistant {
                    for block in &message.content {
                        if let responses::MessageContent::OutputText(content) = block {
                            if !text.is_empty() {
                                text.push('\n');
                            }
                            text.push_str(&content.text);
                        }
                    }
                }
            }
        }
        text
    }

    pub fn conversation(&self) -> &Mutex<Option<ConversationInfo>> {
        &self.conversation
    }
}

#[derive(Default)]
pub struct TranslationState {
    pub text: String,
    pub usage: Option<responses::Usage>,
    pub response_id: Option<String>,
    pub completed: bool,
}

pub struct ResponsesTranslation {
    pub model: ModelId,
    pub conversation_id: Option<String>,
    state: Arc<Mutex<TranslationState>>,
    _guard: DropGuard,
}

#[async_trait]
impl Translator for ResponsesTranslator {
    async fn translate(
        &self,
        settings: &Settings,
        text: String,
    ) -> Result<Box<dyn Translation>, Error> {
        let responses_settings = &settings.responses;
        let conversation = self.ensure_conversation().await?;
        let request = self.build_request(responses_settings, &text, Some(conversation.id.clone()));

        let state = Arc::new(Mutex::new(TranslationState::default()));
        let token = CancellationToken::new();

        if responses_settings.stream {
            let mut stream = self.client.stream_responses(request).await?;
            let stream_state = Arc::clone(&state);
            let stream_token = token.clone();
            tokio::spawn(
                enclose! { (stream_state => state, stream_token => token) async move {
                    loop {
                        tokio::select! {
                            Some(event) = stream.next() => {
                                match event {
                                    Ok(StreamEvent::OutputTextDelta { delta }) => {
                                        state.lock().await.text.push_str(&delta);
                                    }
                                    Ok(StreamEvent::ResponseCompleted { response }) => {
                                        let mut guard = state.lock().await;
                                        guard.response_id = Some(response.id);
                                        guard.usage = response.usage;
                                        if !response.output.is_empty() {
                                            guard.text = ResponsesTranslator::collect_text(&response.output);
                                        }
                                        guard.completed = true;
                                        break;
                                    }
                                    Ok(StreamEvent::OutputTextDone) | Ok(StreamEvent::ResponseCreated { .. }) => {}
                                    Ok(StreamEvent::Unknown) => {}
                                    Err(err) => {
                                        tracing::error!(%err, "responses stream");
                                        break;
                                    }
                                }
                            }
                            _ = token.cancelled() => {
                                break;
                            }
                            else => { break; }
                        }
                    }
                }.instrument(tracing::Span::current())},
            );
        } else {
            let response = self.client.responses(request).await?;
            let mut guard = state.lock().await;
            guard.text = Self::collect_text(&response.output);
            guard.response_id = Some(response.id);
            guard.usage = response.usage;
            guard.completed = true;
        }

        Ok(Box::new(ResponsesTranslation {
            model: responses_settings.model.clone(),
            conversation_id: Some(conversation.id),
            state,
            _guard: token.drop_guard(),
        }))
    }

    fn view<'a>(&'a self, settings: &'a mut Settings) -> Box<dyn View + 'a> {
        Box::new(ViewResponsesTranslator(self, settings))
    }
}

impl Translation for ResponsesTranslation {
    fn view(&self) -> Box<dyn View + '_> {
        Box::new(ViewResponsesTranslation(self))
    }

    fn view_usage(&self) -> Box<dyn View + '_> {
        Box::new(ViewResponsesTranslationUsage(self))
    }
}

impl ResponsesTranslation {
    pub fn state(&self) -> &Arc<Mutex<TranslationState>> {
        &self.state
    }
}
