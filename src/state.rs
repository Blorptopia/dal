use std::sync::Arc;

use crate::services::solve_fetcher::SolveFetcherService;

pub(crate) struct AppState {
    pub(crate) solve_fetcher_service: Arc<SolveFetcherService>
}
