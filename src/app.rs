use std::sync::Arc;
use crate::config::Config;
use crate::repository::Repository;
use crate::services::solve_fetcher::SolveFetcherService;
use crate::services::solve_sender::SolveSenderService;
use crate::services::webhook::WebhookService;
use crate::state::AppState;
use crate::USER_AGENT;
use tokio::sync::mpsc;
use http::header::USER_AGENT as USER_AGENT_HEADER_KEY;
use http::{HeaderMap, HeaderValue};
use tokio::net::TcpListener;

pub(crate) async fn run() -> Result<(), AppRunError> {
    let config = {
        let raw_config = tokio::fs::read_to_string("config.toml").await.map_err(AppRunError::FailedToReadConfig)?;
        toml::from_str::<Config>(&raw_config).map_err(AppRunError::ConfigParseError)?
    };
    let repository = Arc::new(Repository::new(&config.postgres_url).await.map_err(AppRunError::PostgresConnectionError)?);
    let http_client = {
        let mut default_headers = HeaderMap::new();
        default_headers.insert(USER_AGENT_HEADER_KEY, HeaderValue::from_static(USER_AGENT));
        reqwest::Client::builder()
            .default_headers(default_headers)
            .build()
            .expect("all options is known to be good")
    };
    
    // Services
    let (solve_tx, solve_rx) = mpsc::unbounded_channel();
    let webhook_service = Arc::new(WebhookService::new(config.webhooks.clone()));
    let solve_fetcher_service = SolveFetcherService::new(config.berg_api_base.clone(), http_client, solve_tx);
    let solve_sender_service = SolveSenderService::new(webhook_service.clone(), repository.clone());
    solve_fetcher_service.clone().start();
    solve_sender_service.clone().start(solve_rx);




    let state = Arc::new(AppState {
        solve_fetcher_service
    });
    let service = crate::routers::router().with_state(state);
    let listener = TcpListener::bind(("0.0.0.0", 5000)).await.map_err(AppRunError::BindError)?;

    axum::serve(listener, service).await.map_err(AppRunError::ServeError)?;

    Ok(())
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum AppRunError {
    #[error("failed to bind")]
    BindError(std::io::Error),
    #[error("failed to serve")]
    ServeError(std::io::Error),
    #[error("failed to read config")]
    FailedToReadConfig(std::io::Error),
    #[error("failed to parse config")]
    ConfigParseError(toml::de::Error),
    #[error("failed to connect to postgres")]
    PostgresConnectionError(sqlx::Error)
}
