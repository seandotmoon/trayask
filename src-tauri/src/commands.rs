use std::sync::atomic::{AtomicU64, Ordering};

use tauri::Emitter;

use crate::core::orchestrator::truncate_query;
use crate::core::types::{ChatRequest, FunctionKind, StreamEvent};
use crate::keys::{KeyStore, KeychainStore};
use crate::llm::anthropic::AnthropicProvider;
use crate::llm::gemini::GeminiProvider;
use crate::llm::openai::OpenAiProvider;
use crate::llm::trait_def::LlmProvider;

/// Global single-flight generation: each `chat` bumps it; `cancel_chat`
/// bumps it too so an in-flight loop stops emitting.
static CHAT_GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, serde::Serialize)]
struct StreamPayload {
    kind: String,
    text: String,
}

fn payload_for(event: &StreamEvent) -> StreamPayload {
    match event {
        StreamEvent::Token(t) => StreamPayload {
            kind: "token".to_string(),
            text: t.clone(),
        },
        StreamEvent::Done => StreamPayload {
            kind: "done".to_string(),
            text: String::new(),
        },
        StreamEvent::Error(e) => StreamPayload {
            kind: "error".to_string(),
            text: e.clone(),
        },
    }
}

fn parse_function(s: Option<String>) -> FunctionKind {
    match s.unwrap_or_default().trim().to_ascii_lowercase().as_str() {
        "explain" => FunctionKind::Explain,
        "summarize" => FunctionKind::Summarize,
        "translate" => FunctionKind::Translate,
        _ => FunctionKind::Chat,
    }
}

/// Providers allowed in S05+. No new providers beyond this list.
pub const ALLOWED_PROVIDERS: &[&str] = &["openai", "anthropic", "gemini", "custom"];

fn normalize_provider(provider: &str) -> String {
    provider.trim().to_ascii_lowercase()
}

fn validate_provider(provider: &str) -> Result<String, String> {
    let p = normalize_provider(provider);
    if ALLOWED_PROVIDERS.contains(&p.as_str()) {
        Ok(p)
    } else {
        Err("Unsupported provider. Use openai, anthropic, gemini, or custom.".to_string())
    }
}

/// Resolve a custom OpenAI-compatible base URL.
///
/// If `raw` is None or empty, falls back to the `CUSTOM_BASE_URL` env var.
/// Trims whitespace and trailing '/'s. Must start with `http://` or `https://`
/// and have a non-empty host after the scheme.
/// NOTE: callers should prefer [`pick_custom_raw`] + this function so an
/// omitted/empty `base_url` param falls back to the saved prefs file first
/// (then env here). The frontend passes `base_url` from panel-local state
/// which can be stale/empty right after startup; the prefs file is the
/// source of truth.
fn normalize_custom_base_url(raw: Option<&str>) -> Result<String, String> {
    let from_param = raw.map(str::trim).filter(|s| !s.is_empty());
    let from_env = std::env::var("CUSTOM_BASE_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let candidate = from_param
        .map(|s| s.to_string())
        .or(from_env)
        .ok_or_else(|| {
            "Custom base URL is empty. Set it in Settings (e.g. http://10.0.1.10:8000/v1)."
                .to_string()
        })?;
    let trimmed = candidate.trim().trim_end_matches('/').to_string();
    if trimmed.is_empty() {
        return Err(
            "Custom base URL is empty. Set it in Settings (e.g. http://10.0.1.10:8000/v1)."
                .to_string(),
        );
    }
    let scheme_len = if trimmed.strip_prefix("http://").is_some() {
        "http://".len()
    } else if trimmed.strip_prefix("https://").is_some() {
        "https://".len()
    } else {
        return Err("Custom base URL must start with http:// or https://.".to_string());
    };
    let host_part = &trimmed[scheme_len..];
    if host_part.is_empty() {
        return Err(
            "Custom base URL is empty. Set it in Settings (e.g. http://10.0.1.10:8000/v1)."
                .to_string(),
        );
    }
    Ok(trimmed)
}

/// Pick the raw custom base URL candidate: explicit `chat` param first,
/// then the saved prefs file value. Returns `None` when both are
/// missing/empty (caller lets `normalize_custom_base_url` fall back to
/// the `CUSTOM_BASE_URL` env var or report "empty").
/// Pure and unit-tested — the fix for "Custom base URL is empty" at query
/// time when the panel sends a stale/omitted `base_url` right after startup
/// even though Settings shows (and `prefs.json` holds) the saved URL.
fn pick_custom_raw<'a>(param: Option<&'a str>, prefs_url: &'a str) -> Option<&'a str> {
    let from_param = param.map(str::trim).filter(|s| !s.is_empty());
    if from_param.is_some() {
        return from_param;
    }
    let saved = prefs_url.trim();
    if saved.is_empty() {
        None
    } else {
        Some(saved)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProviderStatus {
    pub id: String,
    pub has_key: bool,
}

/// Build the error for a keychain read-back mismatch: `set_key` reported
/// `Ok` but an immediate `get_key` found nothing (or failed).
/// Pure (no I/O) so it is unit-testable. Never takes key material —
/// only the provider id — so the message can never leak a secret.
fn keychain_mismatch_err(id: &str) -> String {
    format!(
        "Key for '{id}' was not verified in the OS keychain (service 'trayask-{id}'): \
save reported ok but read-back found nothing. This usually means a stale pre-B-04 \
binary is still running with the mock backend, or keychain access was denied. \
Fully restart `tauri dev` (or rebuild via `tauri build`) to rebuild Rust with \
native keychain features, then check the terminal logs and Keychain Access (macOS) / \
Credential Manager (Windows) / Secret Service (Linux) for 'trayask-{id}'."
    )
}

#[tauri::command]
pub fn save_provider(provider: String, key: String) -> Result<(), String> {
    let id = validate_provider(&provider)?;
    if id == "custom" {
        // Custom needs no key: empty save clears any stored value and succeeds.
        if key.trim().is_empty() {
            let r = KeychainStore::new().delete_key(&id);
            if let Err(ref e) = r {
                eprintln!("[trayask] save_provider (custom clear) failed: {e}");
            }
            return r;
        }
    } else if key.trim().is_empty() {
        return Err("Key is empty.".to_string());
    }
    match KeychainStore::new().set_key(&id, key.trim()) {
        Ok(()) => {
            eprintln!("[trayask] saved key for provider: {id}");
            // Immediate read-back: distinguishes "set didn't persist" from
            // "list reads differently". Never logs key material.
            match KeychainStore::new().get_key(&id) {
                Ok(Some(k)) if !k.trim().is_empty() => {
                    eprintln!("[trayask] key read-back ok for provider: {id}");
                    Ok(())
                }
                Ok(_) => {
                    eprintln!(
                        "[trayask] key read-back MISSING for provider: {id} (set reported ok)"
                    );
                    Err(keychain_mismatch_err(&id))
                }
                Err(e) => {
                    eprintln!("[trayask] key read-back failed for provider {id}: {e}");
                    Err(format!("{} (caused by: {e})", keychain_mismatch_err(&id)))
                }
            }
        }
        Err(e) => {
            eprintln!("[trayask] save_provider failed for {id}: {e}");
            Err(e)
        }
    }
}

#[tauri::command]
pub fn list_providers() -> Result<Vec<ProviderStatus>, String> {
    let store = KeychainStore::new();
    let mut out = Vec::with_capacity(ALLOWED_PROVIDERS.len());
    for id in ALLOWED_PROVIDERS {
        let has_key = match store.get_key(id) {
            Ok(Some(k)) => !k.trim().is_empty(),
            Ok(None) => false,
            Err(e) => {
                eprintln!("[trayask] list_providers read failed for {id}: {e}");
                false
            }
        };
        out.push(ProviderStatus {
            id: id.to_string(),
            has_key,
        });
    }
    Ok(out)
}

#[tauri::command]
pub fn delete_provider(provider: String) -> Result<(), String> {
    let id = validate_provider(&provider)?;
    match KeychainStore::new().delete_key(&id) {
        Ok(()) => {
            eprintln!("[trayask] deleted key for provider: {id}");
            Ok(())
        }
        Err(e) => {
            eprintln!("[trayask] delete_provider failed for {id}: {e}");
            Err(e)
        }
    }
}

#[tauri::command]
pub fn get_prefs(app: tauri::AppHandle) -> Result<crate::store::Prefs, String> {
    Ok(crate::store::load_prefs_file(&app))
}

#[tauri::command]
pub fn save_prefs(
    app: tauri::AppHandle,
    prefs: crate::store::Prefs,
) -> Result<crate::store::Prefs, String> {
    // Validate provider against the allowlist so a stale/foreign value
    // can never persist and break the panel on next start.
    let provider_id = validate_provider(&prefs.provider_id)?;
    let mut next = prefs;
    next.provider_id = provider_id;
    crate::store::write_prefs_file(&app, &next)?;
    let _ = app.emit("prefs-changed", &next);
    Ok(next)
}

/// Resolve an API key: keychain first, `OPENAI_API_KEY` env fallback (dev only).
fn resolve_openai_key() -> Result<String, String> {
    if let Ok(Some(k)) = KeychainStore::new().get_key("openai") {
        if !k.trim().is_empty() {
            return Ok(k);
        }
    }
    std::env::var("OPENAI_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| "Missing API key. Save your OpenAI key in Settings.".to_string())
}

/// Resolve an Anthropic API key: keychain first, `ANTHROPIC_API_KEY` env fallback.
fn resolve_anthropic_key() -> Result<String, String> {
    if let Ok(Some(k)) = KeychainStore::new().get_key("anthropic") {
        if !k.trim().is_empty() {
            return Ok(k);
        }
    }
    std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| "Missing API key. Save your Anthropic key in Settings.".to_string())
}

/// Resolve a Gemini API key: keychain first, `GEMINI_API_KEY` then `GOOGLE_API_KEY` env fallback.
fn resolve_gemini_key() -> Result<String, String> {
    if let Ok(Some(k)) = KeychainStore::new().get_key("gemini") {
        if !k.trim().is_empty() {
            return Ok(k);
        }
    }
    std::env::var("GEMINI_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())
        .or_else(|| {
            std::env::var("GOOGLE_API_KEY")
                .ok()
                .filter(|k| !k.trim().is_empty())
        })
        .ok_or_else(|| "Missing API key. Save your Gemini key in Settings.".to_string())
}
/// Resolve a custom endpoint key: keychain "custom" if non-empty, else None.
/// The key is optional — never errors.
fn resolve_custom_key() -> Option<String> {
    if let Ok(Some(k)) = KeychainStore::new().get_key("custom") {
        if !k.trim().is_empty() {
            return Some(k);
        }
    }
    None
}

/// Parse model ids from an OpenAI-compatible `/models` response.
///
/// Supports `{data:[{id:...}]}` plus fallbacks `{models:[...]}`
/// (array of strings or `{id}` objects) and a bare array.
/// Trims, drops empties, dedupes preserving order.
fn parse_custom_models(json: &serde_json::Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: &str, out: &mut Vec<String>| {
        let t = s.trim();
        if !t.is_empty() && !out.iter().any(|x| x == t) {
            out.push(t.to_string());
        }
    };
    let mut push_value = |v: &serde_json::Value, out: &mut Vec<String>| match v {
        serde_json::Value::String(s) => push(s, out),
        serde_json::Value::Object(m) => {
            if let Some(serde_json::Value::String(id)) = m.get("id").or_else(|| m.get("name")) {
                push(id, out);
            }
        }
        _ => {}
    };
    if let Some(arr) = json.get("data").and_then(|v| v.as_array()) {
        for v in arr {
            push_value(v, &mut out);
        }
        return out;
    }
    if let Some(arr) = json.get("models").and_then(|v| v.as_array()) {
        for v in arr {
            push_value(v, &mut out);
        }
        return out;
    }
    if let Some(arr) = json.as_array() {
        for v in arr {
            push_value(v, &mut out);
        }
    }
    out
}

#[tauri::command]
pub async fn list_custom_models(
    base_url: String,
    key: Option<String>,
) -> Result<Vec<String>, String> {
    println!("Message from list_custom_models: {}", base_url);
    let base = normalize_custom_base_url(Some(&base_url))?;
    let url = format!("{base}/models");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;
    let mut req = client.get(&url);
    if let Some(k) = key.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        req = req.bearer_auth(k);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        let snippet: String = body.chars().take(200).collect();
        return Err(format!("Request failed with status {status}: {snippet}"));
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse models response: {e}"))?;
    let models = parse_custom_models(&json);
    if models.is_empty() {
        return Err("No models returned by the custom endpoint.".to_string());
    }
    Ok(models)
}
#[tauri::command]
pub async fn chat(
    app: tauri::AppHandle,
    provider: String,
    model: String,
    function: Option<String>,
    query: String,
    base_url: Option<String>,
) -> Result<String, String> {
    if query.trim().is_empty() {
        return Err("Query is empty. Type something first.".to_string());
    }
    let provider_id = validate_provider(&provider)?;
    let function_kind = parse_function(function);
    let truncated = truncate_query(&query);

    let gen = CHAT_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;

    let default_model = |fallback: &str| -> String {
        if model.trim().is_empty() {
            fallback.to_string()
        } else {
            model.clone()
        }
    };

    let events: Vec<StreamEvent> = match provider_id.as_str() {
        "openai" => {
            let api_key = resolve_openai_key()?;
            let request = ChatRequest {
                provider: "openai".to_string(),
                model: default_model("gpt-4o-mini"),
                function: function_kind,
                query: truncated,
            };
            let llm = OpenAiProvider::new(api_key);
            llm.chat_stream(request).await
        }
        "anthropic" => {
            let api_key = resolve_anthropic_key()?;
            let request = ChatRequest {
                provider: "anthropic".to_string(),
                model: default_model("claude-3-5-sonnet-20241022"),
                function: function_kind,
                query: truncated,
            };
            let llm = AnthropicProvider::new(api_key);
            llm.chat_stream(request).await
        }
        "gemini" => {
            let api_key = resolve_gemini_key()?;
            let request = ChatRequest {
                provider: "gemini".to_string(),
                model: default_model("gemini-1.5-flash"),
                function: function_kind,
                query: truncated,
            };
            let llm = GeminiProvider::new(api_key);
            llm.chat_stream(request).await
        }
        "custom" => {
            // Backend is the source of truth for the URL: an omitted or
            // stale-empty `base_url` param (panel state right after startup)
            // falls back to the saved prefs file before the env var.
            let saved_prefs = crate::store::load_prefs_file(&app);
            let raw = pick_custom_raw(base_url.as_deref(), &saved_prefs.custom_base_url);
            let base = normalize_custom_base_url(raw)?;
            let api_key = resolve_custom_key().unwrap_or_default();
            let request = ChatRequest {
                provider: "custom".to_string(),
                model: default_model("default"),
                function: function_kind,
                query: truncated,
            };
            // NOTE: OpenAiProvider::chat_stream always sends
            // `.bearer_auth(&self.api_key)`; with an empty key that emits an
            // empty "Bearer " header. Most OpenAI-compatible servers (vLLM,
            // LM Studio) ignore it. Cannot change openai.rs here.
            let llm = OpenAiProvider::with_base_url(api_key, base);
            llm.chat_stream(request).await
        }
        _ => {
            return Err(
                "Unsupported provider. Use openai, anthropic, gemini, or custom.".to_string(),
            )
        }
    };

    let mut last_error: Option<String> = None;
    for ev in events {
        if CHAT_GENERATION.load(Ordering::SeqCst) != gen {
            return Ok("cancelled".to_string());
        }
        if let StreamEvent::Error(e) = &ev {
            last_error = Some(e.clone());
        }
        let payload = payload_for(&ev);
        let _ = app.emit("stream-token", payload);
        if ev == StreamEvent::Done {
            break;
        }
    }
    if let Some(err) = last_error {
        return Err(err);
    }
    Ok("ok".to_string())
}

#[tauri::command]
pub fn cancel_chat() -> bool {
    CHAT_GENERATION.fetch_add(1, Ordering::SeqCst);
    true
}

#[tauri::command]
pub fn open_settings(app: tauri::AppHandle) {
    crate::platform::windows::show_settings(&app);
}

#[tauri::command]
pub fn open_help(app: tauri::AppHandle) {
    crate::platform::windows::show_help(&app);
}

#[cfg(test)]
mod validation_tests {
    use super::*;

    #[test]
    fn accepts_allowlist_case_insensitive_and_trimmed() {
        assert_eq!(validate_provider("openai").unwrap(), "openai");
        assert_eq!(validate_provider("  OpenAI ").unwrap(), "openai");
        assert_eq!(validate_provider("ANTHROPIC").unwrap(), "anthropic");
        assert_eq!(validate_provider("gemini").unwrap(), "gemini");
        assert_eq!(validate_provider("custom").unwrap(), "custom");
        assert_eq!(validate_provider(" Custom ").unwrap(), "custom");
    }

    #[test]
    fn rejects_empty_and_unknown_provider() {
        assert!(validate_provider("").is_err());
        assert!(validate_provider("   ").is_err());
        assert!(validate_provider("cohere").is_err());
        assert!(validate_provider("openai2").is_err());
    }

    #[test]
    fn allowlist_has_exactly_the_four_providers() {
        assert_eq!(
            ALLOWED_PROVIDERS,
            &["openai", "anthropic", "gemini", "custom"]
        );
    }

    #[test]
    fn provider_status_serializes_without_secret() {
        // `list_providers` must expose only `{id, has_key}` — never key material.
        let s = ProviderStatus {
            id: "openai".to_string(),
            has_key: true,
        };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v, serde_json::json!({"id": "openai", "has_key": true}));
        assert!(v.get("key").is_none());
        assert!(v.get("secret").is_none());
        assert!(v.get("password").is_none());
    }

    #[test]
    fn normalize_custom_base_url_accepts_http_with_port_and_v1() {
        std::env::remove_var("CUSTOM_BASE_URL");
        assert_eq!(
            normalize_custom_base_url(Some("http://10.0.1.10:8000/v1/")).unwrap(),
            "http://10.0.1.10:8000/v1"
        );
    }

    #[test]
    fn normalize_custom_base_url_accepts_https() {
        std::env::remove_var("CUSTOM_BASE_URL");
        assert_eq!(
            normalize_custom_base_url(Some("https://example.com/v1")).unwrap(),
            "https://example.com/v1"
        );
    }

    #[test]
    fn normalize_custom_base_url_rejects_empty_and_none() {
        std::env::remove_var("CUSTOM_BASE_URL");
        assert!(normalize_custom_base_url(None).is_err());
        assert!(normalize_custom_base_url(Some("")).is_err());
        assert!(normalize_custom_base_url(Some("   ")).is_err());
    }

    #[test]
    fn normalize_custom_base_url_rejects_bad_scheme_and_bare_host() {
        std::env::remove_var("CUSTOM_BASE_URL");
        assert!(normalize_custom_base_url(Some("ftp://example.com/v1")).is_err());
        assert!(normalize_custom_base_url(Some("http://")).is_err());
    }

    #[test]
    fn pick_custom_raw_prefers_param_over_prefs() {
        assert_eq!(
            pick_custom_raw(Some("http://param:8000/v1"), "http://saved:8000/v1"),
            Some("http://param:8000/v1")
        );
    }

    #[test]
    fn pick_custom_raw_falls_back_to_saved_prefs_when_param_missing_or_empty() {
        assert_eq!(
            pick_custom_raw(None, "http://10.0.1.10:8000/v1"),
            Some("http://10.0.1.10:8000/v1")
        );
        assert_eq!(
            pick_custom_raw(Some(""), "http://10.0.1.10:8000/v1"),
            Some("http://10.0.1.10:8000/v1")
        );
        assert_eq!(
            pick_custom_raw(Some("   "), "http://10.0.1.10:8000/v1"),
            Some("http://10.0.1.10:8000/v1")
        );
    }

    #[test]
    fn pick_custom_raw_returns_none_when_both_empty() {
        assert_eq!(pick_custom_raw(None, ""), None);
        assert_eq!(pick_custom_raw(Some("  "), "   "), None);
    }

    #[test]
    fn saved_prefs_url_flows_through_normalize() {
        std::env::remove_var("CUSTOM_BASE_URL");
        // Simulates: panel sent stale/empty base_url, prefs.json holds the URL.
        let raw = pick_custom_raw(None, "http://10.0.1.10:8000/v1/");
        assert_eq!(
            normalize_custom_base_url(raw).unwrap(),
            "http://10.0.1.10:8000/v1"
        );
    }

    #[test]
    fn parse_custom_models_accepts_data_shape() {
        let v = serde_json::json!({"data": [{"id": "llama3.1"}, {"id": "qwen2.5"}]});
        assert_eq!(parse_custom_models(&v), vec!["llama3.1", "qwen2.5"]);
    }

    #[test]
    fn parse_custom_models_accepts_models_shape() {
        let v = serde_json::json!({"models": ["a", {"id": "b"}]});
        assert_eq!(parse_custom_models(&v), vec!["a", "b"]);
    }

    #[test]
    fn parse_custom_models_accepts_bare_array() {
        let v = serde_json::json!(["x", {"id": "y"}]);
        assert_eq!(parse_custom_models(&v), vec!["x", "y"]);
    }

    #[test]
    fn parse_custom_models_dedupes_and_drops_empties() {
        let v = serde_json::json!({"data": [
            {"id": " a "}, {"id": "a"}, {"id": ""}, {"id": "  "}, {"id": "b"}, "b"
        ]});
        assert_eq!(parse_custom_models(&v), vec!["a", "b"]);
    }

    #[test]
    fn keychain_mismatch_err_contains_provider_id() {
        let msg = keychain_mismatch_err("openai");
        assert!(
            msg.contains("openai"),
            "mismatch msg must name provider, got: {msg}"
        );
    }

    #[test]
    fn keychain_mismatch_err_names_keychain_entry() {
        let msg = keychain_mismatch_err("openai");
        assert!(
            msg.contains("trayask-openai"),
            "mismatch msg must name entry trayask-<id>, got: {msg}"
        );
        let msg2 = keychain_mismatch_err("anthropic");
        assert!(msg2.contains("anthropic") && msg2.contains("trayask-anthropic"));
    }

    #[test]
    fn keychain_mismatch_err_mentions_restart_or_rebuild() {
        let msg = keychain_mismatch_err("openai").to_ascii_lowercase();
        assert!(
            msg.contains("restart") || msg.contains("rebuild"),
            "mismatch msg must guide restart/rebuild, got: {msg}"
        );
    }

    #[test]
    fn keychain_mismatch_err_mentions_terminal_and_keychain_access() {
        let lower = keychain_mismatch_err("openai").to_ascii_lowercase();
        assert!(
            lower.contains("terminal"),
            "must mention terminal, got: {lower}"
        );
        assert!(
            lower.contains("keychain access"),
            "must mention Keychain Access, got: {lower}"
        );
    }

    #[test]
    fn keychain_mismatch_err_contains_no_key_material() {
        // NOTE: cannot assert `!contains("sk-")` — the service name
        // `trayask-<id>` itself contains that substring. Use sentinels
        // that only real key material would contain.
        for id in ["openai", "anthropic", "gemini", "custom"] {
            let msg = keychain_mismatch_err(id);
            assert!(
                !msg.contains("sk-test-secret-value"),
                "must not leak key material for {id}"
            );
            assert!(
                !msg.contains("diag-secret"),
                "must not leak key material for {id}"
            );
        }
    }

    #[test]
    fn parse_custom_models_accepts_user_server_payload() {
        let v: serde_json::Value = serde_json::from_str(r#"{"object":"list","data":[{"id":"unsloth/Qwen3.8-27B-GGUF:UD-Q4_K_XL","object":"model","created":0,"owned_by":"splash","max_model_len":262144,"context_length":262144,"vision":true,"input_modalities":["text","image","pdf"]}],"models":[{"name":"unsloth/Qwen3.8-27B-GGUF:UD-Q4_K_XL","description":"Splash resident model","release_date":""}]}"#).unwrap();
        assert_eq!(
            parse_custom_models(&v),
            vec!["unsloth/Qwen3.8-27B-GGUF:UD-Q4_K_XL"]
        );
    }

    #[test]
    fn parse_custom_models_accepts_name_objects() {
        let v = serde_json::json!({"models": [{"name": "m1"}, {"name": "m2"}]});
        assert_eq!(parse_custom_models(&v), vec!["m1", "m2"]);
        let bare = serde_json::json!([{"name": "m3"}]);
        assert_eq!(parse_custom_models(&bare), vec!["m3"]);
    }

    #[test]
    fn parse_custom_models_prefers_id_over_name() {
        let v = serde_json::json!({"models": [{"id": "the-id", "name": "the-name"}]});
        assert_eq!(parse_custom_models(&v), vec!["the-id"]);
    }
}
