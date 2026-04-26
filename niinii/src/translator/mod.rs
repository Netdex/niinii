//! Translator: backend (`chat`) and the application-level controller.
//!
//! `Translator` is the App's handle to the running backend task. It owns the
//! `ChatHandle` (commands in, state snapshot out) plus the "current
//! exchange" id, and provides settings-aware dispatch methods. Views
//! (`TranslatorWindow`) borrow the controller read-only to render exchange
//! data; the App orchestrates by mutating it.

pub mod chat;

pub use chat::{
    ChatHandle, ChatState, ContextEdit, ExchangeId, ExchangeView, MsgId, Response, TranslationSpan,
};

use std::sync::Arc;

use crate::settings::{Settings, TranslatorType};
use chat::TranslateConfig;

/// Application-layer controller around the chat backend.
pub struct Translator {
    handle: ChatHandle,
    current: Option<ExchangeId>,
}

impl Translator {
    pub fn new(settings: &Settings) -> Self {
        let handle = match settings.translator_type {
            TranslatorType::Chat => chat::spawn(settings),
        };
        Self {
            handle,
            current: None,
        }
    }

    pub fn handle(&self) -> &ChatHandle {
        &self.handle
    }

    pub fn current(&self) -> Option<ExchangeId> {
        self.current
    }

    /// Cheap snapshot of the chat state. Hold across a render pass to
    /// borrow data out of it without re-loading.
    pub fn state(&self) -> Arc<ChatState> {
        self.handle.state()
    }

    /// The currently in-flight or last-displayed exchange, if any.
    pub fn current_exchange<'a>(&self, state: &'a ChatState) -> Option<&'a ExchangeView> {
        state.exchange(self.current?)
    }

    /// Cancel any in-flight exchange and submit a basic-split segment translation.
    /// Translation spans stream into `Response::translations()` using the same
    /// segment indices supplied here.
    pub fn translate(
        &mut self,
        settings: &Settings,
        text: String,
        segments: Arc<Vec<String>>,
    ) -> ExchangeId {
        if let Some(prev) = self.current {
            self.handle.cancel(prev);
        }
        let config = Arc::new(TranslateConfig::from_settings(settings));
        let id = self.handle.translate(text, config, segments);
        self.current = Some(id);
        id
    }

    /// Cancel and forget the current exchange, if any.
    pub fn cancel_current(&mut self) {
        if let Some(prev) = self.current.take() {
            self.handle.cancel(prev);
        }
    }

    /// Forget the current exchange without cancelling it. Used when a new
    /// gloss arrives and the user has not opted into auto-translate.
    pub fn clear_current(&mut self) {
        self.current = None;
    }

    /// Translation spans of the exchange identified by `id`, borrowed
    /// from `state`. Returns `None` if the exchange is gone (rare; only if
    /// context was cleared mid-flight).
    pub fn translations_for<'a>(
        &self,
        state: &'a ChatState,
        id: ExchangeId,
    ) -> Option<&'a [TranslationSpan]> {
        state.exchange(id).map(|ex| ex.response.translations())
    }
}
