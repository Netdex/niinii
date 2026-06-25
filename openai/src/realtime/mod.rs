//! GA Realtime API client over WebSocket.
//!
//! https://developers.openai.com/api/docs/guides/realtime-websocket
//!
//! [`Client::connect_realtime`] opens a `wss://.../v1/realtime?model=...`
//! connection (deriving the host + auth from the same [`Client`] the HTTP
//! backends use). The returned [`Realtime`] is a thin duplex wrapper: send typed
//! [`ClientEvent`]s, receive typed [`ServerEvent`]s. Text-only; audio frames are
//! never sent and audio events deserialize to [`ServerEvent::Other`].

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tracing::Level;
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{
        client::IntoClientRequest,
        http::{header::AUTHORIZATION, HeaderValue},
        Message,
    },
    MaybeTlsStream, WebSocketStream,
};

pub use crate::protocol::realtime::{
    ClientEvent, ContentPart, InputTokenDetails, Item, ItemType, Modality, OutputTokenDetails,
    RealtimeError, RealtimeReasoning, RealtimeTruncation, RealtimeUsage, ResponseConfig,
    ResponseMeta, ResponseObject, ServerEvent, SessionConfig, SessionType, StatusDetails,
};

use crate::{Client, Error, ModelId};

/// An open Realtime WebSocket session. Drive it by [`send`](Realtime::send)ing
/// client events and awaiting [`next_event`](Realtime::next_event).
pub struct Realtime {
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl Realtime {
    /// Send one client event as a JSON text frame.
    #[tracing::instrument(level = Level::DEBUG, skip_all, err)]
    pub async fn send(&mut self, event: &ClientEvent) -> Result<(), Error> {
        tracing::debug!(?event);
        let payload = serde_json::to_string(event)?;
        self.ws.send(Message::Text(payload.into())).await?;
        Ok(())
    }

    /// Send several client events with a single flush. Fewer writes than calling
    /// [`send`](Self::send) repeatedly, and the events reach the server together
    /// (e.g. `conversation.item.create` + `response.create`), so generation can
    /// begin on one network read instead of waiting for a second.
    #[tracing::instrument(level = Level::DEBUG, skip_all, err)]
    pub async fn send_all(&mut self, events: &[ClientEvent]) -> Result<(), Error> {
        for event in events {
            tracing::debug!(?event);
            let payload = serde_json::to_string(event)?;
            self.ws.feed(Message::Text(payload.into())).await?;
        }
        self.ws.flush().await?;
        Ok(())
    }

    /// Send a WebSocket ping. Used to keep an idle connection (and any NAT /
    /// firewall mappings) warm so a turn after a long pause does not have to
    /// reconnect, and to detect a silently-dropped socket proactively.
    #[tracing::instrument(level = Level::TRACE, skip_all, err)]
    pub async fn ping(&mut self) -> Result<(), Error> {
        self.ws.send(Message::Ping(Default::default())).await?;
        Ok(())
    }

    /// Await the next typed server event. Non-text frames (ping/pong/binary) are
    /// skipped; `None` means the socket closed.
    pub async fn next_event(&mut self) -> Option<Result<ServerEvent, Error>> {
        loop {
            match self.ws.next().await? {
                Ok(Message::Text(text)) => {
                    tracing::trace!(%text);
                    let event = serde_json::from_str::<ServerEvent>(text.as_str());
                    match &event {
                        Ok(event) => tracing::debug!(?event),
                        Err(err) => tracing::error!(?err, %text, "failed to parse server event"),
                    }
                    return Some(event.map_err(Into::into));
                }
                Ok(Message::Close(_)) => {
                    tracing::debug!("realtime socket closed by server");
                    return None;
                }
                // Ping/Pong are auto-handled by tungstenite; ignore stray frames.
                Ok(_) => continue,
                Err(err) => {
                    tracing::error!(?err, "realtime socket error");
                    return Some(Err(err.into()));
                }
            }
        }
    }

    /// Close the connection gracefully.
    #[tracing::instrument(level = Level::DEBUG, skip_all, err)]
    pub async fn close(mut self) -> Result<(), Error> {
        self.ws.close(None).await?;
        Ok(())
    }
}

impl Client {
    /// Open a text-only Realtime session against `model`. Derives the `wss` URL
    /// and bearer auth from the client's configured endpoint/token.
    #[tracing::instrument(level = Level::DEBUG, skip_all, fields(model = %model.as_ref()), err)]
    pub async fn connect_realtime(&self, model: &ModelId) -> Result<Realtime, Error> {
        // api_base is normalized with a trailing slash by `Client::new`. Swap the
        // HTTP scheme for the WebSocket one and append the realtime path.
        let base = self.shared.api_base.as_str();
        let ws_base = if let Some(rest) = base.strip_prefix("https://") {
            format!("wss://{rest}")
        } else if let Some(rest) = base.strip_prefix("http://") {
            format!("ws://{rest}")
        } else {
            base.to_string()
        };
        let url = format!("{ws_base}v1/realtime?model={}", model.as_ref());
        tracing::debug!(%url, "opening realtime connection");

        let mut request = url
            .into_client_request()
            .map_err(|e| Error::Realtime(format!("invalid realtime url: {e}")))?;
        let auth = HeaderValue::from_str(&format!("Bearer {}", self.shared.token))
            .map_err(|e| Error::Realtime(format!("invalid auth header: {e}")))?;
        request.headers_mut().insert(AUTHORIZATION, auth);

        // `disable_nagle = true` sets TCP_NODELAY: the small JSON client events
        // (session.update / item.create / response.create) flush immediately
        // instead of waiting on Nagle + delayed-ACK, directly lowering TTFT.
        let (ws, _resp) = connect_async_with_config(request, None, true).await?;
        tracing::debug!("realtime connection established");
        Ok(Realtime { ws })
    }
}
