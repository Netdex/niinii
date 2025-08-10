use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use enclose::enclose;
use futures::StreamExt;
use openai::{realtime::*, ConnectionPolicy};
use tokio::sync::{Mutex, OnceCell, RwLock, RwLockReadGuard, RwLockWriteGuard};
use tokio_util::sync::{CancellationToken, DropGuard};

use crate::{
    settings::{RealtimeSettings, Settings},
    view::{
        translator::{
            ViewRealtimeTranslation, ViewRealtimeTranslationUsage, ViewRealtimeTranslator,
        },
        View,
    },
};

use super::{Error, Translation, Translator};

pub struct RealtimeTranslator {
    client: openai::Client,
    pub models: Vec<openai::ModelId>,
    session: RwLock<OnceCell<RealtimeSession>>,
}
impl RealtimeTranslator {
    pub async fn new(settings: &Settings) -> Self {
        let client = openai::Client::new(
            &settings.openai_api_key,
            &settings.chat.api_endpoint,
            ConnectionPolicy {
                timeout: Duration::from_millis(settings.chat.timeout),
                connect_timeout: Duration::from_millis(settings.chat.connection_timeout),
            },
        );
        let models = client
            .models()
            .await
            .inspect_err(|err| {
                tracing::error!(
                    ?err,
                    "failed to query OpenAI models, no models will be available"
                )
            })
            .unwrap_or_default();
        Self {
            client,
            models,
            session: RwLock::new(OnceCell::new()),
        }
    }
    async fn create_session(
        &self,
        settings: &RealtimeSettings,
    ) -> Result<RealtimeSession, openai::Error> {
        self.client
            .realtime(SessionParameters {
                inference_parameters: InferenceParameters {
                    modalities: vec![Modality::Text],
                    model: Some(settings.model.clone()),
                    temperature: settings.temperature,
                    instructions: Some(settings.system_prompt.clone()),
                    ..Default::default()
                },
            })
            .await
    }
    pub fn session(&self) -> RwLockReadGuard<'_, OnceCell<RealtimeSession>> {
        self.session.blocking_read()
    }
    pub fn session_mut(&self) -> RwLockWriteGuard<'_, OnceCell<RealtimeSession>> {
        self.session.blocking_write()
    }
}

#[async_trait]
impl Translator for RealtimeTranslator {
    async fn translate(
        &self,
        settings: &Settings,
        text: String,
    ) -> Result<Box<dyn Translation>, Error> {
        let session = self.session.read().await;
        let session = session
            .get_or_try_init(|| self.create_session(&settings.realtime))
            .await?;

        // send both requests, in flight simultaneously
        let fut = session
            .conversation_item_create(ConversationItem::input_text(text))
            .await?;
        let mut stream = session
            .response_create(ResponseParameters {
                ..Default::default()
            })
            .await?;
        fut.await?;

        let inner = Arc::new(Mutex::new(Inner::default()));
        let token = CancellationToken::new();
        tokio::spawn(enclose! { (inner, token) async move {
            loop {
                tokio::select! {
                    Some(event) = stream.next() => {
                        match event {
                            Ok(ServerEvent::ResponseDone { response }) => {
                                let Inner { usage, ..} = &mut *inner.lock().await;
                                *usage = response.usage;
                            }
                            Ok(ServerEvent::ResponseTextDelta(delta)) => {
                                let Inner { text, .. } = &mut *inner.lock().await;
                                text.push_str(&delta.delta);
                            }
                            Ok(ServerEvent::ResponseTextDone(response_text)) => {
                                let Inner { text, .. } = &mut *inner.lock().await;
                                *text = response_text.text;
                            }
                            Ok(_) => {}
                            Err(err) => {
                                tracing::error!(%err, "stream");
                                break;
                            }
                        }
                    }
                    _ = token.cancelled() => {
                        break;
                    }
                    else => { break }
                }
            }
        }});

        Ok(Box::new(RealtimeTranslation {
            inner,
            _guard: token.drop_guard(),
        }))
    }

    fn view<'a>(&'a self, settings: &'a mut Settings) -> Box<dyn View + 'a> {
        Box::new(ViewRealtimeTranslator(self, settings))
    }
}

#[derive(Default)]
pub struct Inner {
    pub text: String,
    pub usage: Option<Usage>,
}
pub struct RealtimeTranslation {
    pub inner: Arc<Mutex<Inner>>,
    _guard: DropGuard,
}
impl Translation for RealtimeTranslation {
    fn view(&self) -> Box<dyn View + '_> {
        Box::new(ViewRealtimeTranslation(self))
    }

    fn view_usage(&self) -> Box<dyn View + '_> {
        Box::new(ViewRealtimeTranslationUsage(self))
    }
}
