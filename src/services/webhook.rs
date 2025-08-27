use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use crate::config::{WebhookConfig, WebhookRole};
use rand::seq::IndexedRandom;

#[derive(Clone)]
pub(crate) struct WebhookService {
    signal_tx: mpsc::UnboundedSender<WebhookRequest>
}
impl WebhookService {
    pub(crate) fn new(webhooks: Vec<WebhookConfig>) -> Self {
        let (signal_tx, signal_rx) = mpsc::unbounded_channel();
        tokio::spawn(Self::run(webhooks, signal_rx));
        Self {
            signal_tx 
        }
    }
    async fn run(webhooks: Vec<WebhookConfig>, mut receiver: mpsc::UnboundedReceiver<WebhookRequest>) {
        let webhooks = webhooks.into_iter().map(|config| Arc::new(config)).collect::<Vec<_>>();
        while let Some(request) = receiver.recv().await {
            match request {
                WebhookRequest::RequestWebhook(request) => {
                    let valid_webhooks = webhooks.iter().filter(|webhook| webhook.roles.contains(&request.required_role)).collect::<Vec<_>>();
                    let webhook = valid_webhooks.choose(&mut rand::rng());
                    if let Some(webhook) = webhook {
                        let _ = request.response_tx.send((*webhook).clone());
                    }
                }
            }
        }
    }
    pub(crate) async fn get_webhook(&self, required_role: WebhookRole) -> Option<Arc<WebhookConfig>> {
        let (response_tx, response_rx) = oneshot::channel();
        // Handled in next line instead
        let _ = self.signal_tx.send(WebhookRequest::RequestWebhook(RequestWebhookRequest {
            required_role,
            response_tx
        }));
        response_rx.await.ok()
    }
}

pub(crate) enum WebhookRequest {
    RequestWebhook(RequestWebhookRequest)
}
pub(crate) struct RequestWebhookRequest {
    pub(crate) required_role: WebhookRole,
    pub(crate) response_tx: oneshot::Sender<Arc<WebhookConfig>>
}
