use serde::{Deserialize};

use crate::models::solve::Solve;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", tag = "type", content = "message")]
pub(crate) enum WebSocketResponse {
    Solve(Solve),
    #[serde(other)]
    Unknown
}
