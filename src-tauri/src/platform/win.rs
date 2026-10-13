//! Windows-OS platform helpers.
//!
//! Pure functions only (no `tauri::AppHandle` dependency) so the unit
//! tests run on any host OS, including Linux/macOS CI.
//!
//! NOTE: this module is Windows-OS helpers. Window *management*
//! (panel show/hide) lives in `platform::windows` — do not confuse
//! the two (`win` = OS, `windows` = panel windows).

/// Windows global-shortcut label shown in UI hints.
pub fn windows_shortcut_label() -> &'static str {
    "Ctrl+/"
}

/// Official WebView2 Evergreen bootstrapper download URL.
pub fn webview2_download_url() -> &'static str {
    "https://go.microsoft.com/fwlink/p/?LinkId=2124703"
}

/// Hint shown when the WebView2 Runtime is missing: points at the
/// embedded bootstrapper and documents the offline fallback.
pub fn webview2_bootstrapper_hint() -> &'static str {
    "WebView2 Runtime is required. The installer embeds the WebView2 \
     bootstrapper, which downloads the Runtime on install. For offline \
     machines, install the WebView2 Runtime manually from \
     https://go.microsoft.com/fwlink/p/?LinkId=2124703 first, then reinstall TrayAsk."
}

/// Hint describing secret storage on Windows: same `keys.rs` API
/// (`trayask-<provider>` / `api-key`), backed by Windows Credential
/// Manager. No code change needed — the `keyring` crate routes to
/// Credential Manager automatically.
pub fn credential_manager_hint() -> &'static str {
    "API keys are stored in Windows Credential Manager via the same \
     keys.rs API (service `trayask-<provider>`, account `api-key`); \
     no code change needed."
}

/// Map a shortcut-registration failure reason to an elevation hint.
///
/// Returns `Some(("elevation", hint))` when `reason` mentions
/// elevation/admin/UAC (case-insensitive); otherwise `None` so the
/// caller falls through to the existing logic.
pub fn windows_shortcut_hint(reason: &str) -> Option<(&'static str, String)> {
    let lower = reason.to_lowercase();
    if lower.contains("elevation")
        || lower.contains("admin")
        || lower.contains("uac")
        || lower.contains("elevat")
    {
        Some((
            "elevation",
            "Global shortcut registration needs elevation: run TrayAsk \
             as administrator, or register the shortcut from an elevated \
             process (UAC prompt), then restart."
                .to_string(),
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_label_is_ctrl_slash() {
        assert_eq!(windows_shortcut_label(), "Ctrl+/");
    }

    #[test]
    fn download_url_is_evergreen_bootstrapper() {
        assert_eq!(
            webview2_download_url(),
            "https://go.microsoft.com/fwlink/p/?LinkId=2124703"
        );
    }

    #[test]
    fn bootstrapper_hint_mentions_runtime_and_offline() {
        let hint = webview2_bootstrapper_hint();
        assert!(hint.contains("WebView2 Runtime"));
        assert!(hint.contains("bootstrapper"));
        assert!(hint.to_lowercase().contains("offline"));
    }

    #[test]
    fn credential_hint_mentions_manager_and_key_ids() {
        let hint = credential_manager_hint();
        assert!(hint.contains("Credential Manager"));
        assert!(hint.contains("trayask-<provider>"));
        assert!(hint.contains("api-key"));
    }

    #[test]
    fn elevation_reasons_map_to_hint() {
        for reason in [
            "registration requires elevation",
            "access denied: run as admin",
            "blocked by UAC policy",
            "ELEVATION required for global hook",
            "Administrator privileges needed",
        ] {
            let mapped = windows_shortcut_hint(reason);
            assert!(mapped.is_some(), "expected hint for: {reason}");
            let (kind, _) = mapped.unwrap();
            assert_eq!(kind, "elevation");
        }
    }

    #[test]
    fn non_elevation_reasons_return_none() {
        assert!(windows_shortcut_hint("already registered by another app").is_none());
        assert!(windows_shortcut_hint("access denied").is_none());
        assert!(windows_shortcut_hint("compositor denied bind on wayland").is_none());
        assert!(windows_shortcut_hint("").is_none());
    }

    #[test]
    fn elevation_hint_mentions_admin_and_uac() {
        let (_, hint) = windows_shortcut_hint("needs elevation").unwrap();
        assert!(hint.to_lowercase().contains("administrator"));
        assert!(hint.contains("UAC"));
    }
}
