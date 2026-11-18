use super::types::FunctionKind;

/// Build a fixed system-style prompt for the given function, embedding the query.
///
/// Pure function: no I/O. Each output is non-empty and contains `query`.
pub fn build_prompt(function: FunctionKind, query: &str) -> String {
    match function {
        FunctionKind::Explain => format!(
            "You are a helpful assistant. Explain the following clearly and concisely:\n\n{query}"
        ),
        FunctionKind::FixGrammar => format!(
            "You are a meticulous proofreader. Fix only the grammar, spelling, and punctuation errors in the text below. Make the smallest possible changes: keep the author's word choice, voice, tone, and sentence structure exactly as they are. Do not rewrite or reorganize sentences, do not add or remove information, and do not change formatting. Reply with only the corrected text — no explanations, no comments.\n\n{query}"
        ),
        FunctionKind::Summarize => format!(
            "You are a helpful assistant. Summarize the following text in a few key points:\n\n{query}"
        ),
        FunctionKind::Translate => format!(
            "You are a helpful translator. Translate the following text to English, preserving meaning and tone:\n\n{query}"
        ),
    }
}
