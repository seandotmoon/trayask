#[path = "trait.rs"]
pub mod trait_def;

pub mod anthropic;
pub mod gemini;
pub mod openai;

pub use anthropic::AnthropicProvider;
pub use gemini::GeminiProvider;
pub use openai::OpenAiProvider;

pub use trait_def::LlmProvider;
