pub mod gemini;
pub mod ollama;
pub mod openai;
pub mod scripted;

pub use gemini::GeminiProvider;
pub use ollama::OllamaProvider;
pub use openai::OpenAiProvider;
pub use scripted::ScriptedProvider;
