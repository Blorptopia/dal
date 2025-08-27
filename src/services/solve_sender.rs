use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use crate::config::WebhookRole;
use crate::models::solve::Solve;
use crate::repository::Repository;
use crate::services::webhook::WebhookService;
use tokio::sync::mpsc;
use tokio::time::interval;
use twilight_model::channel::message::AllowedMentions;

pub(crate) struct SolveSenderService {
    failed_to_send_count: AtomicU32,
    failed_to_process_count: AtomicU32,
    webhook_service: Arc<WebhookService>,
    repository: Arc<Repository>,
    twilight_client: Arc<twilight_http::Client>
}
impl SolveSenderService {
    pub(crate) fn new(webhook_service: Arc<WebhookService>, repository: Arc<Repository>) -> Arc<SolveSenderService> {
        let twilight_client = twilight_http::Client::builder().default_allowed_mentions(AllowedMentions::default()).build();
        let twilight_client = Arc::new(twilight_client);
        let instance = Arc::new(Self {
            failed_to_send_count: AtomicU32::default(),
            failed_to_process_count: AtomicU32::default(),
            webhook_service,
            repository,
            twilight_client
        });
        instance
    }
    pub(crate) fn start(self: Arc<Self>, receiver: mpsc::UnboundedReceiver<Solve>) {
        tokio::spawn({
            let instance = self;
            async move {
                instance.run(receiver).await
            }
        });
    }
    async fn run(self: Arc<Self>, mut receiver: mpsc::UnboundedReceiver<Solve>) {
        let mut challenge_to_solve_channels = HashMap::<String, mpsc::UnboundedSender<Solve>>::new();
        loop {
            let solve = receiver.recv().await.expect("solve channel closed");
            
            let maybe_solve_channel = challenge_to_solve_channels.get(&solve.challenge_name);
            match maybe_solve_channel {
                Some(solve_channel) => {
                    if let Err(mpsc::error::SendError(solve)) = solve_channel.send(solve) {
                        let (solve_channel_tx, solve_channel_rx) = mpsc::unbounded_channel();
                        tokio::spawn({
                            let instance = self.clone();
                            async move {
                                instance.challenge_sender(solve_channel_rx).await
                            }
                        });
                        challenge_to_solve_channels.insert(solve.challenge_name.clone(), solve_channel_tx.clone());
                        if solve_channel_tx.send(solve).is_err() {
                            self.failed_to_send_count.fetch_add(1, Ordering::SeqCst);
                            tracing::error!("failed to send solve due to solve_channel");
                            continue;
                        }
                    }
                },
                None => {
                    let (solve_channel_tx, solve_channel_rx) = mpsc::unbounded_channel();
                    tokio::spawn({
                        let instance = self.clone();
                        async move {
                            instance.challenge_sender(solve_channel_rx).await
                        }
                    });
                    challenge_to_solve_channels.insert(solve.challenge_name.clone(), solve_channel_tx.clone());
                    if solve_channel_tx.send(solve).is_err() {
                        self.failed_to_send_count.fetch_add(1, Ordering::SeqCst);
                        tracing::error!("failed to send solve due to solve_channel");
                        continue;
                    }
                }
            }
        }
    }
    async fn challenge_sender(self: Arc<Self>, mut receiver: mpsc::UnboundedReceiver<Solve>) {
        // If any error happens during execution we refuse to give out any first bloods until next
        // restart - this is to avoid giving out false first bloods
        let mut has_been_solved = false;
        loop {
            let solve = tokio::select! {
                maybe_solve = receiver.recv() => {
                    match maybe_solve {
                        Some(solve) => solve,
                        None => break
                    }
                },
                _ = tokio::time::sleep(Duration::from_secs(60 * 10)) => {
                    break;
                }
            };
            
            let mut retry_interval = interval(Duration::from_secs(10));
            loop {
                retry_interval.tick().await;
                let is_first_blood = if has_been_solved {
                    false
                } else {
                    match self.repository.has_been_solved(&solve.challenge_name).await {
                        Ok(has_been_solved) => !has_been_solved,
                        Err(error) => {
                            tracing::error!(?error, "failed to check if challenge is solved");
                            self.failed_to_process_count.fetch_add(1, Ordering::SeqCst);
                            continue;
                        }
                    }
                };

                let has_sent_solve = match self.repository.has_sent_solve(&solve.challenge_name, &solve.player_id).await {
                    Ok(has_sent_solve) => has_sent_solve,
                    Err(error) => {
                        tracing::error!(?error, "failed to check if player has submitted solve");
                        self.failed_to_process_count.fetch_add(1, Ordering::SeqCst);
                        continue;
                    }
                };

                if has_sent_solve {
                    break;
                }
                tracing::debug!(?is_first_blood, ?has_been_solved, "sending notification for {}", &solve.challenge_name);
                if let Err(error) = self.send_solve_notification(&solve, is_first_blood).await {
                    tracing::error!(?error, "failed to send solve notification");
                    self.failed_to_send_count.fetch_add(1, Ordering::SeqCst);
                    continue;
                }
                if let Err(error) = self.repository.mark_challenge_as_solved(&solve.challenge_name, &solve.player_id).await {
                    tracing::error!(?error, "failed to mark challenge as solved");
                    self.failed_to_process_count.fetch_add(1, Ordering::SeqCst);
                    continue;
                };

                has_been_solved = true;
                break;
            }
        }
    }
    async fn send_solve_notification(&self, solve: &Solve, is_first_blood: bool) -> Result<(), NotificationSendError> {
        let user_name = &solve.player_id;
        let challenge_name = &solve.challenge_name;
        let team_name = "TODO";

        let first_blood_message = format!("🩸 **{user_name}** from **{team_name}** solved **{challenge_name}**");
        let solve_message = format!("⭐ **{user_name}** from **{team_name}** solved **{challenge_name}**");

        if is_first_blood {
            let first_blood_webhook = self.webhook_service.get_webhook(WebhookRole::FirstBlood).await;
            if let Some(first_blood_webhook) = &first_blood_webhook {
                self.twilight_client
                    .execute_webhook(first_blood_webhook.id, &first_blood_webhook.token)
                    .content(&first_blood_message)
                    .await
                    .map_err(NotificationSendError::WebhookExecutionError)?;
            }
            let is_first_blood_also_solve_webhook = first_blood_webhook.map(|config| config.roles.contains(&WebhookRole::Solve)).unwrap_or(false);
            if !is_first_blood_also_solve_webhook {
                let solve_webhook = self.webhook_service.get_webhook(WebhookRole::Solve).await;
                if let Some(solve_webhook) = solve_webhook {
                    self.twilight_client
                        .execute_webhook(solve_webhook.id, &solve_webhook.token)
                        .content(&solve_message)
                        .await
                        .map_err(NotificationSendError::WebhookExecutionError)?;
                }
            }
        } else {
            let solve_webhook = self.webhook_service.get_webhook(WebhookRole::Solve).await;
            if let Some(solve_webhook) = solve_webhook {
                self.twilight_client
                    .execute_webhook(solve_webhook.id, &solve_webhook.token)
                    .content(&solve_message)
                    .await
                    .map_err(NotificationSendError::WebhookExecutionError)?;
            }
        }
        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
enum NotificationSendError {
    #[error("webhook execution error")]
    WebhookExecutionError(twilight_http::Error)
}
