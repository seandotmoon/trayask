use crate::core::orchestrator::map_provider_error;
use crate::core::prompts::build_prompt;
use crate::core::types::{ChatRequest, StreamEvent};
use crate::llm::trait_def::LlmProvider;

/// Extract assistant content from an OpenAI chat-completions SSE JSON payload.
///
/// Accepts `choices[0].delta.content` (streaming) with `choices[0].message.content`
/// (non-streaming) as fallback. Returns `None` for empty choices, missing or
/// empty content, or wrong shapes.
pub fn extract_content(json: &serde_json::Value) -> Option<String> {
    let choice = json.get("choices")?.as_array()?.first()?;
    if let Some(content) = choice
        .get("delta")
        .and_then(|d| d.get("content"))
        .and_then(|c| c.as_str())
    {
        if !content.is_empty() {
            return Some(content.to_string());
        }
        return None;
    }
    if let Some(content) = choice
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
    {
        if !content.is_empty() {
            return Some(content.to_string());
        }
    }
    None
}

/// Parse one SSE line into assistant text.
///
/// Returns `None` for non-`data:` lines, empty payloads, `[DONE]`, malformed
/// JSON, or payloads without content.
pub fn parse_sse_chunk(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("data:")?.trim();
    if rest.is_empty() || rest == "[DONE]" {
        return None;
    }
    let json: serde_json::Value = serde_json::from_str(rest).ok()?;
    extract_content(&json)
}

pub struct OpenAiProvider {
    pub api_key: String,
    pub base_url: String,
    pub client: reqwest::Client,
}

impl OpenAiProvider {
    /// Fixed OpenAI endpoint. No custom base URL for the `openai` provider itself.
    pub fn new(api_key: String) -> Self {
        Self::with_base_url(api_key, "https://api.openai.com/v1".to_string())
    }

    /// Custom-endpoint constructor — only used by the `custom` provider branch
    /// in `commands.rs`. Not exposed for `openai`/`anthropic`/`gemini`.
    pub fn with_base_url(api_key: String, base_url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            api_key,
            base_url,
            client,
        }
    }
}

impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }

    async fn chat_stream(&self, request: ChatRequest) -> Vec<StreamEvent> {
        let system_prompt = build_prompt(request.function, &request.query);
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": request.model,
            "stream": true,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": request.query },
            ],
        });

        let resp = match self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .timeout(std::time::Duration::from_secs(60))
            .json(&body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                let msg = map_provider_error(None, "", e.is_timeout());
                return vec![StreamEvent::Error(msg)];
            }
        };

        let status = resp.status();
        if !status.is_success() {
            let code = Some(status.as_u16());
            let text = resp.text().await.unwrap_or_default();
            let snippet: String = text.chars().take(500).collect();
            let msg = map_provider_error(code, &snippet, false);
            return vec![StreamEvent::Error(msg)];
        }

        // Stream SSE body chunk-by-chunk.
        let mut events: Vec<StreamEvent> = Vec::new();
        let mut buffer = String::new();
        let mut stream = resp.bytes_stream();
        {
            use tokio_stream::StreamExt as _;
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        // Drain complete lines, keep partial tail buffered.
                        while let Some(pos) = buffer.find('\n') {
                            let line: String = buffer.drain(..=pos).collect();
                            if let Some(text) = parse_sse_chunk(&line) {
                                events.push(StreamEvent::Token(text));
                            }
                        }
                    }
                    Err(e) => {
                        let msg = map_provider_error(
                            None,
                            &e.to_string(),
                            e.is_timeout() || e.is_connect(),
                        );
                        events.push(StreamEvent::Error(msg));
                        return events;
                    }
                }
            }
        }
        // Flush any trailing line without a newline.
        if !buffer.trim().is_empty() {
            if let Some(text) = parse_sse_chunk(&buffer) {
                events.push(StreamEvent::Token(text));
            }
        }
        events.push(StreamEvent::Done);
        events
    }
}
