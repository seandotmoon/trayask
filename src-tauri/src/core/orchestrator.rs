use std::sync::atomic::{AtomicU64, Ordering};

use crate::core::types::{ChatRequest, StreamEvent};
use crate::llm::trait_def::LlmProvider;

/// Maximum query length in chars; longer input is truncated.
pub const MAX_QUERY_CHARS: usize = 4000;

/// Char-boundary safe truncation to [`MAX_QUERY_CHARS`] chars.
pub fn truncate_query(q: &str) -> String {
    if q.chars().count() <= MAX_QUERY_CHARS {
        return q.to_string();
    }
    q.chars().take(MAX_QUERY_CHARS).collect()
}

/// Map provider failures to user-facing messages. Never includes secrets.
pub fn map_provider_error(status: Option<u16>, body_snippet: &str, is_timeout: bool) -> String {
    if is_timeout {
        return "Request timed out. Please try again.".to_string();
    }
    match status {
        Some(401) => "Invalid API key (401). Check your OpenAI key.".to_string(),
        Some(429) => "Rate limited (429). Please wait and retry.".to_string(),
        Some(code) => {
            let snippet: String = body_snippet.chars().take(500).collect();
            if snippet.trim().is_empty() {
                format!("Provider error ({code}).")
            } else {
                format!("Provider error ({code}): {snippet}")
            }
        }
        None => {
            let snippet: String = body_snippet.chars().take(500).collect();
            if snippet.trim().is_empty() {
                "Provider error.".to_string()
            } else {
                format!("Provider error: {snippet}")
            }
        }
    }
}

/// Shared single-flight orchestrator: truncates, delegates to the provider,
/// and supports cancellation via a monotonically increasing generation id.
pub struct Orchestrator<P: LlmProvider> {
    provider: P,
    generation: AtomicU64,
}

impl<P: LlmProvider> Orchestrator<P> {
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            generation: AtomicU64::new(0),
        }
    }

    /// Cancel any in-flight run.
    pub fn cancel(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    pub fn current_generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// Run one chat request, invoking `on_event` per event. Stops emitting
    /// if [`cancel`] bumps the generation mid-run. Returns the generation id.
    pub async fn run(&self, mut request: ChatRequest, on_event: impl Fn(StreamEvent)) -> u64 {
        request.query = truncate_query(&request.query);
        let gen = self.generation.load(Ordering::SeqCst);
        let events = self.provider.chat_stream(request).await;
        for ev in events {
            if self.generation.load(Ordering::SeqCst) != gen {
                break;
            }
            on_event(ev);
        }
        gen
    }
}
