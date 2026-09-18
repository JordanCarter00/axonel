//! Long-horizon autonomous mission coordinator and engine for Plexis.

pub mod budget;
pub mod checkpoint;
pub mod engine;
pub mod liveness;

pub use budget::BudgetTracker;
pub use checkpoint::CheckpointManager;
pub use engine::MissionEngine;
pub use liveness::{LivenessEvaluator, ProgressSnapshot};
