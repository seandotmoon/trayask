use tauri::Manager;

use crate::core::types::FunctionKind;

#[allow(dead_code)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Prefs {
    #[serde(default = "default_provider_id")]
    pub provider_id: String,
    #[serde(default = "default_model_id")]
    pub model_id: String,
    #[serde(default = "default_function")]
    pub default_function: FunctionKind,
    #[serde(default = "default_auto_submit")]
    pub auto_submit: bool,
    #[serde(default)]
    pub custom_base_url: String,
}

fn default_provider_id() -> String {
    "openai".to_string()
}

fn default_model_id() -> String {
    "gpt-4o-mini".to_string()
}

fn default_function() -> FunctionKind {
    FunctionKind::Chat
}

fn default_auto_submit() -> bool {
    true
}

/// Canonical default preferences.
pub fn default_prefs() -> Prefs {
    Prefs {
        provider_id: default_provider_id(),
        model_id: default_model_id(),
        default_function: default_function(),
        auto_submit: default_auto_submit(),
        custom_base_url: String::new(),
    }
}

/// Parse prefs from JSON, falling back to defaults for missing fields.
///
/// An empty or whitespace-only string yields defaults.
pub fn prefs_from_json(s: &str) -> Result<Prefs, serde_json::Error> {
    if s.trim().is_empty() {
        return Ok(default_prefs());
    }
    serde_json::from_str(s)
}

/// Serialize prefs to pretty JSON.
pub fn prefs_to_json(p: &Prefs) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(p)
}

/// File the prefs are persisted to (app data dir).
pub const PREFS_FILE: &str = "prefs.json";

/// Resolve the prefs file path. Falls back to a relative path when the
/// app-data dir is unavailable (tests / non-Tauri hosts).
pub fn prefs_path(app: &tauri::AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .map(|d| d.join(PREFS_FILE))
        .unwrap_or_else(|_| std::path::PathBuf::from(PREFS_FILE))
}

/// Best-effort load: missing or unparseable file yields defaults.
/// Never errors — old installs with `{}` or partial JSON migrate via serde defaults.
pub fn load_prefs_file(app: &tauri::AppHandle) -> Prefs {
    let path = prefs_path(app);
    let s = std::fs::read_to_string(path).unwrap_or_default();
    prefs_from_json(&s).unwrap_or_else(|_| default_prefs())
}

/// Write prefs to disk, creating parent dirs. Errors are returned as strings.
pub fn write_prefs_file(app: &tauri::AppHandle, prefs: &Prefs) -> Result<(), String> {
    let path = prefs_path(app);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("Failed to save settings: {e}"))?;
        }
    }
    let s = prefs_to_json(prefs).map_err(|e| format!("Failed to save settings: {e}"))?;
    std::fs::write(path, s).map_err(|e| format!("Failed to save settings: {e}"))
}
