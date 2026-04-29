use dotenvy::dotenv;
use std::net::{SocketAddr, SocketAddrV4, Ipv4Addr};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod app;
mod auth;
mod db;
mod error;
mod middleware;
mod routes;

#[tokio::main]
async fn main() {
    dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "box_fraise_server=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let pool   = db::connect().await.expect("failed to connect to database");
    let router = app::build(pool);

    let addr: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 3001));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    info!("box fraise server listening on {addr}");
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
