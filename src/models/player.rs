use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct Player {
    pub(crate) id: Uuid,
    pub(crate) name: String
}
