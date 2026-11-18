//! S03 orchestrator tests: truncate, error mapping, SSE helpers, single-flight.

use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use trayask::core::orchestrator::{
    map_provider_error, truncate_query, Orchestrator, MAX_QUERY_CHARS,
};
use trayask::core::types::{ChatRequest, FunctionKind, StreamEvent};
use trayask::llm::openai::{extract_content, parse_sse_chunk};
use trayask::llm::LlmProvider;

fn test_request(query: String) -> ChatRequest {
    ChatRequest {
        provider: "mock".to_string(),
        model: "mock-model".to_string(),
        function: FunctionKind::FixGrammar,
        query,
    }
}

// ---- 1. truncate_query ----

#[test]
fn truncate_long_input_to_exactly_4000_chars() {
    assert_eq!(MAX_QUERY_CHARS, 4000);
    let long = "a".repeat(6000);
    let out = truncate_query(&long);
    assert_eq!(out.chars().count(), 4000);
    assert_eq!(out.len(), 4000);
}

#[test]
fn truncate_multibyte_emoji_safe() {
    let long = "😀".repeat(5000);
    let out = truncate_query(&long);
    assert_eq!(out.chars().count(), 4000);
    // Must still be valid UTF-8 (no split surrogate/panic) and round-trip.
    assert_eq!(out, "😀".repeat(4000));
}

#[test]
fn truncate_short_input_unchanged() {
    let short = "hello world";
    assert_eq!(truncate_query(short), short);
}

#[test]
fn truncate_empty_unchanged() {
    assert_eq!(truncate_query(""), "");
}

#[test]
fn truncate_exactly_4000_unchanged() {
    let exact = "b".repeat(4000);
    assert_eq!(truncate_query(&exact), exact);
}

// ---- 2. map_provider_error ----

#[test]
fn map_401_mentions_key_or_status() {
    let msg = map_provider_error(Some(401), "", false).to_lowercase();
    assert!(
        msg.contains("401") || msg.contains("key"),
        "401 message must mention 401 or key, got: {msg}"
    );
}

#[test]
fn map_429_mentions_rate_or_status() {
    let msg = map_provider_error(Some(429), "", false).to_lowercase();
    assert!(
        msg.contains("429") || msg.contains("rate"),
        "429 message must mention 429 or rate, got: {msg}"
    );
}

#[test]
fn map_timeout_mentions_timeout() {
    let msg = map_provider_error(None, "", true).to_lowercase();
    assert!(
        msg.contains("timed out") || msg.contains("timeout"),
        "timeout message must mention timed out/timeout, got: {msg}"
    );
}

#[test]
fn map_500_includes_status_and_snippet() {
    let msg = map_provider_error(Some(500), "internal boom", false);
    assert!(msg.contains("500"), "must contain status, got: {msg}");
    assert!(
        msg.contains("internal boom"),
        "must contain snippet, got: {msg}"
    );
}

#[test]
fn map_none_no_timeout_is_generic_nonempty() {
    let msg = map_provider_error(None, "", false);
    assert!(!msg.is_empty(), "generic error must be non-empty");
}

// ---- 3. SSE helpers ----

#[test]
fn sse_delta_content_parsed() {
    let out = parse_sse_chunk("data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}");
    assert_eq!(out, Some("hi".to_string()));
}

#[test]
fn sse_done_returns_none() {
    assert_eq!(parse_sse_chunk("data: [DONE]"), None);
}

#[test]
fn sse_comment_returns_none() {
    assert_eq!(parse_sse_chunk(": comment"), None);
}

#[test]
fn sse_event_line_returns_none() {
    assert_eq!(parse_sse_chunk("event: message"), None);
}

#[test]
fn sse_malformed_json_returns_none() {
    assert_eq!(parse_sse_chunk("data: not-json{"), None);
}

#[test]
fn extract_content_delta_preferred() {
    let v: serde_json::Value =
        serde_json::from_str("{\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}").unwrap();
    assert_eq!(extract_content(&v), Some("hi".to_string()));
}

#[test]
fn extract_content_message_fallback() {
    let v: serde_json::Value =
        serde_json::from_str("{\"choices\":[{\"message\":{\"content\":\"yo\"}}]}").unwrap();
    assert_eq!(extract_content(&v), Some("yo".to_string()));
}

#[test]
fn extract_content_empty_choices_none() {
    let v: serde_json::Value = serde_json::from_str("{\"choices\":[]}").unwrap();
    assert_eq!(extract_content(&v), None);
}

// ---- mocks ----

struct SleepyProvider {
    delay_ms: u64,
}

impl LlmProvider for SleepyProvider {
    fn name(&self) -> &str {
        "mock"
    }

    fn chat_stream(&self, _request: ChatRequest) -> impl Future<Output = Vec<StreamEvent>> + Send {
        let ms = self.delay_ms;
        async move {
            tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
            vec![StreamEvent::Token("late".to_string()), StreamEvent::Done]
        }
    }
}

struct CaptureProvider {
    seen_chars: Arc<AtomicUsize>,
}

impl LlmProvider for CaptureProvider {
    fn name(&self) -> &str {
        "mock"
    }

    fn chat_stream(&self, request: ChatRequest) -> impl Future<Output = Vec<StreamEvent>> + Send {
        let n = request.query.chars().count();
        self.seen_chars.store(n, Ordering::SeqCst);
        async { vec![StreamEvent::Done] }
    }
}

struct ScriptedProvider;

impl LlmProvider for ScriptedProvider {
    fn name(&self) -> &str {
        "mock"
    }

    fn chat_stream(&self, _request: ChatRequest) -> impl Future<Output = Vec<StreamEvent>> + Send {
        async {
            vec![
                StreamEvent::Token("a".to_string()),
                StreamEvent::Token("b".to_string()),
                StreamEvent::Done,
            ]
        }
    }
}

// ---- 4. single-flight cancel suppresses late events ----

#[tokio::test]
async fn orchestrator_cancel_suppresses_in_flight_events() {
    let orch = Arc::new(Orchestrator::new(SleepyProvider { delay_ms: 100 }));
    let count = Arc::new(AtomicUsize::new(0));
    let count2 = count.clone();
    let orch2 = orch.clone();

    let run = tokio::spawn(async move {
        orch2
            .run(test_request("hi".to_string()), move |_ev| {
                count2.fetch_add(1, Ordering::SeqCst);
            })
            .await
    });
    // Cancel mid-flight while the provider is still sleeping.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    orch.cancel();
    let _gen = run.await.expect("run task joins");
    assert_eq!(
        count.load(Ordering::SeqCst),
        0,
        "events after cancel must be suppressed"
    );
}

#[tokio::test]
async fn orchestrator_second_run_supersedes_first() {
    let orch = Orchestrator::new(SleepyProvider { delay_ms: 50 });
    // Simulate a superseding run by bumping the generation before running.
    orch.cancel();
    let gen_before = orch.current_generation();
    let collected = Arc::new(Mutex::new(Vec::new()));
    let c2 = collected.clone();
    let gen = orch
        .run(test_request("hi".to_string()), move |ev| {
            c2.lock().unwrap().push(ev);
        })
        .await;
    assert_eq!(gen, gen_before, "run must report its own generation");
    assert_eq!(
        collected.lock().unwrap().len(),
        2,
        "un-cancelled run replays all events"
    );
}

// ---- 5. orchestrator truncates before delegating ----

#[tokio::test]
async fn orchestrator_truncates_long_query() {
    let seen = Arc::new(AtomicUsize::new(usize::MAX));
    let orch = Orchestrator::new(CaptureProvider {
        seen_chars: seen.clone(),
    });
    orch.run(test_request("x".repeat(6000)), |_| {}).await;
    let n = seen.load(Ordering::SeqCst);
    assert!(
        n <= 4000,
        "provider must see query len <= 4000 chars, saw {n}"
    );
    assert_eq!(n, 4000);
}

// ---- 6. mock E2E replays events in order ----

#[tokio::test]
async fn orchestrator_replays_scripted_events_in_order() {
    let orch = Orchestrator::new(ScriptedProvider);
    let collected = Arc::new(Mutex::new(Vec::new()));
    let c2 = collected.clone();
    orch.run(test_request("ping".to_string()), move |ev| {
        c2.lock().unwrap().push(ev);
    })
    .await;
    let events = collected.lock().unwrap().clone();
    assert_eq!(
        events,
        vec![
            StreamEvent::Token("a".to_string()),
            StreamEvent::Token("b".to_string()),
            StreamEvent::Done,
        ]
    );
}
