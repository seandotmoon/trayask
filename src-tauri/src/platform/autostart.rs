use tauri_plugin_autostart::ManagerExt;

/// Returns true when launch-at-login is currently enabled.
/// Never panics: any backend error maps to `false`.
pub fn is_launch_at_login(app: &tauri::AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

/// Map an autostart backend error to a user-facing string.
/// Pure and unit-tested; keeps error text consistent across callers.
pub fn autostart_err_msg(op: &str, e: &impl std::fmt::Display) -> String {
    format!("Failed to {op} launch at login: {e}")
}

/// Enable or disable launch-at-login. Errors are returned as strings, never panics.
pub fn set_launch_at_login(app: &tauri::AppHandle, enable: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    if enable {
        manager
            .enable()
            .map_err(|e| autostart_err_msg("enable", &e))
    } else {
        manager
            .disable()
            .map_err(|e| autostart_err_msg("disable", &e))
    }
}

#[tauri::command]
pub fn get_autostart(app: tauri::AppHandle) -> bool {
    is_launch_at_login(&app)
}

#[tauri::command]
pub fn set_autostart(app: tauri::AppHandle, enable: bool) -> Result<(), String> {
    set_launch_at_login(&app, enable)
}

#[cfg(test)]
mod tests {
    use super::autostart_err_msg;

    #[test]
    fn enable_error_mentions_enable() {
        let msg = autostart_err_msg("enable", &"backend down");
        assert!(msg.contains("enable"), "{msg}");
        assert!(msg.contains("backend down"), "{msg}");
    }

    #[test]
    fn disable_error_mentions_disable() {
        let msg = autostart_err_msg("disable", &"denied");
        assert!(msg.contains("disable"), "{msg}");
        assert!(msg.contains("denied"), "{msg}");
    }
}
