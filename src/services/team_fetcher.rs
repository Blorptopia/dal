use std::sync::atomic::AtomicU32;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::time::interval;
use url::Url;
use uuid::Uuid;

use crate::models::team::Team;

const CACHE_DURATION: Duration = Duration::from_secs(15);

pub(crate) struct TeamFetcherService {
    signal_tx: mpsc::UnboundedSender<SignalRequest>,
    failed_to_fetch_teams_count: AtomicU32,
    http_client: reqwest::Client,
    berg_api_base: Url
}
impl TeamFetcherService {
    pub(crate) fn new(berg_api_base: Url, http_client: reqwest::Client) -> Arc<Self> {
        let (signal_request_tx, signal_request_rx) = mpsc::unbounded_channel();
        let instance = Arc::new(Self {
            signal_tx: signal_request_tx,
            failed_to_fetch_teams_count: AtomicU32::default(),
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
    pub(crate) async fn get_players_team(&self, player_id: Uuid) -> Option<Arc<Team>> {
        let (response_tx, response_rx) = oneshot::channel();
        // Handled in next line instead
        let _ = self.signal_tx.send(SignalRequest::GetPlayersTeam(GetPlayersTeamSignalRequest {
            player_id,
            response_tx
        }));
        response_rx.await.ok()
    }
    async fn run(self: Arc<Self>, mut signal_rx: mpsc::UnboundedReceiver<SignalRequest>) {
        let mut teams = Vec::<Arc<Team>>::new();
        let mut poll_interval = interval(CACHE_DURATION);

        loop {
            tokio::select! {
                _ = poll_interval.tick() => {
                    match self.fetch_teams().await {
                        Ok(new_teams) => {
                            teams = new_teams.into_iter().map(Arc::new).collect();
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
                        SignalRequest::GetPlayersTeam(request) => {
                            let maybe_team = teams.iter().filter(|team| team.player_ids.contains(&request.player_id)).next();
                            if let Some(team) = maybe_team {
                                let _ = request.response_tx.send(team.clone());
                            }
                        }
                    }
                },
            };
        }
    }
    async fn fetch_teams(&self) -> Result<Vec<Team>, reqwest::Error> {
        let teams_url = self.berg_api_base.join("teams").expect("teams is hard-coded and known to be good");
        self.http_client
            .get(teams_url)
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<Team>>()
            .await
    }
}

enum SignalRequest {
    GetPlayersTeam(GetPlayersTeamSignalRequest)
}
struct GetPlayersTeamSignalRequest {
    player_id: Uuid,
    response_tx: oneshot::Sender<Arc<Team>>
}
