pub mod app;
pub mod audit;
pub mod auth;
pub mod config;
pub mod crypto;
pub mod db;
pub mod domain;
pub mod error;
pub mod http;
pub mod integrations;
pub mod jobs;
pub mod types;

use secrecy::ExposeSecret;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub async fn run() -> anyhow::Result<()> {
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
    let pool = db::connect(cfg.database_url.expose_secret()).await?;

    let state  = app::AppState::new(pool, cfg);
    let router = app::build(state.clone());

    if let Err(e) = jobs::start(state).await {
        tracing::warn!(error = %e, "cron scheduler failed to start");
    }

    let addr = std::net::SocketAddr::V4(std::net::SocketAddrV4::new(
        std::net::Ipv4Addr::UNSPECIFIED,
        port,
    ));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("box fraise listening on {addr}");

    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;

    Ok(())
}
