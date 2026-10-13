#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            trayask::platform::windows::show_panel(app);
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            trayask::platform::tray::build_tray(app.handle())?;
            trayask::platform::clipboard::spawn_clipboard_watcher(app.handle().clone());
            if let Err(e) = trayask::platform::shortcut::register(app.handle()) {
                eprintln!("[trayask] failed to register global shortcut: {e}");
            }
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            Ok(())
        })
        .on_window_event(|window, event| {
            // The panel stays open until closed manually (Esc, tray toggle).
            // Never auto-hide on focus loss (user-directed, overrides the
            // original KorTalk-like focus-loss behavior).
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Closing via X would destroy the window so tray-menu
                // Open/Settings could never re-show it. Hide instead to
                // keep the window alive.
                if trayask::platform::windows::should_hide_on_close(window.label()) {
                    if window.label() == "panel" {
                        if let Ok(pos) = window.outer_position() {
                            trayask::platform::windows::save_panel_pos(
                                window.app_handle(),
                                pos.x,
                                pos.y,
                            );
                        }
                    }
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            trayask::commands::chat,
            trayask::commands::cancel_chat,
            trayask::commands::save_provider,
            trayask::commands::list_providers,
            trayask::commands::delete_provider,
            trayask::commands::list_custom_models,
            trayask::commands::get_prefs,
            trayask::commands::save_prefs,
            trayask::commands::open_settings,
            trayask::commands::open_help,
            trayask::platform::autostart::get_autostart,
            trayask::platform::autostart::set_autostart,
            trayask::platform::clipboard::read_clipboard_fresh
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
