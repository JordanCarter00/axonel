//! Fake Agent Backend Adapter
//!
//! Bridges the `AgentBackend` abstraction with `LocalAgentHost` to execute
//! the deterministic `plexis-fake-agent` OS executable.

use std::sync::Arc;

use async_trait::async_trait;
use plexis_core::ids::ExecutionId;
use plexis_core::protocol::{ExecutionEvent, ExecutionRequest, ExecutionResult};
use tokio::sync::mpsc;

use super::AgentBackend;
use crate::agent_host::LocalAgentHost;
use crate::error::RuntimeError;

/// Agent backend adapter invoking `plexis-fake-agent` via `LocalAgentHost`.
pub struct FakeAgentBackend {
    host: Arc<LocalAgentHost>,
}

impl FakeAgentBackend {
    pub fn new(host: Arc<LocalAgentHost>) -> Self {
        Self { host }
    }

    pub fn with_default_host() -> Self {
        Self::new(Arc::new(LocalAgentHost::with_default_binary()))
    }

    pub fn host(&self) -> &Arc<LocalAgentHost> {
        &self.host
    }
}

#[async_trait]
impl AgentBackend for FakeAgentBackend {
    fn id(&self) -> &str {
        "fake_agent"
    }

    fn display_name(&self) -> &str {
        "Plexis Fake Coding Agent (Supervised OS Process)"
    }

    fn is_available(&self) -> bool {
        self.host.executable_path().exists()
    }

    async fn execute(
        &self,
        request: &ExecutionRequest,
        event_sender: Option<mpsc::Sender<ExecutionEvent>>,
    ) -> Result<ExecutionResult, RuntimeError> {
        self.host
            .spawn_execution(request.clone(), event_sender)
            .await
    }

    async fn cancel(&self, execution_id: &ExecutionId) -> Result<(), RuntimeError> {
        self.host.cancel_execution(execution_id).await
    }
}
