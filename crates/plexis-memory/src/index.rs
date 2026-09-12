use chrono::{DateTime, Utc};
use plexis_core::{MemoryRecord, MemoryScope, MemoryState};
use serde::{Deserialize, Serialize};

/// Configurable weights for hybrid memory scoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringWeights {
    pub similarity: f32,
    pub importance: f32,
    pub recency: f32,
    pub recency_half_life_hours: f32,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            similarity: 0.60,
            importance: 0.25,
            recency: 0.15,
            recency_half_life_hours: 24.0,
        }
    }
}

/// A retrieved memory record augmented with its scoring components.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredMemory {
    pub record: MemoryRecord,
    pub similarity: f32,
    pub importance_score: f32,
    pub recency_score: f32,
    pub total_score: f32,
}

/// Query parameters for searching persistent memories.
#[derive(Debug, Clone)]
pub struct MemoryQuery {
    pub text: Option<String>,
    pub embedding: Option<Vec<f32>>,
    pub scopes: Option<Vec<MemoryScope>>,
    pub scope_id: Option<String>,
    pub states: Option<Vec<MemoryState>>,
    pub tags: Option<Vec<String>>,
    pub min_score: Option<f32>,
    pub limit: usize,
    pub weights: Option<ScoringWeights>,
}

impl Default for MemoryQuery {
    fn default() -> Self {
        Self {
            text: None,
            embedding: None,
            scopes: None,
            scope_id: None,
            states: Some(vec![MemoryState::Active]),
            tags: None,
            min_score: None,
            limit: 10,
            weights: None,
        }
    }
}

impl MemoryQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn with_scopes(mut self, scopes: Vec<MemoryScope>) -> Self {
        self.scopes = Some(scopes);
        self
    }

    pub fn with_scope_id(mut self, scope_id: impl Into<String>) -> Self {
        self.scope_id = Some(scope_id.into());
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = Some(tags);
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    pub fn with_min_score(mut self, min_score: f32) -> Self {
        self.min_score = Some(min_score);
        self
    }
}

/// Computes the hybrid score combining vector similarity, explicit importance, and recency decay.
pub fn compute_hybrid_score(
    record: &MemoryRecord,
    similarity: f32,
    weights: &ScoringWeights,
    now: DateTime<Utc>,
) -> ScoredMemory {
    let normalized_sim = similarity.clamp(0.0, 1.0);
    let normalized_importance = record.importance.clamp(0.0, 1.0);

    // Recency decay based on accessed_at or updated_at
    let ref_time = record.accessed_at.unwrap_or(record.updated_at);
    let elapsed_seconds = now.signed_duration_since(ref_time).num_seconds().max(0);
    let elapsed_hours = elapsed_seconds as f32 / 3600.0;

    let half_life = weights.recency_half_life_hours.max(0.1);
    let recency_score = (2.0f32).powf(-elapsed_hours / half_life).clamp(0.0, 1.0);

    let total_score = weights.similarity * normalized_sim
        + weights.importance * normalized_importance
        + weights.recency * recency_score;

    ScoredMemory {
        record: record.clone(),
        similarity: normalized_sim,
        importance_score: normalized_importance,
        recency_score,
        total_score,
    }
}
