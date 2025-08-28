use std::sync::atomic::AtomicU32;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::time::interval;
use url::Url;
use uuid::Uuid;

use crate::models::player::Player;

const CACHE_DURATION: Duration = Duration::from_secs(15);

pub(crate) struct PlayerFetcherService {
    signal_tx: mpsc::UnboundedSender<SignalRequest>,
    failed_to_fetch_players_count: AtomicU32,
    http_client: reqwest::Client,
    berg_api_base: Url
}
impl PlayerFetcherService {
    pub(crate) fn new(berg_api_base: Url, http_client: reqwest::Client) -> Arc<Self> {
        let (signal_request_tx, signal_request_rx) = mpsc::unbounded_channel();
        let instance = Arc::new(Self {
            signal_tx: signal_request_tx,
            failed_to_fetch_players_count: AtomicU32::default(),
            http_client,
            berg_api_base
        });
        tokio::spawn({
            let instance = instance.clone();
            async move {
                instance.run(signal_request_rx).await
            }
        });
        instance
    }
    pub(crate) async fn get_player(&self, player_id: Uuid) -> Option<Arc<Player>> {
        let (response_tx, response_rx) = oneshot::channel();
        // Handled in next line instead
        let _ = self.signal_tx.send(SignalRequest::GetPlayer(GetPlayerSignalRequest {
            player_id,
            response_tx
        }));
        response_rx.await.ok()
    }
    async fn run(self: Arc<Self>, mut signal_rx: mpsc::UnboundedReceiver<SignalRequest>) {
        let mut players = Vec::<Arc<Player>>::new();
        let mut poll_interval = interval(CACHE_DURATION);

        loop {
            tokio::select! {
                _ = poll_interval.tick() => {
                    match self.fetch_players().await {
                        Ok(new_players) => {
                            players = new_players.into_iter().map(Arc::new).collect();
                        },
                        Err(error) => {
                            tracing::error!(?error, "failed to fetch players");
                        }
                    }
                },
                maybe_message = signal_rx.recv() => {
                    let Some(message) = maybe_message else {
                        return;
                    };
                    match message {
                        SignalRequest::GetPlayer(request) => {
                            let maybe_player = players.iter().filter(|player| player.id == request.player_id).next();
                            if let Some(player) = maybe_player {
                                let _ = request.response_tx.send(player.clone());
                            }
                        }
                    }
                },
            };
        }
    }
    async fn fetch_players(&self) -> Result<Vec<Player>, reqwest::Error> {
        let players_url = self.berg_api_base.join("players").expect("players is hard-coded and known to be good");
        self.http_client
            .get(players_url)
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<Player>>()
            .await
    }
}

enum SignalRequest {
    GetPlayer(GetPlayerSignalRequest)
}
struct GetPlayerSignalRequest {
    player_id: Uuid,
    response_tx: oneshot::Sender<Arc<Player>>
}

