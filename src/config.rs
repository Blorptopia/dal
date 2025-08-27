use std::collections::HashSet;

use url::Url;
use serde::Deserialize;
use twilight_model::id::Id as Snowflake;
use twilight_model::id::marker::WebhookMarker;

#[derive(Deserialize, Clone)]
pub(crate) struct Config {
    pub(crate) berg_api_base: Url,
    pub(crate) postgres_url: String,
    pub(crate) webhooks: Vec<WebhookConfig>
}
#[derive(Deserialize, Clone)]
pub(crate) struct WebhookConfig {
    pub(crate) id: Snowflake<WebhookMarker>,
    pub(crate) token: String,
    pub(crate) roles: HashSet<WebhookRole>
}
#[derive(Deserialize, PartialEq, Eq, Hash, Clone)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WebhookRole {
    FirstBlood,
    Solve
}
