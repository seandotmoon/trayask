use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use tauri_plugin_clipboard_manager::ClipboardExt;

/// Maximum clipboard text length in chars; longer input is truncated.
pub const MAX_CLIPBOARD_CHARS: usize = 4000;

/// Clipboard freshness window in seconds: shortcut-triggered queries only use
/// clipboard text last observed changing within this window. Older content is
/// treated as empty (hint only, never auto-sent).
pub const CLIPBOARD_FRESHNESS_SECS: u64 = 30;

/// Background watcher poll interval. The tray app is long-lived, so a 1s
/// poll keeps `last_change` accurate to ~1s without noticeable cost.
pub const CLIPBOARD_POLL_INTERVAL: Duration = Duration::from_millis(1000);

/// Pure, unit-testable sanitizer for clipboard text:
/// - `None` → `None`
/// - trimmed; empty/whitespace-only → `None`
/// - truncated to [`MAX_CLIPBOARD_CHARS`] chars (char-boundary safe)
/// - otherwise `Some(trimmed)`
pub fn sanitize_clipboard(raw: Option<String>) -> Option<String> {
    let trimmed = raw?.trim().to_string();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.chars().count() > MAX_CLIPBOARD_CHARS {
        Some(trimmed.chars().take(MAX_CLIPBOARD_CHARS).collect())
    } else {
        Some(trimmed)
    }
}

/// Pure freshness check: `true` when the clipboard was last observed changing
/// `elapsed_secs` ago and that is within [`CLIPBOARD_FRESHNESS_SECS`].
/// Boundary (`== 30`) counts as fresh ("older (>30 seconds)" is stale).
pub fn is_within_freshness(elapsed_secs: u64) -> bool {
    elapsed_secs <= CLIPBOARD_FRESHNESS_SECS
}

/// Last observed clipboard state. `last_change` is the monotonic time the
/// current `last_text` was first seen.
#[derive(Debug, Clone)]
struct Tracker {
    last_text: Option<String>,
    last_change: Option<Instant>,
}

fn tracker() -> &'static Mutex<Tracker> {
    static TRACKER: OnceLock<Mutex<Tracker>> = OnceLock::new();
    TRACKER.get_or_init(|| {
        Mutex::new(Tracker {
            last_text: None,
            last_change: None,
        })
    })
}

/// Pure tracker transition, unit-tested without threads or the OS clipboard.
///
/// - `current` is the freshly read (already sanitized) clipboard text.
/// - When `current` differs from `prev_text`, the clipboard changed: the new
///   text becomes current with `last_change = now` (age 0 = fresh).
/// - When equal (including both `None`), the previous timestamp is kept so
///   age keeps growing while the clipboard is untouched.
fn update_tracker(
    prev_text: Option<String>,
    prev_change: Option<Instant>,
    current: Option<String>,
    now: Instant,
) -> (Option<String>, Option<Instant>) {
    if current != prev_text {
        (current, Some(now))
    } else {
        (prev_text, prev_change)
    }
}

/// Record an observation and return the age in seconds of the clipboard
/// content, or `None` when there is no usable text / no timestamp yet.
fn note_observation(current: Option<String>, now: Instant) -> Option<u64> {
    let (prev_text, prev_change) = {
        let t = tracker().lock().expect("clipboard tracker poisoned");
        (t.last_text.clone(), t.last_change)
    };
    let (next_text, next_change) = update_tracker(prev_text, prev_change, current.clone(), now);
    let age = match (&next_text, next_change) {
        (Some(_), Some(since)) => Some(now.duration_since(since).as_secs()),
        _ => None,
    };
    {
        let mut t = tracker().lock().expect("clipboard tracker poisoned");
        t.last_text = next_text;
        t.last_change = next_change;
    }
    // `current` is `Some` here exactly when `next_text` is `Some` (they are
    // equal in the changed branch, and prev==current in the kept branch), so
    // `age` is `Some` iff there is usable text.
    let _ = current;
    age
}

/// Read text from the system clipboard. Text-only (never reads images).
/// Returns `None` on error or when the clipboard is empty/whitespace-only.
pub fn read_clipboard_text(app: &tauri::AppHandle) -> Option<String> {
    match app.clipboard().read_text() {
        Ok(text) => sanitize_clipboard(Some(text)),
        Err(e) => {
            eprintln!("[trayask] clipboard read failed: {e}");
            None
        }
    }
}

/// Read the clipboard gated by freshness: returns the sanitized text only
/// when it was last observed changing within [`CLIPBOARD_FRESHNESS_SECS`];
/// otherwise `None` (caller treats it exactly like an empty clipboard:
/// hint only, never auto-send).
///
/// First-sight text (differs from everything the watcher has seen, e.g. a
/// copy made <1s before the shortcut fired) is aged 0 = fresh. Known
/// limitation: a copy made while the app was not running also looks
/// first-sight fresh on the very first shortcut press after start.
pub fn read_clipboard_text_fresh(app: &tauri::AppHandle) -> Option<String> {
    let text = read_clipboard_text(app);
    let now = Instant::now();
    let age = note_observation(text.clone(), now);
    match (text, age) {
        (Some(t), Some(age_secs)) if is_within_freshness(age_secs) => Some(t),
        _ => None,
    }
}

/// Poll the OS clipboard in the background and keep the freshness tracker
/// accurate. Spawn once from `setup`; the thread exits with the process.
/// Only char counts are logged, never content.
pub fn spawn_clipboard_watcher(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        let now = Instant::now();
        let current = match app.clipboard().read_text() {
            Ok(text) => sanitize_clipboard(Some(text)),
            Err(_) => None,
        };
        note_observation(current, now);
        std::thread::sleep(CLIPBOARD_POLL_INTERVAL);
    });
}

/// Frontend fallback (`Cmd/Ctrl+/` pressed while the panel webview has focus)
/// reads through this instead of `navigator.clipboard` so the 30s freshness
/// gate applies there too. Returns `""` when stale/empty (never errors).
#[tauri::command]
pub fn read_clipboard_fresh(app: tauri::AppHandle) -> String {
    read_clipboard_text_fresh(&app).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{
        is_within_freshness, sanitize_clipboard, update_tracker, CLIPBOARD_FRESHNESS_SECS,
        MAX_CLIPBOARD_CHARS,
    };
    use std::time::Instant;

    #[test]
    fn none_stays_none() {
        assert_eq!(sanitize_clipboard(None), None);
    }

    #[test]
    fn empty_and_whitespace_become_none() {
        assert_eq!(sanitize_clipboard(Some(String::new())), None);
        assert_eq!(sanitize_clipboard(Some("   \n\t  ".to_string())), None);
    }

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(
            sanitize_clipboard(Some("  hello  ".to_string())),
            Some("hello".to_string())
        );
    }

    #[test]
    fn truncates_to_max_chars_on_char_boundary() {
        let long = "é".repeat(MAX_CLIPBOARD_CHARS + 100);
        let out = sanitize_clipboard(Some(long)).expect("should be Some");
        assert_eq!(out.chars().count(), MAX_CLIPBOARD_CHARS);
    }

    #[test]
    fn short_text_passes_through() {
        assert_eq!(
            sanitize_clipboard(Some("query".to_string())),
            Some("query".to_string())
        );
    }

    #[test]
    fn freshness_boundary_is_fresh() {
        assert!(is_within_freshness(0));
        assert!(is_within_freshness(29));
        assert!(is_within_freshness(CLIPBOARD_FRESHNESS_SECS));
    }

    #[test]
    fn older_than_30s_is_stale() {
        assert!(!is_within_freshness(CLIPBOARD_FRESHNESS_SECS + 1));
        assert!(!is_within_freshness(3600));
        assert!(!is_within_freshness(u64::MAX));
    }

    #[test]
    fn changed_text_resets_timestamp() {
        let now = Instant::now();
        let old = now.checked_sub(std::time::Duration::from_secs(60)).unwrap();
        let (text, since) = update_tracker(
            Some("old".to_string()),
            Some(old),
            Some("new".to_string()),
            now,
        );
        assert_eq!(text, Some("new".to_string()));
        assert_eq!(since, Some(now));
    }

    #[test]
    fn unchanged_text_keeps_timestamp() {
        let now = Instant::now();
        let old = now.checked_sub(std::time::Duration::from_secs(60)).unwrap();
        let (text, since) = update_tracker(
            Some("same".to_string()),
            Some(old),
            Some("same".to_string()),
            now,
        );
        assert_eq!(text, Some("same".to_string()));
        assert_eq!(since, Some(old));
        assert!(!is_within_freshness(
            now.duration_since(since.unwrap()).as_secs()
        ));
    }

    #[test]
    fn first_sight_sets_timestamp() {
        let now = Instant::now();
        let (text, since) = update_tracker(None, None, Some("fresh".to_string()), now);
        assert_eq!(text, Some("fresh".to_string()));
        assert_eq!(since, Some(now));
    }

    #[test]
    fn cleared_clipboard_resets_to_none() {
        let now = Instant::now();
        let old = now.checked_sub(std::time::Duration::from_secs(5)).unwrap();
        let (text, since) = update_tracker(Some("old".to_string()), Some(old), None, now);
        assert_eq!(text, None);
        assert_eq!(since, Some(now));
    }
}
