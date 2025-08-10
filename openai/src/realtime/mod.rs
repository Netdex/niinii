use std::{collections::VecDeque, future::Future, sync::Arc};

use futures_util::{stream::SplitSink, Sink, SinkExt, Stream, StreamExt};
use reqwest::Method;
use tokio::{
    net::TcpStream,
    sync::{mpsc, Mutex},
};
use tokio_stream::wrappers::ReceiverStream;
use tokio_tungstenite::{
    tungstenite::{
        self,
        client::IntoClientRequest,
        http::{self},
    },
    MaybeTlsStream, WebSocketStream,
};
use tracing::{Instrument, Level};

pub use crate::protocol::realtime::*;
use crate::{Client, Error};

#[derive(Clone)]
pub struct RealtimeSession {
    session: Session,
    state: Arc<Mutex<State>>,
}
struct State {
    client_tx: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, tungstenite::Message>,
    request_tx: mpsc::Sender<mpsc::Sender<Result<ServerEvent, Error>>>,
}
impl RealtimeSession {
    pub async fn new(create_session_response: CreateSessionResponse) -> Result<Self, crate::Error> {
        let ephemeral_key = create_session_response.0?.client_secret.value;
        let mut request = "wss://api.openai.com/v1/realtime"
            .into_client_request()
            .unwrap();
        request.headers_mut().extend([
            (
                http::header::AUTHORIZATION,
                format!("Bearer {}", ephemeral_key).parse().unwrap(),
            ),
            (
                "OpenAI-Beta".parse().unwrap(),
                "realtime=v1".parse().unwrap(),
            ),
        ]);

        let (ws_stream, _) = tokio_tungstenite::connect_async(request).await?;
        let (client_tx, mut client_rx) = ws_stream.split();

        let event = client_rx.next_response().await?.unwrap().event?;
        let session = match event {
            ServerEvent::SessionCreated { session } => session,
            _ => return Err(Error::UnexpectedResponse(Box::new(event))),
        };

        let (request_tx, mut request_rx) = mpsc::channel::<mpsc::Sender<_>>(1);

        // Task which feeds multiple senders from one receiver, switching to the
        // next sender when the current sender hangs up without losing any
        // values.
        tokio::spawn(async move {
            let mut buffer = VecDeque::new();
            let mut server_tx: Option<mpsc::Sender<_>> = None;
            loop {
                tokio::select! {
                    biased;
                    // We should always prioritize getting the next sender,
                    // since we would otherwise be buffering and increasing
                    // latency.
                    Some(tx) = request_rx.recv(), if server_tx.is_none() => 'branch: {
                        // new sender available, flush buffered events
                        while let Some(event) = buffer.pop_front() {
                            match tx.send(event).await {
                                Ok(_) => {},
                                Err(mpsc::error::SendError(event)) => {
                                    // new sender hung up, buffer event for next sender
                                    tracing::trace!(?event, "new sender hang up");
                                    buffer.push_back(event);
                                    break 'branch;
                                },
                            }
                        }
                        server_tx = Some(tx);
                    },
                    // client_rx must be constantly polled to keep the websocket
                    // alive, since this is how tokio-tungstenite responds to
                    // ping requests
                    response = client_rx.next_response() => {
                        match response.transpose() {
                            Some(response) => {
                                let event = response.and_then(|response| response.event.map_err(Error::Protocol));
                                match event {
                                    Ok(ServerEvent::RateLimitsUpdated { .. }) => {},
                                    Err(Error::Protocol(err)) if err.code.as_deref() == Some("response_cancel_not_active") => {},
                                    _ => {
                                        match &server_tx {
                                            Some(tx) => {
                                                match tx.send(event).await {
                                                    Ok(_) => {},
                                                    Err(mpsc::error::SendError(event)) => {
                                                        // sender hung up, buffer event for next sender
                                                        tracing::trace!(?event, "sender hang up");
                                                        buffer.push_back(event);
                                                        server_tx = None;
                                                    },
                                                }
                                            },
                                            None => {
                                                // no active sender, buffer event for next sender
                                                tracing::trace!(?event, "no active sender");
                                                buffer.push_back(event);
                                            },
                                        }
                                    }
                                }
                            },
                            None => {
                                tracing::trace!("connection closed");
                                // connection is dead, kill any active sender then kill ourselves
                                if let Some(tx) = server_tx {
                                    let _ = tx.send(Err(Error::WebSocket(tungstenite::Error::ConnectionClosed))).await;
                                }
                                return;
                            }
                        }
                    },
                }
            }
        });

        Ok(RealtimeSession {
            session,
            state: Arc::new(Mutex::new(State {
                client_tx,
                request_tx,
            })),
        })
    }
    #[tracing::instrument(level = Level::DEBUG, skip_all, err)]
    pub async fn session_update(
        &self,
        session_parameters: SessionParameters,
    ) -> Result<impl Future<Output = Result<Session, Error>>, Error> {
        let server_rx = self
            .request(&ClientEventRequest {
                event_id: None,
                event: ClientEvent::SessionUpdate {
                    session: session_parameters,
                },
            })
            .await?;
        Ok(async move {
            let event = server_rx.await?;
            match event {
                ServerEvent::SessionUpdated { session, .. } => Ok(session),
                _ => Err(Error::UnexpectedResponse(Box::new(event))),
            }
        })
    }

    #[tracing::instrument(level = Level::DEBUG, skip_all, err)]
    pub async fn conversation_item_create(
        &self,
        conversation_item: ConversationItem,
    ) -> Result<impl Future<Output = Result<ConversationItemCreated, Error>>, Error> {
        let server_rx = self
            .request(&ClientEventRequest {
                event_id: None,
                event: ClientEvent::ConversationItemCreate {
                    item: conversation_item,
                },
            })
            .await?;
        Ok(async move {
            let event = server_rx.await?;
            match event {
                ServerEvent::ConversationItemCreated(message) => Ok(message),
                _ => Err(Error::UnexpectedResponse(Box::new(event))),
            }
        })
    }

    #[tracing::instrument(level = Level::DEBUG, skip_all, err)]
    pub async fn conversation_item_delete(
        &self,
        item_id: ConversationItemId,
    ) -> Result<impl Future<Output = Result<ConversationItemId, Error>>, Error> {
        let server_rx = self
            .request(&ClientEventRequest {
                event_id: None,
                event: ClientEvent::ConversationItemDelete { item_id },
            })
            .await?;
        Ok(async move {
            let event = server_rx.await?;
            match event {
                ServerEvent::ConversationItemDeleted { item_id } => Ok(item_id),
                _ => Err(Error::UnexpectedResponse(Box::new(event))),
            }
        })
    }

    #[tracing::instrument(level = Level::DEBUG, skip_all, err)]
    pub async fn response_create(
        &self,
        response_parameters: ResponseParameters,
    ) -> Result<impl Stream<Item = Result<ServerEvent, Error>>, Error> {
        let (tx, rx) = mpsc::channel(4);
        let mut guard = self.state.clone().lock_owned().await;
        guard
            .client_tx
            .send_request(&ClientEventRequest {
                event_id: None,
                event: ClientEvent::ResponseCreate {
                    response: response_parameters,
                },
            })
            .await?;

        // We can't simply return a stream using something like try_stream!
        // here, since we want to cancel the response if the rx side is dropped.
        tokio::spawn(
            async move {
                // guard needs to be held across tasks to prevent other request
                // from barging
                let State {
                    client_tx,
                    request_tx,
                } = &mut *guard;

                let (server_tx, mut server_rx) = mpsc::channel(1);
                request_tx.send(server_tx).await.unwrap();

                let mut response_id = None;
                let mut cancelled = false;
                loop {
                    tokio::select! {
                        event = server_rx.recv() => {
                            let event = event.unwrap();
                            let mut done = false;
                            match &event {
                                Ok(ServerEvent::ResponseCreated { response }) => {
                                    response_id = Some(response.id.clone());
                                },
                                Ok(ServerEvent::ResponseDone {..}) => {
                                    done = true;
                                },
                                _ => {},
                            }
                            let _ = tx.send(event).await;
                            if done {
                                return;
                            }
                        }
                        _ = tx.closed(), if !cancelled => {
                            // on rx hang up, cancel response by id if available,
                            // otherwise cancel latest response
                            client_tx.send_request(&ClientEventRequest {
                                event_id: None,
                                event: ClientEvent::ResponseCancel {
                                    response_id: response_id.clone(),
                                },
                            }).await.ok();
                            cancelled = true;
                        }
                    }
                }
            }
            .in_current_span(),
        );
        Ok(ReceiverStream::new(rx))
    }

    async fn request(
        &self,
        request: &ClientEventRequest,
    ) -> Result<impl Future<Output = Result<ServerEvent, Error>>, Error> {
        let State {
            client_tx,
            request_tx,
        } = &mut *self.state.lock().await;
        client_tx.send_request(request).await?;
        let (server_tx, mut server_rx) = mpsc::channel(1);
        request_tx.send(server_tx).await.unwrap();
        Ok(async move { server_rx.recv().await.unwrap() })
    }

    pub fn info(&self) -> &Session {
        &self.session
    }
}

impl Client {
    #[tracing::instrument(level = Level::DEBUG, skip_all, err)]
    pub async fn realtime(
        &self,
        session_parameters: SessionParameters,
    ) -> Result<RealtimeSession, Error> {
        let create_session_request = CreateSessionRequest(session_parameters);
        tracing::debug!(?create_session_request);
        let create_session_response: CreateSessionResponse = self
            .shared
            .request(Method::POST, "/v1/realtime/sessions")
            .body(&create_session_request)
            .beta("realtime=v1")
            .send()
            .await?
            .json()
            .await?;
        tracing::debug!(?create_session_response);
        RealtimeSession::new(create_session_response).await
    }
}

trait ServerEventResponseStream {
    async fn next_response(&mut self) -> Result<Option<ServerEventResponse>, Error>;
}
impl<S> ServerEventResponseStream for S
where
    S: Stream<Item = tungstenite::Result<tungstenite::Message>> + Unpin,
{
    async fn next_response(&mut self) -> Result<Option<ServerEventResponse>, Error> {
        while let Some(message) = self.next().await {
            let message = message?;
            if let tungstenite::Message::Text(_) = message {
                let response = (&message)
                    .try_into()
                    .inspect(|response| tracing::debug!(?response))
                    .inspect_err(|err| tracing::error!(%err));
                return Ok(Some(response?));
            } else {
                tracing::trace!(?message, "discarding websocket message")
            }
        }
        Ok(None)
    }
}

trait ClientEventRequestSink {
    async fn send_request(&mut self, request: &ClientEventRequest) -> Result<(), Error>;
}
impl<S> ClientEventRequestSink for S
where
    S: Sink<tungstenite::Message, Error = tungstenite::Error> + Unpin,
{
    async fn send_request(&mut self, request: &ClientEventRequest) -> Result<(), Error> {
        tracing::debug!(?request);
        Ok(self.send(request.try_into()?).await?)
    }
}

#[cfg(test)]
mod tests {
    use tracing_test::traced_test;

    use super::*;
    use crate::{tests::fixture, ModelId};

    #[tokio::test]
    #[traced_test]
    #[ignore]
    async fn test_response() -> Result<(), Error> {
        let client = fixture::client();
        let session = client
            .realtime(SessionParameters {
                inference_parameters: InferenceParameters {
                    model: Some(ModelId("gpt-4o-mini-realtime-preview".into())),
                    modalities: vec![Modality::Text],
                    ..Default::default()
                },
            })
            .await?;
        let prompt = ConversationItem::input_text("What is the capital of Canada?");
        let fut = session.conversation_item_create(prompt).await?;
        let mut response_stream = session
            .response_create(ResponseParameters::default())
            .await?;
        let conversation_item_created = fut.await?;
        println!("{:?}", conversation_item_created);
        while let Some(message) = response_stream.next().await {
            println!("{:?}", message?);
        }
        Ok(())
    }
}
