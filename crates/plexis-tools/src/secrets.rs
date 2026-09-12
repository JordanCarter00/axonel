use plexis_core::ids::{AgentId, TaskId};
use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

/// Access control specification for a secret.
#[derive(Debug, Clone, Default)]
pub struct SecretAcl {
    pub allowed_agents: Option<HashSet<AgentId>>,
    pub allowed_tasks: Option<HashSet<TaskId>>,
    pub allowed_tools: Option<HashSet<String>>,
}

impl SecretAcl {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_agent(mut self, agent_id: AgentId) -> Self {
        self.allowed_agents
            .get_or_insert_with(HashSet::new)
            .insert(agent_id);
        self
    }

    pub fn with_task(mut self, task_id: TaskId) -> Self {
        self.allowed_tasks
            .get_or_insert_with(HashSet::new)
            .insert(task_id);
        self
    }

    pub fn with_tool(mut self, tool_name: impl Into<String>) -> Self {
        self.allowed_tools
            .get_or_insert_with(HashSet::new)
            .insert(tool_name.into());
        self
    }

    /// Evaluates if the caller is authorized to access the secret.
    pub fn is_authorized(
        &self,
        agent_id: Option<&AgentId>,
        task_id: Option<&TaskId>,
        tool_name: Option<&str>,
    ) -> bool {
        if let Some(ref agents) = self.allowed_agents {
            match agent_id {
                Some(a) if agents.contains(a) => {}
                _ => return false,
            }
        }

        if let Some(ref tasks) = self.allowed_tasks {
            match task_id {
                Some(t) if tasks.contains(t) => {}
                _ => return false,
            }
        }

        if let Some(ref tools) = self.allowed_tools {
            match tool_name {
                Some(tool) if tools.contains(tool) => {}
                _ => return false,
            }
        }

        true
    }
}

/// Trait defining a secure secret store with caller-scoped authorization checks.
pub trait SecretStore: Send + Sync {
    /// Returns a specific secret if the caller has authorization.
    fn get_secret(
        &self,
        name: &str,
        agent_id: Option<&AgentId>,
        task_id: Option<&TaskId>,
        tool_name: Option<&str>,
    ) -> Option<String>;

    /// Returns all secrets authorized for the caller as environment variable pairs.
    fn get_authorized_secrets(
        &self,
        agent_id: Option<&AgentId>,
        task_id: Option<&TaskId>,
        tool_name: Option<&str>,
    ) -> HashMap<String, String>;

    /// Returns all known secret values across the store for global redaction filtering.
    fn all_secret_values(&self) -> Vec<String>;
}

#[allow(dead_code)]
struct SecretEntry {
    name: String,
    value: String,
    acl: SecretAcl,
}

/// In-memory thread-safe secret store with ACL authorization.
#[derive(Default)]
pub struct InMemorySecretStore {
    secrets: RwLock<HashMap<String, SecretEntry>>,
}

impl InMemorySecretStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_secret(&self, name: impl Into<String>, value: impl Into<String>, acl: SecretAcl) {
        let name_str = name.into();
        let value_str = value.into();
        let mut map = self.secrets.write().unwrap();
        map.insert(
            name_str.clone(),
            SecretEntry {
                name: name_str,
                value: value_str,
                acl,
            },
        );
    }
}

impl SecretStore for InMemorySecretStore {
    fn get_secret(
        &self,
        name: &str,
        agent_id: Option<&AgentId>,
        task_id: Option<&TaskId>,
        tool_name: Option<&str>,
    ) -> Option<String> {
        let map = self.secrets.read().unwrap();
        let entry = map.get(name)?;
        if entry.acl.is_authorized(agent_id, task_id, tool_name) {
            Some(entry.value.clone())
        } else {
            None
        }
    }

    fn get_authorized_secrets(
        &self,
        agent_id: Option<&AgentId>,
        task_id: Option<&TaskId>,
        tool_name: Option<&str>,
    ) -> HashMap<String, String> {
        let map = self.secrets.read().unwrap();
        let mut result = HashMap::new();
        for (name, entry) in map.iter() {
            if entry.acl.is_authorized(agent_id, task_id, tool_name) {
                result.insert(name.clone(), entry.value.clone());
            }
        }
        result
    }

    fn all_secret_values(&self) -> Vec<String> {
        let map = self.secrets.read().unwrap();
        map.values().map(|e| e.value.clone()).collect()
    }
}
