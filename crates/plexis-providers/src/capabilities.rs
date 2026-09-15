//! Provider capability matrix, model pricing, and routing requirements.

use serde::{Deserialize, Serialize};

/// Reasoning effort tier supported by the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningTier {
    None = 0,
    Low = 1,
    Medium = 2,
    High = 3,
}

/// Token pricing information for cost estimation (USD per 1,000 tokens).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelPricing {
    /// Cost in USD per 1,000 prompt / input tokens.
    pub cost_per_1k_input_tokens: f64,
    /// Cost in USD per 1,000 completion / output tokens.
    pub cost_per_1k_output_tokens: f64,
}

impl Default for ModelPricing {
    fn default() -> Self {
        Self {
            cost_per_1k_input_tokens: 0.0,
            cost_per_1k_output_tokens: 0.0,
        }
    }
}

impl ModelPricing {
    pub fn free() -> Self {
        Self::default()
    }

    pub fn new(input_per_1k: f64, output_per_1k: f64) -> Self {
        Self {
            cost_per_1k_input_tokens: input_per_1k,
            cost_per_1k_output_tokens: output_per_1k,
        }
    }

    /// Computes total cost in USD given input and output token counts.
    pub fn calculate_cost(&self, input_tokens: u64, output_tokens: u64) -> f64 {
        (input_tokens as f64 / 1000.0) * self.cost_per_1k_input_tokens
            + (output_tokens as f64 / 1000.0) * self.cost_per_1k_output_tokens
    }
}

/// Detailed capability metadata for a specific model under a provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    /// Provider name ("ollama", "openai", "gemini", etc.)
    pub provider: String,
    /// Model name or alias ("llama3", "gpt-4o", "gemini-1.5-pro", etc.)
    pub model: String,
    /// Whether the model supports structured tool / function calling.
    pub supports_tools: bool,
    /// Whether the provider supports live token streaming.
    pub supports_streaming: bool,
    /// Maximum context window size in tokens.
    pub context_window_tokens: usize,
    /// Whether the model supports multimodal vision / image inputs.
    pub supports_vision: bool,
    /// Reasoning effort tier.
    pub reasoning_tier: ReasoningTier,
    /// Pricing metadata for input/output tokens.
    pub pricing: ModelPricing,
    /// Whether this model runs locally on the user's workstation.
    pub is_local: bool,
}

impl ProviderCapabilities {
    pub fn for_model(provider: &str, model: &str) -> Self {
        let p_lower = provider.to_lowercase();
        let m_lower = model.to_lowercase();

        if p_lower.contains("ollama") {
            // Local Ollama models
            let (context, tools) = if m_lower.contains("llama")
                || m_lower.contains("qwen")
                || m_lower.contains("mistral")
                || m_lower.contains("coder")
            {
                (8192, true)
            } else {
                (4096, false)
            };
            Self {
                provider: "ollama".to_string(),
                model: model.to_string(),
                supports_tools: tools,
                supports_streaming: true,
                context_window_tokens: context,
                supports_vision: m_lower.contains("llava") || m_lower.contains("vision"),
                reasoning_tier: ReasoningTier::Medium,
                pricing: ModelPricing::free(),
                is_local: true,
            }
        } else if p_lower.contains("gemini") {
            // Google Gemini
            let (tier, context) = if m_lower.contains("pro") {
                (ReasoningTier::High, 1_000_000)
            } else {
                (ReasoningTier::Medium, 500_000)
            };
            Self {
                provider: "gemini".to_string(),
                model: model.to_string(),
                supports_tools: true,
                supports_streaming: true,
                context_window_tokens: context,
                supports_vision: true,
                reasoning_tier: tier,
                pricing: ModelPricing::new(0.00125, 0.005),
                is_local: false,
            }
        } else if p_lower.contains("openai") {
            // OpenAI
            let (tier, context, pricing) = if m_lower.contains("o1") || m_lower.contains("o3") {
                (
                    ReasoningTier::High,
                    200_000,
                    ModelPricing::new(0.015, 0.060),
                )
            } else if m_lower.contains("gpt-4o-mini") {
                (
                    ReasoningTier::Low,
                    128_000,
                    ModelPricing::new(0.00015, 0.00060),
                )
            } else {
                (
                    ReasoningTier::Medium,
                    128_000,
                    ModelPricing::new(0.005, 0.015),
                )
            };
            Self {
                provider: "openai".to_string(),
                model: model.to_string(),
                supports_tools: true,
                supports_streaming: true,
                context_window_tokens: context,
                supports_vision: true,
                reasoning_tier: tier,
                pricing,
                is_local: false,
            }
        } else {
            // Generic / Fallback
            Self {
                provider: provider.to_string(),
                model: model.to_string(),
                supports_tools: false,
                supports_streaming: false,
                context_window_tokens: 4096,
                supports_vision: false,
                reasoning_tier: ReasoningTier::Low,
                pricing: ModelPricing::free(),
                is_local: false,
            }
        }
    }

    /// Checks if this provider capabilities satisfy the given task requirements.
    pub fn satisfies_requirements(
        &self,
        requires_tools: bool,
        required_context_tokens: usize,
        min_reasoning: ReasoningTier,
    ) -> bool {
        if requires_tools && !self.supports_tools {
            return false;
        }
        if self.context_window_tokens < required_context_tokens {
            return false;
        }
        if self.reasoning_tier < min_reasoning {
            return false;
        }
        true
    }
}

/// Returns standard catalog of known models across Ollama, OpenAI, and Gemini.
pub fn standard_capability_matrix() -> Vec<ProviderCapabilities> {
    vec![
        // Local Ollama
        ProviderCapabilities::for_model("ollama", "qwen2.5-coder"),
        ProviderCapabilities::for_model("ollama", "llama3.2"),
        ProviderCapabilities::for_model("ollama", "mistral"),
        // OpenAI
        ProviderCapabilities::for_model("openai", "gpt-4o"),
        ProviderCapabilities::for_model("openai", "gpt-4o-mini"),
        ProviderCapabilities::for_model("openai", "o1-preview"),
        // Google Gemini
        ProviderCapabilities::for_model("gemini", "gemini-1.5-pro"),
        ProviderCapabilities::for_model("gemini", "gemini-1.5-flash"),
    ]
}

/// Selects the best provider capability matching the criteria.
/// If `prefer_local` is true, local providers satisfying requirements are given priority.
pub fn select_best_provider(
    available: &[ProviderCapabilities],
    requires_tools: bool,
    required_context: usize,
    min_reasoning: ReasoningTier,
    prefer_local: bool,
) -> Option<&ProviderCapabilities> {
    let eligible: Vec<&ProviderCapabilities> = available
        .iter()
        .filter(|c| c.satisfies_requirements(requires_tools, required_context, min_reasoning))
        .collect();

    if eligible.is_empty() {
        return None;
    }

    if prefer_local {
        if let Some(local) = eligible.iter().find(|c| c.is_local) {
            return Some(local);
        }
    }

    // Default: prioritize lowest cost per 1k input tokens, then highest context
    let mut sorted = eligible;
    sorted.sort_by(|a, b| {
        a.pricing
            .cost_per_1k_input_tokens
            .partial_cmp(&b.pricing.cost_per_1k_input_tokens)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.context_window_tokens.cmp(&a.context_window_tokens))
    });

    sorted.first().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pricing_calculation() {
        let pricing = ModelPricing::new(0.005, 0.015);
        let cost = pricing.calculate_cost(2000, 1000);
        // 2k input * 0.005 = 0.010, 1k output * 0.015 = 0.015 => total 0.025
        assert!((cost - 0.025).abs() < 1e-6);
    }

    #[test]
    fn test_ollama_local_capabilities() {
        let caps = ProviderCapabilities::for_model("ollama", "qwen2.5-coder");
        assert!(caps.is_local);
        assert!(caps.supports_tools);
        assert_eq!(caps.pricing, ModelPricing::free());
    }

    #[test]
    fn test_openai_and_gemini_capabilities() {
        let oai = ProviderCapabilities::for_model("openai", "gpt-4o");
        assert!(!oai.is_local);
        assert!(oai.supports_tools);
        assert_eq!(oai.context_window_tokens, 128_000);

        let gem = ProviderCapabilities::for_model("gemini", "gemini-1.5-pro");
        assert!(!gem.is_local);
        assert_eq!(gem.context_window_tokens, 1_000_000);
        assert_eq!(gem.reasoning_tier, ReasoningTier::High);
    }

    #[test]
    fn test_selection_fallback_logic() {
        let matrix = standard_capability_matrix();

        // 1. Routine tool task with local preference
        let sel = select_best_provider(&matrix, true, 4000, ReasoningTier::Low, true);
        assert!(sel.is_some());
        assert!(sel.unwrap().is_local);

        // 2. Huge context task requiring 500k tokens
        let sel_huge = select_best_provider(&matrix, true, 500_000, ReasoningTier::Medium, true);
        assert!(sel_huge.is_some());
        assert!(!sel_huge.unwrap().is_local);
        assert_eq!(sel_huge.unwrap().provider, "gemini");
    }
}
