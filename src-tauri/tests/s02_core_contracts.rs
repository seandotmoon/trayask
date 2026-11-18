//! S02 core-contract tests (integration, std-only — no async runtime dep).
//!
//! Covers: types serde, prompts, store prefs, in-memory keys, llm trait mock.

use std::future::Future;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use trayask::core::prompts::build_prompt;
use trayask::core::types::{ChatRequest, FunctionKind, StreamEvent};
use trayask::keys::{InMemoryKeyStore, KeyStore};
use trayask::llm::LlmProvider;
use trayask::store::{default_prefs, prefs_from_json, prefs_to_json};

/// Minimal single-threaded block_on for immediately-ready futures.
/// Avoids adding a tokio/futures dev-dependency for S02.
fn block_on<F: Future>(mut f: F) -> F::Output {
    fn noop_raw_waker() -> RawWaker {
        fn noop(_: *const ()) {}
        fn clone(_: *const ()) -> RawWaker {
            noop_raw_waker()
        }
        let vtable: &'static RawWakerVTable = &RawWakerVTable::new(clone, noop, noop, noop);
        RawWaker::new(std::ptr::null(), vtable)
    }
    let waker = unsafe { Waker::from_raw(noop_raw_waker()) };
    let mut cx = Context::from_waker(&waker);
    // SAFETY: pinned on the stack for the duration of the poll loop; never moved.
    let mut f = unsafe { std::pin::Pin::new_unchecked(&mut f) };
    loop {
        match f.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

// ---- types ----

#[test]
fn chat_request_serde_round_trip() {
    let req = ChatRequest {
        provider: "openai".to_string(),
        model: "gpt-4o-mini".to_string(),
        function: FunctionKind::Summarize,
        query: "hello world".to_string(),
    };
    let json = serde_json::to_string(&req).expect("serialize ChatRequest");
    let back: ChatRequest = serde_json::from_str(&json).expect("deserialize ChatRequest");
    assert_eq!(back.provider, req.provider);
    assert_eq!(back.model, req.model);
    assert_eq!(back.function, req.function);
    assert_eq!(back.query, req.query);
}

#[test]
fn function_kind_serde_snake_case() {
    let cases = [
        (FunctionKind::Explain, "\"explain\""),
        (FunctionKind::FixGrammar, "\"fix_grammar\""),
        (FunctionKind::Summarize, "\"summarize\""),
        (FunctionKind::Translate, "\"translate\""),
    ];
    for (kind, expected) in cases {
        let json = serde_json::to_string(&kind).expect("serialize FunctionKind");
        assert_eq!(json, expected, "FunctionKind::{kind:?} must be snake_case");
        let back: FunctionKind = serde_json::from_str(&json).expect("deserialize FunctionKind");
        assert_eq!(back, kind);
    }
}

#[test]
fn stream_event_variants_constructible_and_eq() {
    let tok = StreamEvent::Token("hi".to_string());
    let done = StreamEvent::Done;
    let err = StreamEvent::Error("boom".to_string());
    assert_eq!(tok, StreamEvent::Token("hi".to_string()));
    assert_eq!(done, StreamEvent::Done);
    assert_eq!(err, StreamEvent::Error("boom".to_string()));
    assert_ne!(tok, done);
    assert_ne!(done, err);
    assert_ne!(tok, err);
}

// ---- prompts ----

#[test]
fn all_prompts_nonempty_contain_query_and_distinct() {
    let query = "the quick brown fox 42";
    let kinds = [
        FunctionKind::Explain,
        FunctionKind::FixGrammar,
        FunctionKind::Summarize,
        FunctionKind::Translate,
    ];
    let outputs: Vec<String> = kinds.iter().map(|k| build_prompt(*k, query)).collect();
    for (kind, out) in kinds.iter().zip(outputs.iter()) {
        assert!(!out.is_empty(), "{kind:?} prompt must be non-empty");
        assert!(
            out.contains(query),
            "{kind:?} prompt must contain the query"
        );
    }
    for i in 0..outputs.len() {
        for j in (i + 1)..outputs.len() {
            assert_ne!(
                outputs[i], outputs[j],
                "prompts for {:?} and {:?} must be pairwise distinct",
                kinds[i], kinds[j]
            );
        }
    }
}

// ---- store ----

#[test]
fn default_prefs_values() {
    let p = default_prefs();
    assert_eq!(p.provider_id, "openai");
    assert_eq!(p.model_id, "gpt-4o-mini");
    assert_eq!(p.default_function, FunctionKind::FixGrammar);
    assert!(p.auto_submit);
}

#[test]
fn prefs_json_round_trip() {
    let p = default_prefs();
    let json = prefs_to_json(&p).expect("serialize prefs");
    let back = prefs_from_json(&json).expect("deserialize prefs");
    assert_eq!(back.provider_id, p.provider_id);
    assert_eq!(back.model_id, p.model_id);
    assert_eq!(back.default_function, p.default_function);
    assert_eq!(back.auto_submit, p.auto_submit);
}

#[test]
fn prefs_from_empty_object_yields_defaults() {
    let p = prefs_from_json("{}").expect("parse {}");
    assert_eq!(p.provider_id, "openai");
    assert_eq!(p.model_id, "gpt-4o-mini");
    assert_eq!(p.default_function, FunctionKind::FixGrammar);
    assert!(p.auto_submit);
}

#[test]
fn prefs_pretty_json_contains_newline() {
    let json = prefs_to_json(&default_prefs()).expect("serialize prefs");
    assert!(json.contains('\n'), "pretty JSON must contain a newline");
}

#[test]
fn legacy_chat_pref_migrates_to_fix_grammar() {
    // Legacy persisted prefs with `"chat"` — or any unrecognized value —
    // deserialize to FixGrammar via `#[serde(other)]`, and re-serialize as
    // `"fix_grammar"` so the next save self-heals the stored file.
    for legacy in [
        r#"{"default_function":"chat"}"#,
        r#"{"default_function":"banana"}"#,
    ] {
        let p = prefs_from_json(legacy).expect("parse legacy prefs");
        assert_eq!(p.default_function, FunctionKind::FixGrammar, "{legacy}");
        let json = prefs_to_json(&p).expect("serialize migrated prefs");
        assert!(
            json.contains("\"fix_grammar\""),
            "{legacy} must re-serialize as fix_grammar"
        );
        assert!(
            !json.contains("\"chat\""),
            "{legacy} must not keep the legacy chat id"
        );
    }
}

#[test]
fn fix_grammar_prompt_is_minimal_proofreader() {
    let out = build_prompt(FunctionKind::FixGrammar, "i went to the store");
    assert!(out.contains("smallest possible changes"));
    assert!(out.contains("i went to the store"));
    assert!(
        !out.contains("conversational"),
        "old chat prompt text must be gone"
    );
}

// ---- keys ----

#[test]
fn in_memory_keystore_set_get_delete_overwrite() {
    let ks = InMemoryKeyStore::new();

    // get unknown returns None
    assert_eq!(ks.get_key("openai").expect("get unknown"), None);

    // set -> get returns key
    ks.set_key("openai", "sk-test-1").expect("set");
    assert_eq!(
        ks.get_key("openai").expect("get"),
        Some("sk-test-1".to_string())
    );

    // overwrite updates
    ks.set_key("openai", "sk-test-2").expect("overwrite");
    assert_eq!(
        ks.get_key("openai").expect("get after overwrite"),
        Some("sk-test-2".to_string())
    );

    // delete removes
    ks.delete_key("openai").expect("delete");
    assert_eq!(ks.get_key("openai").expect("get after delete"), None);
}

// ---- llm trait ----

struct MockProvider;

impl LlmProvider for MockProvider {
    fn name(&self) -> &str {
        "mock"
    }

    fn chat_stream(&self, _request: ChatRequest) -> impl Future<Output = Vec<StreamEvent>> + Send {
        async { vec![StreamEvent::Token("hello".to_string()), StreamEvent::Done] }
    }
}

#[test]
fn mock_provider_streams_tokens_then_done() {
    let provider = MockProvider;
    assert_eq!(provider.name(), "mock");
    let req = ChatRequest {
        provider: "mock".to_string(),
        model: "mock-model".to_string(),
        function: FunctionKind::FixGrammar,
        query: "ping".to_string(),
    };
    let events = block_on(provider.chat_stream(req));
    assert!(events.len() >= 2, "expected tokens + done, got {events:?}");
    assert_eq!(events[0], StreamEvent::Token("hello".to_string()));
    assert_eq!(events[events.len() - 1], StreamEvent::Done);
}
