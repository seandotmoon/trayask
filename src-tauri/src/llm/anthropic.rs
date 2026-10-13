use crate::core::orchestrator::map_provider_error;
use crate::core::prompts::build_prompt;
use crate::core::types::{ChatRequest, StreamEvent};
use crate::llm::trait_def::LlmProvider;

/// Extract assistant text from an Anthropic messages SSE JSON payload.
///
/// Handles `type=content_block_delta` with `delta.text`. Returns `None` for
/// `message_start` / `message_delta` / `message_stop` / `ping` shapes,
/// error shapes, or missing/empty text.
pub fn extract_content(json: &serde_json::Value) -> Option<String> {
    if json.get("type").and_then(|t| t.as_str()) != Some("content_block_delta") {
        return None;
    }
    let text = json
        .get("delta")
        .and_then(|d| d.get("text"))
        .and_then(|t| t.as_str())?;
    if text.is_empty() {
        return None;
    }
    Some(text.to_string())
}

/// Parse one SSE line into assistant text.
///
/// Returns `None` for `event:` lines, empty payloads, `[DONE]`, malformed
/// JSON, or payloads without delta text.
pub fn parse_sse_chunk(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("event:") {
        return None;
    }
    let rest = trimmed.strip_prefix("data:")?.trim();
    if rest.is_empty() || rest == "[DONE]" {
        return None;
    }
    let json: serde_json::Value = serde_json::from_str(rest).ok()?;
    extract_content(&json)
}

pub struct AnthropicProvider {
    pub api_key: String,
    pub client: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new(api_key: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { api_key, client }
    }
}

impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    async fn chat_stream(&self, request: ChatRequest) -> Vec<StreamEvent> {
        let system_prompt = build_prompt(request.function, &request.query);
        let url = "https://api.anthropic.com/v1/messages".to_string();
        let body = serde_json::json!({
            "model": request.model,
            "max_tokens": 1024,
            "system": system_prompt,
            "messages": [
                { "role": "user", "content": request.query },
            ],
            "stream": true,
        });

        let resp = match self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_delta_text() {
        let v = serde_json::json!({
            "type": "content_block_delta",
            "delta": { "type": "text_delta", "text": "hello" }
        });
        assert_eq!(extract_content(&v).as_deref(), Some("hello"));
    }

    #[test]
    fn ignores_message_stop() {
        let v = serde_json::json!({ "type": "message_stop" });
        assert_eq!(extract_content(&v), None);
    }

    #[test]
    fn ignores_event_lines() {
        assert_eq!(parse_sse_chunk("event: content_block_delta"), None);
        assert_eq!(parse_sse_chunk(""), None);
    }

    #[test]
    fn ignores_done_marker() {
        assert_eq!(parse_sse_chunk("data: [DONE]"), None);
    }

    #[test]
    fn malformed_json_returns_none() {
        assert_eq!(parse_sse_chunk("data: {not json"), None);
    }

    #[test]
    fn parses_data_line_with_delta() {
        let line =
            r#"data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"hi"}}"#;
        assert_eq!(parse_sse_chunk(line).as_deref(), Some("hi"));
    }
}
