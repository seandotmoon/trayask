use crate::core::types::ChatRequest;
use crate::core::types::StreamEvent;

/// Shared interface for all LLM providers.
///
/// Native async-fn-in-trait (stable since Rust 1.75; toolchain here is 1.90).
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &str;

    fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> impl std::future::Future<Output = Vec<StreamEvent>> + Send;
}
