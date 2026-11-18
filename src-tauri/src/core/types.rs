#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FunctionKind {
    Explain,
    Summarize,
    Translate,
    /// Grammar/spelling fixer. Also the catch-all for any unrecognized value
    /// (e.g. the legacy `"chat"` id in persisted prefs) so old settings files
    /// keep parsing instead of resetting to defaults.
    #[serde(other)]
    FixGrammar,
}

#[allow(dead_code)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatRequest {
    pub provider: String,
    pub model: String,
    pub function: FunctionKind,
    pub query: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    Token(String),
    Done,
    Error(String),
}

#[allow(dead_code)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderConfig {
    pub id: String,
    pub model: String,
    pub key_ref: String,
}
