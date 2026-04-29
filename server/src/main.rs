use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod app;
mod auth;
mod config;
mod db;
mod domain;
mod error;
mod http;
mod integrations;
mod jobs;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "box_fraise_server=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cfg  = config::Config::load()?;
    let port = cfg.port;
    let pool = db::connect(&cfg.database_url).await?;

    let state  = app::AppState::new(pool, cfg);
    let router = app::build(state.clone());

    // Background cron jobs — non-fatal if scheduler fails to start.
    if let Err(e) = jobs::start(state).await {
        tracing::warn!(error = %e, "cron scheduler failed to start");
    }

    let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("box fraise listening on {addr}");

    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
