//! Scripted provider adapter for deterministic testing and simulations.

use async_trait::async_trait;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::error::ProviderError;
use crate::traits::Provider;
use crate::types::{CompletionRequest, CompletionResponse};

/// Deterministic scripted provider for unit and integration testing.
pub struct ScriptedProvider {
    id: String,
    responses: Arc<Mutex<VecDeque<Result<CompletionResponse, ProviderError>>>>,
    history: Arc<Mutex<Vec<CompletionRequest>>>,
}

impl ScriptedProvider {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            responses: Arc::new(Mutex::new(VecDeque::new())),
            history: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Enqueues a successful response.
    pub fn queue_response(&self, response: CompletionResponse) {
        self.responses.lock().unwrap().push_back(Ok(response));
    }

    /// Enqueues a failure error from a string.
    pub fn queue_error(&self, error_message: impl Into<String>) {
        self.queue_provider_error(ProviderError::ExecutionError(error_message.into()));
    }

    /// Enqueues an explicit ProviderError.
    pub fn queue_provider_error(&self, error: ProviderError) {
        self.responses.lock().unwrap().push_back(Err(error));
    }

    /// Returns recorded requests made to this provider.
    pub fn requests(&self) -> Vec<CompletionRequest> {
        self.history.lock().unwrap().clone()
    }
}

impl Default for ScriptedProvider {
    fn default() -> Self {
        Self::new("scripted")
    }
}

#[async_trait]
impl Provider for ScriptedProvider {
    fn id(&self) -> &str {
        &self.id
    }

    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        self.history.lock().unwrap().push(request.clone());

        let mut queue = self.responses.lock().unwrap();
        match queue.pop_front() {
            Some(Ok(resp)) => Ok(resp),
            Some(Err(err)) => Err(err),
            None => Err(ProviderError::Unavailable(
                "ScriptedProvider response queue is empty".into(),
            )),
        }
    }
}
