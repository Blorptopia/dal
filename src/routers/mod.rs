use std::sync::Arc;
use axum::routing::get;

use crate::state::AppState;

pub(crate) fn router() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/metrics", get(get_metrics))
}

async fn get_metrics() -> String {
    let lines = Vec::<String>::new();

    lines.join("\n")
}
