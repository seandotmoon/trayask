//! Linux platform helpers.
//!
//! Pure functions only (no `tauri::AppHandle` dependency) so the unit
//! tests run on any host OS, including macOS CI.

/// Linux global-shortcut label shown in UI hints.
pub fn linux_shortcut_label() -> &'static str {
    "Ctrl+/"
}

/// Short fallback text shown when a global shortcut cannot be registered
/// on Wayland: X11/XWayland works, otherwise fall back to tray-click + paste.
pub fn wayland_fallback_text() -> &'static str {
    "Global shortcut may fail on Wayland compositors. X11/XWayland works; \
     otherwise click the tray icon and paste instead."
}

/// Hint describing the Linux tray backend (AppIndicator fallback).
pub fn tray_backend_hint() -> &'static str {
    "Linux tray uses AppIndicator (libayatana-appindicator3) with fallback; \
     install libayatana-appindicator3-1 if the tray icon is missing."
}

/// True when the session looks like Wayland: `WAYLAND_DISPLAY` is non-empty
/// or `XDG_SESSION_TYPE` equals "wayland" (case-insensitive).
pub fn is_wayland_session(env_wayland: Option<&str>, env_xdg_session: Option<&str>) -> bool {
    if env_wayland.is_some_and(|v| !v.is_empty()) {
        return true;
    }
    env_xdg_session.is_some_and(|v| v.eq_ignore_ascii_case("wayland"))
}

/// Map a shortcut-registration failure reason to a Wayland hint.
///
/// Returns `Some(("wayland", fallback))` when `reason` mentions
/// wayland/compositor; otherwise `None` so the caller falls through
/// to the existing access/conflict logic.
pub fn linux_shortcut_hint(reason: &str) -> Option<(&'static str, String)> {
    let lower = reason.to_lowercase();
    if lower.contains("wayland") || lower.contains("compositor") {
        Some(("wayland", wayland_fallback_text().to_string()))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_wayland_display() {
        assert!(is_wayland_session(Some("wayland-0"), None));
        assert!(is_wayland_session(Some("wayland-0"), Some("x11")));
    }

    #[test]
    fn detects_xdg_session_type_wayland() {
        assert!(is_wayland_session(None, Some("wayland")));
        assert!(is_wayland_session(None, Some("Wayland")));
        assert!(is_wayland_session(None, Some("WAYLAND")));
    }

    #[test]
    fn rejects_non_wayland_sessions() {
        assert!(!is_wayland_session(None, None));
        assert!(!is_wayland_session(Some(""), Some("x11")));
        assert!(!is_wayland_session(None, Some("x11")));
        assert!(!is_wayland_session(Some(""), None));
    }

    #[test]
    fn fallback_text_mentions_tray() {
        let text = wayland_fallback_text();
        assert!(text.to_lowercase().contains("tray"));
        assert!(text.contains("X11"));
    }

    #[test]
    fn shortcut_label_is_ctrl_slash() {
        assert_eq!(linux_shortcut_label(), "Ctrl+/");
    }

    #[test]
    fn tray_hint_mentions_appindicator() {
        let hint = tray_backend_hint();
        assert!(hint.contains("ayatana") || hint.contains("AppIndicator"));
    }

    #[test]
    fn wayland_reasons_map_to_hint() {
        for reason in [
            "failed on wayland compositor",
            "compositor denied global shortcut",
            "wayland: global shortcut not supported",
        ] {
            let mapped = linux_shortcut_hint(reason);
            assert!(mapped.is_some(), "expected hint for: {reason}");
            let (kind, _) = mapped.unwrap();
            assert_eq!(kind, "wayland");
        }
    }

    #[test]
    fn non_wayland_reasons_return_none() {
        assert!(linux_shortcut_hint("already registered by another app").is_none());
        assert!(linux_shortcut_hint("access denied").is_none());
        assert!(linux_shortcut_hint("could not bind shortcut").is_none());
        assert!(linux_shortcut_hint("").is_none());
    }
}
