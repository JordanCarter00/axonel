use plexis_core::{MemoryProvenance, MemoryScope, MemoryState};
use plexis_memory::{DeterministicEmbeddingModel, MemoryManager, MemoryQuery};
use plexis_storage::SqliteStore;
use std::sync::Arc;

#[tokio::test]
async fn test_memory_lifecycle_and_semantic_retrieval() {
    let store = Arc::new(SqliteStore::open_in_memory().expect("open sqlite"));
    let embedding_model = Arc::new(DeterministicEmbeddingModel::default_128());
    let manager = MemoryManager::new(store.clone(), embedding_model);

    // 1. Create memories across different scopes
    let provenance = MemoryProvenance::default();

    let mem1 = manager
        .create_memory(
            MemoryScope::Project,
            Some("proj_1".to_string()),
            "system_init",
            "We use SQLite with WAL mode for durable single-node execution and relational querying."
                .to_string(),
            0.9,
            provenance.clone(),
            serde_json::json!({
                "tags": ["database", "sqlite"],
                "engine": "sqlite"
            }),
        )
        .await
        .expect("create mem1");

    let mem2 = manager
        .create_memory(
            MemoryScope::Project,
            Some("proj_1".to_string()),
            "system_init",
            "Authentication uses Ed25519 asymmetric signed session tokens for agent authorization."
                .to_string(),
            0.8,
            provenance.clone(),
            serde_json::json!({
                "tags": ["auth", "security"],
                "algorithm": "ed25519"
            }),
        )
        .await
        .expect("create mem2");

    let mem3 = manager
        .create_memory(
            MemoryScope::Task,
            Some("task_123".to_string()),
            "task_execution",
            "Rust borrow checker error: Cannot borrow value as mutable because it is also borrowed as immutable."
                .to_string(),
            0.5,
            provenance.clone(),
            serde_json::json!({
                "tags": ["rust", "compiler"],
                "error_code": "E0502"
            }),
        )
        .await
        .expect("create mem3");

    // 2. Semantic search for database architecture
    let query_db = MemoryQuery::new()
        .with_text("SQLite WAL mode relational database execution")
        .with_scopes(vec![MemoryScope::Project])
        .with_limit(5);

    let results = manager.search_memories(&query_db).await.expect("search db");
    println!(
        "Search result 0: sim={}, total={}",
        results[0].similarity, results[0].total_score
    );
    assert_eq!(results[0].record.id, mem1.id);
    assert!(results[0].similarity > 0.1);
    assert!(results[0].total_score > 0.3);

    // Verify access count was incremented
    let fetched_mem1 = manager.get_memory(&mem1.id).await.expect("get mem1");
    assert_eq!(fetched_mem1.access_count, 1);
    assert!(fetched_mem1.accessed_at.is_some());

    // 3. Update memory content and verify embedding update
    let old_emb = fetched_mem1.embedding.clone().unwrap();
    let updated = manager
        .update_memory(
            &mem1.id,
            Some("We use SQLite with strict typing and WAL mode for high concurrency.".to_string()),
            Some(0.95),
            None,
        )
        .await
        .expect("update mem1");
    assert_ne!(updated.embedding.unwrap(), old_emb);

    // 4. Supersede memory
    let (superseded, new_mem) = manager
        .supersede_memory(
            &mem2.id,
            "architecture_revised",
            "Authentication migrated to mutual TLS and Ed25519 signed claims.".to_string(),
            0.85,
            provenance.clone(),
            serde_json::json!({
                "tags": ["auth", "tls"],
                "tls": true
            }),
        )
        .await
        .expect("supersede mem2");

    assert_eq!(superseded.state, MemoryState::Superseded);
    assert_eq!(superseded.superseded_by, Some(new_mem.id));

    // Active search should no longer return mem2
    let active_auth_results = manager
        .search_memories(
            &MemoryQuery::new()
                .with_text("Authentication signed session tokens")
                .with_scopes(vec![MemoryScope::Project]),
        )
        .await
        .expect("search auth");

    let ids: Vec<_> = active_auth_results.iter().map(|r| &r.record.id).collect();
    assert!(!ids.contains(&&mem2.id));
    assert!(ids.contains(&&new_mem.id));

    // 5. Soft-delete and archive
    let archived = manager
        .archive_memory(&mem3.id)
        .await
        .expect("archive mem3");
    assert_eq!(archived.state, MemoryState::Archived);

    let active_search = manager
        .search_memories(&MemoryQuery::new().with_text("borrow checker compiler error"))
        .await
        .expect("search after archive");
    assert!(!active_search.iter().any(|r| r.record.id == mem3.id));

    // Also verify archived search retrieves it
    let archived_search = manager
        .search_memories(
            &MemoryQuery::new()
                .with_text("borrow checker compiler error")
                .with_scopes(vec![MemoryScope::Task]),
        )
        .await
        .expect("search archived with scope");
    // Default search filters for active only
    assert!(!archived_search.iter().any(|r| r.record.id == mem3.id));

    let mut archived_query = MemoryQuery::new()
        .with_text("borrow checker compiler error")
        .with_scopes(vec![MemoryScope::Task]);
    archived_query.states = Some(vec![MemoryState::Archived]);
    let archived_explicit = manager
        .search_memories(&archived_query)
        .await
        .expect("search explicit archived");
    assert!(archived_explicit.iter().any(|r| r.record.id == mem3.id));
}
