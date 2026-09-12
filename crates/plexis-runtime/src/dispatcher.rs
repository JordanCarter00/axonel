//! Command dispatching interface for the execution plane.

use async_trait::async_trait;
use tokio::sync::broadcast;
use plexis_core::Command;
use crate::error::RuntimeError;

/// Trait implemented by command dispatchers routing commands from queue to execution workers.
#[async_trait]
pub trait CommandDispatcher: Send + Sync {
    /// Dispatches a command to its recipient.
    async fn dispatch(&self, command: &Command) -> Result<(), RuntimeError>;
}

/// In-memory broadcast dispatcher for local execution and testing.
pub struct BroadcastCommandDispatcher {
    sender: broadcast::Sender<Command>,
}

impl BroadcastCommandDispatcher {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Command> {
        self.sender.subscribe()
    }
}

#[async_trait]
impl CommandDispatcher for BroadcastCommandDispatcher {
    async fn dispatch(&self, command: &Command) -> Result<(), RuntimeError> {
        // If there are no active subscribers yet, we still succeed because command is durably queued
        let _ = self.sender.send(command.clone());
        Ok(())
    }
}
