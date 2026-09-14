use plexis_core::{MemoryProvenance, MemoryScope};
use plexis_memory::embedding::{
    DeterministicEmbeddingModel, EmbeddingModel, PluggableEmbeddingProvider,
};
use plexis_memory::index::MemoryQuery;
use plexis_memory::manager::MemoryManager;
use plexis_storage::SqliteStore;
use std::sync::Arc;

#[tokio::test]
async fn test_pluggable_embedding_and_dynamic_dispatch() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());

    // 1. Initialize MemoryManager with dynamic Arc<dyn EmbeddingModel>
    let local_model: Arc<dyn EmbeddingModel> = Arc::new(DeterministicEmbeddingModel::default_128());
    let manager = MemoryManager::new(store.clone(), local_model);

    let mem1 = manager
        .create_memory(
            MemoryScope::Project,
            Some("proj_1".into()),
            "agent_a",
            "Project constraint: Use PostgreSQL for all durable state transactions".into(),
            0.9,
            MemoryProvenance::new(),
            serde_json::json!({}),
        )
        .await
        .expect("create memory 1");

    assert_eq!(mem1.scope, MemoryScope::Project);

    // 2. Query with semantic hybrid search
    let results = manager
        .search_memories(
            &MemoryQuery::new()
                .with_text("database transactions")
                .with_scopes(vec![MemoryScope::Project]),
        )
        .await
        .expect("search");

    assert_eq!(results.len(), 1);
    assert!(results[0].record.content.contains("PostgreSQL"));

    // 3. Switch to PluggableEmbeddingProvider (e.g. Remote/Custom fallback)
    let pluggable = Arc::new(PluggableEmbeddingProvider::remote_http(
        "https://api.openai.com/v1/embeddings",
        "text-embedding-3-small",
        128,
    ));
    let remote_manager = MemoryManager::new(store.clone(), pluggable);

    let mem2 = remote_manager
        .create_memory(
            MemoryScope::Task,
            Some("task_99".into()),
            "agent_b",
            "Task transient note: Ensure transaction rollback on error".into(),
            0.8,
            MemoryProvenance::new(),
            serde_json::json!({}),
        )
        .await
        .expect("create memory 2");

    assert_eq!(mem2.scope, MemoryScope::Task);

    // 4. Test Scope Isolation: Project-scoped query must NOT return Task-scoped memory
    let project_only = remote_manager
        .search_memories(
            &MemoryQuery::new()
                .with_text("transaction")
                .with_scopes(vec![MemoryScope::Project]),
        )
        .await
        .expect("search project");

    assert_eq!(project_only.len(), 1);
    assert_eq!(project_only[0].record.id, mem1.id);

    // Task-scoped query must NOT return Project-scoped memory
    let task_only = remote_manager
        .search_memories(
            &MemoryQuery::new()
                .with_text("transaction")
                .with_scopes(vec![MemoryScope::Task]),
        )
        .await
        .expect("search task");

    assert_eq!(task_only.len(), 1);
    assert_eq!(task_only[0].record.id, mem2.id);
}
