use std::env;
use std::net::SocketAddr;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use plexis_server::{create_router, AppState};
use plexis_storage::SqliteStore;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let db_path = env::var("PLEXIS_DB_PATH").unwrap_or_else(|_| "plexis.db".to_string());
    let port = env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(3000);

    info!("Opening authoritative storage at: {}", db_path);
    let store = if db_path == ":memory:" {
        SqliteStore::open_in_memory()?
    } else {
        SqliteStore::open(&db_path)?
    };

    let state = AppState::new(store);
    let app = create_router(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Plexis API server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
