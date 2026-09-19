use plexis_core::ids::WorkspaceId;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Manages intra-process concurrency locks per workspace to ensure serialized
/// repository operations (such as checkout and merge) across concurrent requests.
#[derive(Default, Clone)]
pub struct WorkspaceLockManager {
    locks: Arc<tokio::sync::RwLock<HashMap<WorkspaceId, Arc<Mutex<()>>>>>,
}

impl WorkspaceLockManager {
    pub fn new() -> Self {
        Self {
            locks: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }

    /// Acquires the lock for a given workspace.
    pub async fn get_lock(&self, workspace_id: &WorkspaceId) -> Arc<Mutex<()>> {
        // Fast path with read lock
        {
            let read = self.locks.read().await;
            if let Some(lock) = read.get(workspace_id) {
                return lock.clone();
            }
        }
        // Upgrade to write lock
        let mut write = self.locks.write().await;
        write
            .entry(*workspace_id)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }
}
