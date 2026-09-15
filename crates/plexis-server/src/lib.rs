//! Plexis Server
//!
//! API-first HTTP server and operational control interface for Plexis.

pub mod cli;
pub mod git;
pub mod github;
pub mod routes;
pub mod state;
pub mod terminal;

pub use routes::create_router;
pub use state::AppState;
