//! Plexis Server
//!
//! API-first HTTP server and operational control interface for Plexis.

pub mod routes;
pub mod state;

pub use routes::create_router;
pub use state::AppState;
