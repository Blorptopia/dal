mod app;
mod routers;
mod state;
mod services;
mod models;
mod config;
mod repository;

const USER_AGENT: &str = "dal v/1.0.0 (https://github.com/Blorptopia/dal)";

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    if let Err(error) = app::run().await {
        tracing::error!(?error, "failed to run app");
        std::process::exit(1);
    }
}
