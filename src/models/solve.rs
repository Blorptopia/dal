use serde::Deserialize;
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct Solve {
    #[serde(rename = "solvedAt")]
    pub(crate) solved_at: DateTime<Utc>,
    #[serde(rename = "playerId")]
    pub(crate) player_id: Uuid,
    #[serde(rename = "challengeName")]
    pub(crate) challenge_name: String
}
