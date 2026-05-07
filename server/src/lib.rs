pub mod app;
pub mod domain;
pub mod error;
pub mod events;
pub mod http;
pub mod openapi;

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

    // Hardening §4 — observability bootstrap.
    //
    // The Sentry layer is added to the subscriber unconditionally; it's a
    // no-op until `sentry::init` runs below (after config loads). `sentry::init`
    // belongs as early as possible, but Sentry needs the DSN, and the DSN
    // lives on `Config` — so config loads, then Sentry inits, then the
    // observability summary logs.
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "box_fraise_server=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .with(sentry_tracing::layer())
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

    // Sentry guard must outlive the program — its Drop flushes pending events.
    // Hold it in `_sentry_guard` so it lives through `axum::serve(...).await`.
    let _sentry_guard = cfg.sentry_dsn.as_ref().map(|dsn| {
        let g = sentry::init((
            dsn.as_str(),
            sentry::ClientOptions {
                release:            sentry::release_name!(),
                traces_sample_rate: 0.1,
                ..Default::default()
            },
        ));
        info!("Sentry initialised");
        g
    });
    if cfg.sentry_dsn.is_none() {
        tracing::warn!("Sentry disabled — SENTRY_DSN not set");
    }

    // One-line summary of which observability + hardening features came
    // online for this boot.
    info!(
        sentry_enabled  = cfg.sentry_dsn.is_some(),
        storage_enabled = cfg.spaces_access_key.is_some(),
        rls_enforcement = cfg.app_user_database_url.is_some(),
        "Server configuration summary",
    );
    let port = cfg.port;
    // Hardening Section 2c — connection role routing.
    // If APP_USER_DATABASE_URL is set, the main pool connects as the
    // non-superuser `app_user` role, and the RLS policies in migration
    // 002 actually enforce. Otherwise we fall back to DATABASE_URL,
    // which in dev/test is the `fraise` superuser (BYPASSRLS).
    let (pool_url, rls_active) = match cfg.app_user_database_url.as_ref() {
        Some(url) => (url.expose_secret().to_owned(), true),
        None      => (cfg.database_url.expose_secret().to_owned(), false),
    };
    if rls_active {
        info!("RLS enforcement active — connecting as app_user");
    } else {
        tracing::warn!(
            "RLS enforcement inactive — connecting as superuser. \
             Set APP_USER_DATABASE_URL for production.",
        );
    }
    let pool = db::connect(&pool_url).await?;

    // Seed BFIP Section 15 defaults — ON CONFLICT DO NOTHING so custom values are preserved.
    if let Err(e) = box_fraise_domain::domain::platform_configuration::service::initialize_defaults(&pool).await {
        tracing::warn!(error = ?e, "platform configuration seed failed — defaults may be missing");
    } else {
        tracing::info!("Platform configuration defaults initialized");
    }

    // Hardening Section 3 — build the storage client when SPACES_* are set.
    let storage_client = match cfg.storage_config() {
        Some(scfg) => {
            match box_fraise_integrations::storage::StorageClient::new(scfg).await {
                Ok(client) => {
                    info!("Storage client initialised — DigitalOcean Spaces");
                    Some(std::sync::Arc::new(client))
                }
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        "Storage client initialisation failed — feature disabled"
                    );
                    None
                }
            }
        }
        None => {
            tracing::warn!(
                "Storage disabled — SPACES_* not configured. \
                 Evidence upload + presign endpoints will return 503."
            );
            None
        }
    };

    // Hardening §4 — Prometheus pair lives behind a OnceLock in `app.rs`
    // because the metrics-exporter-prometheus global recorder can only be
    // registered once per process. Both `AppState::new` and `app::build`
    // pull from the same memoised pair.
    let state  = app::AppState::new(pool, cfg, storage_client);

    // Subscribe before building the router so no early events are missed.
    let mut event_rx = state.event_bus.subscribe();
    let event_pool   = state.db.clone();
    let event_http   = state.http.clone();
    tokio::spawn(async move {
        use tokio::sync::broadcast::error::RecvError;
        loop {
            match event_rx.recv().await {
                Ok(event) => {
                    events::handle(&event_pool, &event_http, event).await;
                }
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
    .with_graceful_shutdown(async {
        tokio::signal::ctrl_c().await.ok();
    })
    .await?;

    Ok(())
}
