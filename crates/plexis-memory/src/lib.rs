pub mod embedding;
pub mod error;
pub mod index;
pub mod manager;
pub mod similarity;

pub use embedding::{DeterministicEmbeddingModel, EmbeddingModel};
pub use error::MemoryError;
pub use index::{compute_hybrid_score, MemoryQuery, ScoredMemory, ScoringWeights};
pub use manager::MemoryManager;
pub use similarity::{cosine_similarity, dot_product, l2_norm, l2_normalize};
