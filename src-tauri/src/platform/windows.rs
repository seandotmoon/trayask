use std::path::PathBuf;

use tauri::{Manager, PhysicalPosition, Position, WebviewWindow};

/// Persisted panel position (physical pixels).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PanelPos {
    pub x: i32,
    pub y: i32,
}

pub const PANEL_POS_FILE: &str = "panel-pos.json";

/// Returns true when a window-close request should hide the window instead
/// of destroying it. `panel`, `settings`, and `help` are kept alive so the
/// tray menu can re-show them after the user closes them via X.
/// Unknown labels fall through to the default (destroy) behavior.
pub fn should_hide_on_close(label: &str) -> bool {
    label == "panel" || label == "settings" || label == "help"
}

/// Pure clamp: keep a `win_w` x `win_h` window fully on a
/// `screen_w` x `screen_h` screen.
pub fn clamp_panel_pos(
    x: i32,
    y: i32,
    screen_w: i32,
    screen_h: i32,
    win_w: i32,
    win_h: i32,
) -> (i32, i32) {
    let x = x.clamp(0, (screen_w - win_w).max(0));
    let y = y.clamp(0, (screen_h - win_h).max(0));
    (x, y)
}

/// Pure default anchor: top-right corner with `margin` offset,
/// clamped so the window stays fully on screen.
pub fn default_top_right(
    screen_w: i32,
    screen_h: i32,
    win_w: i32,
    win_h: i32,
    margin: i32,
) -> (i32, i32) {
    clamp_panel_pos(
        screen_w - win_w - margin,
        margin,
        screen_w,
        screen_h,
        win_w,
        win_h,
    )
}

/// File the panel position is persisted to (app data dir).
pub fn panel_pos_path(app: &tauri::AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .map(|d| d.join(PANEL_POS_FILE))
        .unwrap_or_else(|_| PathBuf::from(PANEL_POS_FILE))
}

/// Best-effort save; all errors are ignored.
pub fn save_panel_pos(app: &tauri::AppHandle, x: i32, y: i32) {
    let path = panel_pos_path(app);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    if let Ok(s) = serde_json::to_string(&PanelPos { x, y }) {
        let _ = std::fs::write(path, s);
    }
}

/// Best-effort load; returns `None` when missing or unparseable.
pub fn load_panel_pos(app: &tauri::AppHandle) -> Option<PanelPos> {
    let s = std::fs::read_to_string(panel_pos_path(app)).ok()?;
    serde_json::from_str(&s).ok()
}

/// Remember the window's current position. Ignores all errors.
pub fn remember_panel_position(window: &WebviewWindow) {
    if let Ok(pos) = window.outer_position() {
        save_panel_pos(window.app_handle(), pos.x, pos.y);
    }
}

/// Restore the saved position (clamped to the current monitor when the
/// geometry is known); fall back to the top-right corner when nothing was saved.
/// Ignores all errors.
pub fn restore_panel_position(window: &WebviewWindow, app: &tauri::AppHandle) {
    let Some(saved) = load_panel_pos(app) else {
        let (win_w, win_h) = window
            .outer_size()
            .map(|s| (s.width as i32, s.height as i32))
            .unwrap_or((380, 560));
        let (scr_w, scr_h) = window
            .current_monitor()
            .ok()
            .flatten()
            .map(|m| {
                let s = m.size();
                (s.width as i32, s.height as i32)
            })
            .unwrap_or((1920, 1080));
        let (x, y) = default_top_right(scr_w, scr_h, win_w, win_h, 12);
        let _ = window.set_position(Position::Physical(PhysicalPosition { x, y }));
        return;
    };
    let (win_w, win_h) = window
        .outer_size()
        .map(|s| (s.width as i32, s.height as i32))
        .unwrap_or((400, 600));
    let (scr_w, scr_h) = window
        .current_monitor()
        .ok()
        .flatten()
        .map(|m| {
            let s = m.size();
            (s.width as i32, s.height as i32)
        })
        .unwrap_or((1920, 1080));
    let (x, y) = clamp_panel_pos(saved.x, saved.y, scr_w, scr_h, win_w, win_h);
    let _ = window.set_position(Position::Physical(PhysicalPosition { x, y }));
}

pub fn show_panel(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("panel") {
        restore_panel_position(&win, app);
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
}

pub fn hide_panel(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("panel") {
        remember_panel_position(&win);
        let _ = win.hide();
    }
}

pub fn toggle_panel(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("panel") {
        if win.is_visible().unwrap_or(false) {
            remember_panel_position(&win);
            let _ = win.hide();
        } else {
            show_panel(app);
        }
    }
}

pub fn show_settings(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
}

pub fn show_help(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("help") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::{clamp_panel_pos, default_top_right, should_hide_on_close, PanelPos};

    #[test]
    fn panel_hides_instead_of_closing() {
        assert!(should_hide_on_close("panel"));
    }

    #[test]
    fn settings_hides_instead_of_closing() {
        assert!(should_hide_on_close("settings"));
    }

    #[test]
    fn help_hides_instead_of_closing() {
        assert!(should_hide_on_close("help"));
    }

    #[test]
    fn unknown_window_keeps_default_close() {
        assert!(!should_hide_on_close("main"));
        assert!(!should_hide_on_close(""));
    }

    #[test]
    fn clamp_keeps_inside_screen() {
        assert_eq!(clamp_panel_pos(100, 100, 1920, 1080, 400, 600), (100, 100));
        assert_eq!(clamp_panel_pos(-50, -20, 1920, 1080, 400, 600), (0, 0));
        assert_eq!(
            clamp_panel_pos(2000, 1500, 1920, 1080, 400, 600),
            (1520, 480)
        );
    }

    #[test]
    fn clamp_handles_oversized_window() {
        assert_eq!(clamp_panel_pos(300, 300, 800, 600, 1000, 900), (0, 0));
    }

    #[test]
    fn clamp_handles_zero_screen() {
        assert_eq!(clamp_panel_pos(50, 50, 0, 0, 400, 600), (0, 0));
        assert_eq!(clamp_panel_pos(-10, -10, 0, 0, 400, 600), (0, 0));
    }

    #[test]
    fn panel_pos_roundtrips_json() {
        let pos = PanelPos { x: 12, y: 34 };
        let s = serde_json::to_string(&pos).unwrap();
        assert_eq!(serde_json::from_str::<PanelPos>(&s).unwrap(), pos);
    }

    #[test]
    fn default_top_right_math() {
        assert_eq!(default_top_right(1920, 1080, 380, 560, 12), (1528, 12));
    }

    #[test]
    fn default_top_right_applies_margin() {
        assert_eq!(default_top_right(1920, 1080, 380, 560, 0), (1540, 0));
        assert_eq!(default_top_right(1920, 1080, 380, 560, 12), (1528, 12));
    }

    #[test]
    fn default_top_right_clamps_oversized_window() {
        assert_eq!(default_top_right(800, 600, 1000, 900, 12), (0, 0));
    }
}
