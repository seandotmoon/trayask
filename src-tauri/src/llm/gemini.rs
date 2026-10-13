use crate::core::orchestrator::map_provider_error;
use crate::core::prompts::build_prompt;
use crate::core::types::{ChatRequest, StreamEvent};
use crate::llm::trait_def::LlmProvider;

/// Extract assistant text from a Gemini `streamGenerateContent` SSE JSON payload.
///
/// Concatenates non-empty `.text` of `candidates[0].content.parts[]`.
/// Returns `None` when candidates/parts are missing or all texts are empty.
pub fn extract_content(json: &serde_json::Value) -> Option<String> {
    let parts = json
        .get("candidates")?
        .as_array()?
        .first()?
        .get("content")?
        .get("parts")?
        .as_array()?;
    let mut out = String::new();
    for part in parts {
        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
            if !text.is_empty() {
                out.push_str(text);
            }
        }
    }
    if out.is_empty() {
        return None;
    }
    Some(out)
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

pub struct GeminiProvider {
    pub api_key: String,
    pub client: reqwest::Client,
}

impl GeminiProvider {
    pub fn new(api_key: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { api_key, client }
    }
}

impl LlmProvider for GeminiProvider {
    fn name(&self) -> &str {
        "gemini"
    }

    async fn chat_stream(&self, request: ChatRequest) -> Vec<StreamEvent> {
        let system_prompt = build_prompt(request.function, &request.query);
        let model = if request.model.trim().is_empty() {
            "gemini-1.5-flash".to_string()
        } else {
            request.model.clone()
        };
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
            model,
            self.api_key
        );
        let body = serde_json::json!({
            "systemInstruction": { "parts": [{ "text": system_prompt }] },
            "contents": [
                { "role": "user", "parts": [{ "text": request.query }] },
            ],
        });

        let resp = match self
            .client
            .post(&url)
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
    fn extracts_single_part_text() {
        let v = serde_json::json!({
            "candidates": [{ "content": { "parts": [{ "text": "hello" }] } }]
        });
        assert_eq!(extract_content(&v).as_deref(), Some("hello"));
    }

    #[test]
    fn concatenates_multi_part_text() {
        let v = serde_json::json!({
            "candidates": [{ "content": { "parts": [{ "text": "he" }, { "text": "llo" }] } }]
        });
        assert_eq!(extract_content(&v).as_deref(), Some("hello"));
    }

    #[test]
    fn empty_parts_returns_none() {
        let v = serde_json::json!({
            "candidates": [{ "content": { "parts": [{ "text": "" }] } }]
        });
        assert_eq!(extract_content(&v), None);
        let missing = serde_json::json!({ "candidates": [] });
        assert_eq!(extract_content(&missing), None);
    }

    #[test]
    fn parses_data_line() {
        let line = r#"data: {"candidates":[{"content":{"parts":[{"text":"hi"}]}}]}"#;
        assert_eq!(parse_sse_chunk(line).as_deref(), Some("hi"));
        assert_eq!(parse_sse_chunk("data: [DONE]"), None);
    }
}
