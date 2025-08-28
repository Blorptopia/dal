use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Team {
    pub(crate) name: String,
    pub(crate) player_ids: Vec<Uuid>
}
