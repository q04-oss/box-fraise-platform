pub mod app;
pub mod domain;
pub mod error;
pub mod http;

// Re-export shared infrastructure from the domain crate so that:
// 1. Route files can still use `use crate::error::AppError` unchanged.
// 2. Tests can still import via `box_fraise_server::error::AppError`.
pub use box_fraise_domain::audit;
pub use box_fraise_domain::auth;
pub use box_fraise_domain::config;
pub use box_fraise_domain::crypto;
pub use box_fraise_domain::db;
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

    // Fail immediately with a human-readable message if any required variable
    // is absent or invalid. The server must never reach the bind() call with
    // incomplete configuration. process::exit(1) is used rather than panic so
    // the exit code is unambiguous and no Rust backtrace noise is printed.
    let cfg = config::Config::load().unwrap_or_else(|e| {
        tracing::error!("configuration error — server will not start: {e:#}");
        eprintln!("FATAL: {e:#}");
        std::process::exit(1);
    });
    let port = cfg.port;
    let pool = db::connect(cfg.database_url.expose_secret()).await?;

    let state  = app::AppState::new(pool, cfg);

    // Subscribe before building the router so no early events are missed.
    // This consumer logs every event at DEBUG — it proves the bus end-to-end
    // without requiring a real cross-domain consumer yet.
    let mut event_rx = state.event_bus.subscribe();
    tokio::spawn(async move {
        use tokio::sync::broadcast::error::RecvError;
        loop {
            match event_rx.recv().await {
                Ok(event) => tracing::debug!(?event, "domain event"),
                Err(RecvError::Lagged(n)) => {
                    tracing::warn!(dropped = n, "event bus consumer lagged — events dropped");
                }
                Err(RecvError::Closed) => break,
            }
        }
    });

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
