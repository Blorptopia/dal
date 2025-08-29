use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use bytes::Bytes;
use futures::StreamExt;
use tokio::net::TcpStream;
use tokio::time::interval;
use tokio_tungstenite::tungstenite::{ClientRequestBuilder, Message as WebSocketMessage};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use url::Url;
use crate::models::solve::Solve;
use crate::models::websocket::WebSocketResponse;
use crate::USER_AGENT;
use tokio::sync::mpsc;
use futures::SinkExt;

pub(crate) struct SolveFetcherService {
    restart_count: AtomicU32,
    dropped_solves_count: AtomicU32,
    berg_api_url: Url,
    http_client: reqwest::Client,
    sender: mpsc::UnboundedSender<Solve>
}
impl SolveFetcherService {
    pub(crate) fn new(berg_api_url: Url, http_client: reqwest::Client, sender: mpsc::UnboundedSender<Solve>) -> Arc<Self> {
        let instance = Arc::new(Self {
            restart_count: AtomicU32::default(),
            dropped_solves_count: AtomicU32::default(),
            berg_api_url,
            http_client,
            sender
        });
        instance
    }
    pub(crate) fn start(self: Arc<Self>) {
        tokio::spawn({
            let instance = self;
            async move {
                instance.run_with_retries().await
            }
        });
    }
    pub(crate) fn restart_count(&self) -> u32 {
        self.restart_count.load(Ordering::SeqCst)
    }
    pub(crate) fn dropped_solves_count(&self) -> u32 {
        self.dropped_solves_count.load(Ordering::SeqCst)
    }
    async fn run_with_retries(self: &Arc<Self>) {
        // Max retry every 5s
        let mut retry_interval = interval(Duration::from_secs(5));

        loop {
            retry_interval.tick().await;
            if let Err(error) = self.run().await {
                tracing::error!(?error, "SolveFetcherService failed, restarting");
                self.restart_count.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
    async fn run(self: &Arc<Self>) -> Result<(), SolveFetcherError> {
        let websocket = {
            let mut events_url = self.berg_api_url.join("events").expect("hard-coded path should always be fine to join to berg_api_url");
            let websocket_scheme = match events_url.scheme() {
                "http" => "ws",
                "https" => "wss",
                _ => return Err(SolveFetcherError::UnsupportedAPIScheme)
            };
            events_url.set_scheme(websocket_scheme).map_err(|_error| SolveFetcherError::UnsupportedAPIScheme)?;
            let events_uri = events_url.as_str().parse::<http::Uri>().expect("this is a valid url via the url crate");

            let request = ClientRequestBuilder::new(events_uri)
                .with_header("User-Agent", USER_AGENT);
            tokio_tungstenite::connect_async(request).await.map_err(SolveFetcherError::EventWebSocketConnectError)?.0
        };
        {
            let seed_solves = self.fetch_solves().await.map_err(SolveFetcherError::FailedToFetchSeedData)?;
            tracing::debug!("sending {} seeded solves", seed_solves.len());
            for solve in seed_solves {
                if self.sender.send(solve).is_err() {
                    self.dropped_solves_count.fetch_add(1, Ordering::SeqCst);
                }
            }
            tracing::debug!("sent seeded solves");
        }
        
        let (_message_tx, mut message_rx) = {
            let (message_sender_tx, message_sender_rx) = mpsc::unbounded_channel();
            let (message_receiver_tx, message_receiver_rx) = mpsc::unbounded_channel();
            tokio::spawn(Self::handle_websocket(websocket, message_sender_rx, message_receiver_tx));

            (message_sender_tx, message_receiver_rx)
        };

        while let Some(raw_message) = message_rx.recv().await {
            let message = match serde_json::from_str::<WebSocketResponse>(&raw_message) {
                Ok(message) => message,
                Err(error) => {
                    tracing::warn!(?error, "oopsie");
                    continue;
                }
            };

            let WebSocketResponse::Solve(solve) = message else {
                continue;
            };
            tracing::debug!(?solve, "got solve from events ws");
            if let Err(error) = self.sender.send(solve) {
                self.dropped_solves_count.fetch_add(1, Ordering::SeqCst);
                tracing::error!(?error, "failed to send solve to subscriber");
            }
        }
        Err(SolveFetcherError::EventWebSocketDisconnected)

    }
    async fn handle_websocket(websocket: WebSocketStream<MaybeTlsStream<TcpStream>>, mut message_rx: mpsc::UnboundedReceiver<String>, message_tx: mpsc::UnboundedSender<String>) {
        let (mut write, mut read) = websocket.split();

        let mut ping_interval = interval(Duration::from_secs(10));

        loop {
            // TODO: This does not implement chunked responses. They *shouldn't* be produced by
            // berg so I don't really care.
            tokio::select! {
                _ = message_tx.closed() => break,
                to_send = message_rx.recv() => {
                    let Some(to_send) = to_send else {
                        break;
                    };
                    if let Err(error) = write.send(WebSocketMessage::Text(to_send.into())).await {
                        tracing::error!(?error, "failed to send text to events websocket");
                        break;
                    }
                },
                _ = ping_interval.tick() => {
                    let payload = Bytes::from_static(b"hi");
                    if let Err(error) = write.send(WebSocketMessage::Ping(payload)).await {
                        tracing::error!(?error, "failed to ping events websocket");
                        break;
                    }
                },
                raw_message = read.next() => {
                    let raw_message = match raw_message {
                        Some(Ok(raw_message)) => raw_message,
                        Some(Err(error)) => {
                            tracing::error!(?error, "failed to receive from events websocket");
                            break;
                        },
                        None => {
                            tracing::error!("disconnected from events websocket");
                            break;
                        }
                    };
                    match raw_message {
                        WebSocketMessage::Text(raw_data) => {
                            let data = raw_data.to_string();
                            tracing::debug!(?data, "raw text from events websocket");
                            if message_tx.send(data).is_err() {
                                break;
                            }
                        },
                        WebSocketMessage::Close(_) => {
                            tracing::error!("disconnected from events websocket");
                            break;
                        },
                        WebSocketMessage::Ping(message) => {
                            if let Err(error) = write.send(WebSocketMessage::Pong(message)).await {
                                tracing::error!(?error, "failed to send pong events websocket");
                                break;
                            }
                        },
                        WebSocketMessage::Pong(..) => {},
                        _ => todo!()
                    }
                }
            }
        }
    }
    async fn fetch_solves(&self) -> Result<Vec<Solve>, reqwest::Error> {
        let solves_url = self.berg_api_url.join("solves").expect("hard-coded path should always be fine to join to berg_api_url");
        let solves = self.http_client.get(solves_url)
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<Solve>>()
            .await?;
        Ok(solves)
    }
}

#[derive(thiserror::Error, Debug)]
enum SolveFetcherError {
    #[error("unsupported api scheme")]
    UnsupportedAPIScheme,
    #[error("websocket connect error")]
    EventWebSocketConnectError(tokio_tungstenite::tungstenite::Error),
    #[error("failed to fetch seed data")]
    FailedToFetchSeedData(reqwest::Error),
    #[error("failed to parse message from events websocket")]
    EventMessageParseError(serde_json::Error),
    #[error("event websocket disconnected")]
    EventWebSocketDisconnected
}
