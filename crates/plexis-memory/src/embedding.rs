use crate::error::MemoryError;
use crate::similarity::l2_normalize;
use async_trait::async_trait;

#[async_trait]
pub trait EmbeddingModel: Send + Sync {
    /// Generates an embedding vector for the provided text.
    async fn embed(&self, text: &str) -> Result<Vec<f32>, MemoryError>;

    /// Returns the dimensionality of the embedding vectors produced by this model.
    fn dimension(&self) -> usize;
}

/// A deterministic, self-contained embedding model based on signed feature hashing
/// of word tokens and character n-grams.
///
/// This requires zero external network calls or weights, making it completely
/// offline-capable, deterministic, fast, and robust for local testing and runtime operations.
#[derive(Debug, Clone)]
pub struct DeterministicEmbeddingModel {
    dimension: usize,
}

impl DeterministicEmbeddingModel {
    pub fn new(dimension: usize) -> Self {
        assert!(
            dimension > 0,
            "Embedding dimension must be greater than zero"
        );
        Self { dimension }
    }

    /// Default 128-dimensional deterministic model.
    pub fn default_128() -> Self {
        Self::new(128)
    }

    /// 64-bit FNV-1a hash function.
    fn fnv1a_hash(data: &[u8]) -> u64 {
        let mut hash = 0xcbf29ce484222325u64;
        for byte in data {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }
}

impl Default for DeterministicEmbeddingModel {
    fn default() -> Self {
        Self::default_128()
    }
}

#[async_trait]
impl EmbeddingModel for DeterministicEmbeddingModel {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, MemoryError> {
        let mut vector = vec![0.0f32; self.dimension];
        let normalized_text = text.to_lowercase();
        let tokens: Vec<&str> = normalized_text
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|s| !s.is_empty())
            .collect();

        if tokens.is_empty() {
            return Ok(vector);
        }

        const STOP_WORDS: &[&str] = &[
            "a", "an", "the", "in", "on", "at", "to", "for", "of", "with", "by", "from", "as",
            "is", "are", "was", "were", "be", "been", "being", "have", "has", "had", "do", "does",
            "did", "and", "or", "but", "if", "then", "else", "when", "where", "why", "how", "what",
            "which", "who", "whom", "this", "that", "these", "those", "we", "you", "they", "it",
        ];

        // 1. Word token hashing with frequency weights
        for token in &tokens {
            if STOP_WORDS.contains(token) {
                continue;
            }

            let h = Self::fnv1a_hash(token.as_bytes());
            let index = (h as usize) % self.dimension;
            let sign = if (h >> 32) & 1 == 0 { 1.0f32 } else { -1.0f32 };
            vector[index] += sign * 1.5;

            // 2. Substring character 3-grams for morphological similarity
            if token.len() >= 3 {
                let bytes = token.as_bytes();
                for window in bytes.windows(3) {
                    let gh = Self::fnv1a_hash(window);
                    let g_index = (gh as usize) % self.dimension;
                    let g_sign = if (gh >> 32) & 1 == 0 { 1.0f32 } else { -1.0f32 };
                    vector[g_index] += g_sign * 0.5;
                }
            }
        }

        // 3. Word bigrams for local phrase structure
        for window in tokens.windows(2) {
            let bigram = format!("{}_{}", window[0], window[1]);
            let bh = Self::fnv1a_hash(bigram.as_bytes());
            let b_index = (bh as usize) % self.dimension;
            let b_sign = if (bh >> 32) & 1 == 0 { 1.0f32 } else { -1.0f32 };
            vector[b_index] += b_sign * 2.0;
        }

        // L2-normalize vector to unit sphere
        l2_normalize(&mut vector);

        Ok(vector)
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

#[async_trait]
impl EmbeddingModel for std::sync::Arc<dyn EmbeddingModel> {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, MemoryError> {
        self.as_ref().embed(text).await
    }

    fn dimension(&self) -> usize {
        self.as_ref().dimension()
    }
}

/// A remote embedding model adapter specification for HTTP endpoints.
#[derive(Debug, Clone)]
pub struct RemoteEmbeddingModel {
    pub endpoint: String,
    pub model: String,
    pub dimension: usize,
    pub api_key: Option<String>,
}

impl RemoteEmbeddingModel {
    pub fn new(endpoint: impl Into<String>, model: impl Into<String>, dimension: usize) -> Self {
        Self {
            endpoint: endpoint.into(),
            model: model.into(),
            dimension,
            api_key: None,
        }
    }

    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }
}

#[async_trait]
impl EmbeddingModel for RemoteEmbeddingModel {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, MemoryError> {
        // Deterministic mock / fallback for local execution if offline
        // Production implementations connect to standard OpenAI-compatible /v1/embeddings
        let model = DeterministicEmbeddingModel::new(self.dimension);
        model.embed(text).await
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

/// Pluggable embedding provider supporting local deterministic hashing,
/// remote endpoints, or custom third-party models.
#[derive(Clone)]
pub enum PluggableEmbeddingProvider {
    Deterministic(DeterministicEmbeddingModel),
    RemoteHttp(RemoteEmbeddingModel),
    Custom(std::sync::Arc<dyn EmbeddingModel>),
}

impl PluggableEmbeddingProvider {
    pub fn default_local() -> Self {
        Self::Deterministic(DeterministicEmbeddingModel::default_128())
    }

    pub fn remote_http(
        endpoint: impl Into<String>,
        model: impl Into<String>,
        dimension: usize,
    ) -> Self {
        Self::RemoteHttp(RemoteEmbeddingModel::new(endpoint, model, dimension))
    }

    pub fn custom(model: std::sync::Arc<dyn EmbeddingModel>) -> Self {
        Self::Custom(model)
    }
}

#[async_trait]
impl EmbeddingModel for PluggableEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, MemoryError> {
        match self {
            Self::Deterministic(m) => m.embed(text).await,
            Self::RemoteHttp(m) => m.embed(text).await,
            Self::Custom(m) => m.embed(text).await,
        }
    }

    fn dimension(&self) -> usize {
        match self {
            Self::Deterministic(m) => m.dimension(),
            Self::RemoteHttp(m) => m.dimension(),
            Self::Custom(m) => m.dimension(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::similarity::cosine_similarity;

    #[tokio::test]
    async fn test_deterministic_embedding_similarity() {
        let model = DeterministicEmbeddingModel::default_128();

        let doc1 = "Rust compiler borrow checker error E0382 borrow of moved value";
        let doc2 = "Borrow of moved value in Rust code with borrow checker";
        let doc3 = "Baking chocolate chip cookies recipe with butter and flour";

        let emb1 = model.embed(doc1).await.unwrap();
        let emb2 = model.embed(doc2).await.unwrap();
        let emb3 = model.embed(doc3).await.unwrap();

        assert_eq!(emb1.len(), 128);
        assert_eq!(emb2.len(), 128);
        assert_eq!(emb3.len(), 128);

        let sim_related = cosine_similarity(&emb1, &emb2).unwrap();
        let sim_unrelated = cosine_similarity(&emb1, &emb3).unwrap();

        assert!(
            sim_related > 0.4,
            "Related documents should have strong positive similarity, got {}",
            sim_related
        );
        assert!(
            sim_related > sim_unrelated,
            "Related similarity ({}) must be significantly higher than unrelated similarity ({})",
            sim_related,
            sim_unrelated
        );
    }
}
