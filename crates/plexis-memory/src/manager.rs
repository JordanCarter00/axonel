use chrono::Utc;
use plexis_core::{MemoryId, MemoryProvenance, MemoryRecord, MemoryScope, MemoryState};
use plexis_storage::MemoryStore;
use std::sync::Arc;
use tracing::{debug, info};

use crate::embedding::EmbeddingModel;
use crate::error::MemoryError;
use crate::index::{compute_hybrid_score, MemoryQuery, ScoredMemory, ScoringWeights};
use crate::similarity::cosine_similarity;

/// High-level manager coordinating persistent memory storage, embedding generation,
/// lifecycle transitions, and semantic hybrid retrieval.
pub struct MemoryManager<S: MemoryStore, E: EmbeddingModel + ?Sized> {
    store: Arc<S>,
    embedding_model: Arc<E>,
    default_weights: ScoringWeights,
}

impl<S: MemoryStore, E: EmbeddingModel + ?Sized> MemoryManager<S, E> {
    pub fn new(store: Arc<S>, embedding_model: Arc<E>) -> Self {
        Self {
            store,
            embedding_model,
            default_weights: ScoringWeights::default(),
        }
    }

    pub fn with_default_weights(mut self, weights: ScoringWeights) -> Self {
        self.default_weights = weights;
        self
    }

    pub fn store(&self) -> &Arc<S> {
        &self.store
    }

    pub fn embedding_model(&self) -> &Arc<E> {
        &self.embedding_model
    }

    /// Creates and persists a new active memory record.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_memory(
        &self,
        scope: MemoryScope,
        scope_id: Option<String>,
        source: impl Into<String>,
        content: String,
        importance: f32,
        provenance: MemoryProvenance,
        metadata: serde_json::Value,
    ) -> Result<MemoryRecord, MemoryError> {
        let embedding = self.embedding_model.embed(&content).await?;
        let mut record = MemoryRecord::new(scope, source, content);
        record.scope_id = scope_id;
        record.importance = importance.clamp(0.0, 1.0);
        record.provenance = provenance;
        record.metadata = metadata;
        record.embedding = Some(embedding);

        self.store.save_memory(&record).await?;
        info!(
            memory_id = %record.id,
            scope = ?record.scope,
            source = %record.source,
            "Created memory record"
        );
        Ok(record)
    }

    /// Retrieves a single memory record by its ID.
    pub async fn get_memory(&self, id: &MemoryId) -> Result<MemoryRecord, MemoryError> {
        self.store
            .get_memory(id)
            .await?
            .ok_or_else(|| MemoryError::NotFound(id.to_string()))
    }

    /// Updates an existing memory record. If the content changes, regenerates its embedding.
    pub async fn update_memory(
        &self,
        id: &MemoryId,
        content: Option<String>,
        importance: Option<f32>,
        metadata: Option<serde_json::Value>,
    ) -> Result<MemoryRecord, MemoryError> {
        let mut record = self.get_memory(id).await?;

        if let Some(new_content) = content {
            if new_content != record.content {
                let new_embedding = self.embedding_model.embed(&new_content).await?;
                record.embedding = Some(new_embedding);
                record.content = new_content;
            }
        }

        if let Some(new_importance) = importance {
            record.importance = new_importance.clamp(0.0, 1.0);
        }

        if let Some(new_metadata) = metadata {
            record.metadata = new_metadata;
        }

        record.updated_at = Utc::now();

        self.store.update_memory(&record).await?;
        debug!(memory_id = %record.id, "Updated memory record");
        Ok(record)
    }

    /// Supersedes an existing memory with a new replacement memory record.
    pub async fn supersede_memory(
        &self,
        old_id: &MemoryId,
        new_source: impl Into<String>,
        new_content: String,
        new_importance: f32,
        new_provenance: MemoryProvenance,
        new_metadata: serde_json::Value,
    ) -> Result<(MemoryRecord, MemoryRecord), MemoryError> {
        let mut old_record = self.get_memory(old_id).await?;
        let new_record = self
            .create_memory(
                old_record.scope,
                old_record.scope_id.clone(),
                new_source,
                new_content,
                new_importance,
                new_provenance,
                new_metadata,
            )
            .await?;

        old_record.supersede_with(new_record.id);
        self.store.update_memory(&old_record).await?;
        info!(
            old_id = %old_record.id,
            new_id = %new_record.id,
            "Superseded memory record"
        );
        Ok((old_record, new_record))
    }

    /// Archives a memory record.
    pub async fn archive_memory(&self, id: &MemoryId) -> Result<MemoryRecord, MemoryError> {
        let mut record = self.get_memory(id).await?;
        record.archive();
        self.store.update_memory(&record).await?;
        info!(memory_id = %record.id, "Archived memory record");
        Ok(record)
    }

    /// Soft-deletes a memory record.
    pub async fn soft_delete_memory(&self, id: &MemoryId) -> Result<MemoryRecord, MemoryError> {
        let mut record = self.get_memory(id).await?;
        record.soft_delete();
        self.store.update_memory(&record).await?;
        info!(memory_id = %record.id, "Soft-deleted memory record");
        Ok(record)
    }

    /// Records access to a memory record, updating its access count and last accessed timestamp.
    pub async fn record_access(&self, id: &MemoryId) -> Result<(), MemoryError> {
        let mut record = self.get_memory(id).await?;
        record.mark_accessed();
        self.store.update_memory(&record).await?;
        Ok(())
    }

    /// Performs hybrid semantic retrieval across memories.
    pub async fn search_memories(
        &self,
        query: &MemoryQuery,
    ) -> Result<Vec<ScoredMemory>, MemoryError> {
        // 1. Determine query vector
        let query_vector = if let Some(ref vec) = query.embedding {
            Some(vec.clone())
        } else if let Some(ref text) = query.text {
            Some(self.embedding_model.embed(text).await?)
        } else {
            None
        };

        // 2. Fetch candidate memories
        let candidates = if let Some(ref scopes) = query.scopes {
            let mut list = Vec::new();
            for scope in scopes {
                let records = self
                    .store
                    .list_memories_by_scope(*scope, query.scope_id.as_deref())
                    .await?;
                list.extend(records);
            }
            list
        } else {
            self.store
                .list_active_memories(None, query.scope_id.as_deref(), 1000)
                .await?
        };

        let now = Utc::now();
        let weights = query.weights.as_ref().unwrap_or(&self.default_weights);
        let mut scored_results: Vec<ScoredMemory> = Vec::new();

        for record in candidates {
            // Check state filter
            if let Some(ref states) = query.states {
                if !states.contains(&record.state) {
                    continue;
                }
            } else if record.state != MemoryState::Active {
                continue;
            }

            // Check tags filter in metadata if tags are specified
            if let Some(ref query_tags) = query.tags {
                let record_tags: Vec<String> = record
                    .metadata
                    .get("tags")
                    .and_then(|t| t.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();

                let has_all_tags = query_tags.iter().all(|qt| record_tags.contains(qt));
                if !has_all_tags {
                    continue;
                }
            }

            // Compute similarity if query vector and record embedding exist
            let similarity = match (&query_vector, &record.embedding) {
                (Some(q), Some(r)) => cosine_similarity(q, r).unwrap_or(0.0),
                _ => 0.0,
            };

            let scored = compute_hybrid_score(&record, similarity, weights, now);

            // Filter by minimum score threshold
            if let Some(min_score) = query.min_score {
                if scored.total_score < min_score {
                    continue;
                }
            }

            scored_results.push(scored);
        }

        // Sort descending by total score
        scored_results.sort_by(|a, b| {
            b.total_score
                .partial_cmp(&a.total_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Limit results
        scored_results.truncate(query.limit);

        // Update access statistics for retrieved records
        for item in &scored_results {
            let _ = self.record_access(&item.record.id).await;
        }

        debug!(
            matches = scored_results.len(),
            query_limit = query.limit,
            "Completed memory search"
        );

        Ok(scored_results)
    }
}
