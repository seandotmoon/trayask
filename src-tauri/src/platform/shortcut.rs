use tauri::Emitter;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// Global accelerator: Cmd+/ on macOS, Ctrl+/ on Linux/Windows.
pub const SHORTCUT_ACCEL: &str = "CmdOrCtrl+/";

/// Hint shown when the global shortcut cannot be registered due to
/// missing macOS accessibility / input-monitoring permission.
pub const A11Y_HINT: &str = "Enable TrayAsk in System Settings → Privacy & Security → Accessibility (and Input Monitoring), then restart.";

/// Payload emitted to the frontend when the shortcut fires.
#[derive(Clone, serde::Serialize)]
struct ShortcutQueryPayload {
    text: String,
}

/// Payload emitted to the frontend when shortcut registration fails.
#[derive(Clone, serde::Serialize)]
struct ShortcutErrorPayload {
    kind: String,
    hint: String,
}

/// Map a registration failure reason to `(kind, hint)`.
/// Pure and unit-tested. Reasons mentioning access/permission map to
/// the accessibility hint; everything else is a shortcut conflict.
pub fn shortcut_error_hint(reason: &str) -> (&'static str, String) {
    if let Some(hint) = super::win::windows_shortcut_hint(reason) {
        return hint;
    }
    if let Some(hint) = super::linux::linux_shortcut_hint(reason) {
        return hint;
    }
    let lower = reason.to_lowercase();
    if lower.contains("access") || lower.contains("permission") {
        ("accessibility", A11Y_HINT.to_string())
    } else {
        (
            "conflict",
            format!(
                "Shortcut {SHORTCUT_ACCEL} is already in use by another app. \
                 Disable the conflicting shortcut, then restart TrayAsk."
            ),
        )
    }
}

/// Register the global shortcut. On trigger: show the panel, read the
/// clipboard (text-only, fresh within 30s — older content is treated as
/// empty), and emit `shortcut-query` with `{ text }`
/// (empty string when the clipboard has no usable or no fresh text; the
/// frontend decides whether to auto-submit).
///
/// Conflicts/errors are only logged (plus a best-effort `shortcut-error`
/// event for the frontend banner): this returns `Ok(())` even when
/// registration fails so a shortcut conflict never breaks app boot.
pub fn register(app: &tauri::AppHandle) -> tauri::Result<()> {
    if let Err(e) = app
        .global_shortcut()
        .on_shortcut(SHORTCUT_ACCEL, |app, _shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            super::windows::show_panel(app);
            let text = super::clipboard::read_clipboard_text_fresh(app).unwrap_or_default();
            // Log char count only (never content): distinguishes "clipboard
            // empty/stale/unreadable" from "event lost" when the panel opens empty.
            eprintln!(
                "[trayask] shortcut {SHORTCUT_ACCEL} pressed, emitting shortcut-query ({} chars)",
                text.chars().count()
            );
            if let Err(e) = app.emit("shortcut-query", ShortcutQueryPayload { text }) {
                eprintln!("[trayask] failed to emit shortcut-query: {e}");
            }
        })
    {
        eprintln!("[trayask] global shortcut conflict ({SHORTCUT_ACCEL}): {e}");
        let (kind, hint) = shortcut_error_hint(&e.to_string());
        let _ = app.emit(
            "shortcut-error",
            ShortcutErrorPayload {
                kind: kind.to_string(),
                hint,
            },
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{shortcut_error_hint, A11Y_HINT};

    #[test]
    fn access_reason_maps_to_accessibility() {
        let (kind, hint) = shortcut_error_hint("access denied by TCC");
        assert_eq!(kind, "accessibility");
        assert_eq!(hint, A11Y_HINT);
    }

    #[test]
    fn permission_reason_maps_to_accessibility() {
        let (kind, _) = shortcut_error_hint("missing permission to listen");
        assert_eq!(kind, "accessibility");
    }

    #[test]
    fn other_reason_maps_to_conflict() {
        let (kind, hint) = shortcut_error_hint("already registered by another app");
        assert_eq!(kind, "conflict");
        assert!(hint.contains("already in use"));
    }

    #[test]
    fn wayland_reason_maps_to_wayland() {
        let (kind, _) = shortcut_error_hint("compositor denied bind on wayland");
        assert_eq!(kind, "wayland");
    }

    #[test]
    fn elevation_reason_maps_to_elevation() {
        let (kind, hint) = shortcut_error_hint("registration requires elevation (UAC)");
        assert_eq!(kind, "elevation");
        assert!(hint.contains("administrator"));
    }
}
