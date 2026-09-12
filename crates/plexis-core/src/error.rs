//! Unified error definitions for Plexis Core.

pub use crate::graph::GraphError;
pub use crate::ids::IdParseError;
pub use crate::lease::LeaseError;
pub use crate::state::StateTransitionError;

/// Root error type for domain operations in Plexis Core.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error(transparent)]
    IdParse(#[from] IdParseError),

    #[error(transparent)]
    StateTransition(#[from] StateTransitionError),

    #[error(transparent)]
    Graph(#[from] GraphError),

    #[error(transparent)]
    Lease(#[from] LeaseError),
}
