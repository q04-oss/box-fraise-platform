pub mod app;
pub mod domain;
pub mod http;

// Re-export shared infrastructure from the domain crate so that:
// 1. Route files can still use `use crate::error::AppError` unchanged.
// 2. Tests can still import via `box_fraise_server::error::AppError`.
pub use box_fraise_domain::audit;
pub use box_fraise_domain::auth;
pub use box_fraise_domain::config;
pub use box_fraise_domain::crypto;
pub use box_fraise_domain::db;
pub use box_fraise_domain::error;
pub use box_fraise_domain::types;

// Re-export integrations for route files.
pub use box_fraise_integrations as integrations;

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
