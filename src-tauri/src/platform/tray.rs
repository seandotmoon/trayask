use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

use super::windows;

pub fn build_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    let open_chat = MenuItem::with_id(app, "open_chat", "Open TrayAsk", true, None::<&str>)?;
    let open_settings = MenuItem::with_id(app, "open_settings", "Settings…", true, None::<&str>)?;
    let open_help = MenuItem::with_id(app, "open_help", "Help…", true, None::<&str>)?;
    let checked = super::autostart::is_launch_at_login(app);
    let launch_at_login = CheckMenuItem::with_id(
        app,
        "launch_at_login",
        "Launch at Login",
        true,
        checked,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit TrayAsk", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &open_chat,
            &sep1,
            &open_settings,
            &launch_at_login,
            &open_help,
            &sep2,
            &quit,
        ],
    )?;

    let launch_item = launch_at_login.clone();
    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("TrayAsk — ask from anywhere (CmdOrCtrl+/)")
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "open_chat" => windows::show_panel(app),
            "open_settings" => windows::show_settings(app),
            "open_help" => windows::show_help(app),
            "launch_at_login" => {
                let enable = !launch_item.is_checked().unwrap_or(false);
                match super::autostart::set_launch_at_login(app, enable) {
                    Ok(()) => {
                        let _ = launch_item.set_checked(enable);
                    }
                    Err(e) => eprintln!("[trayask] failed to set launch-at-login: {e}"),
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                windows::toggle_panel(tray.app_handle());
            }
        });

    #[cfg(target_os = "macos")]
    {
        builder = builder.icon_as_template(true);
    }

    // Dedicated tray glyph (transparent, no tile): the bundle app icon now
    // has a filled squircle background, which would render as a solid blob
    // in the menubar. macOS template mode recolors from the alpha channel.
    match tauri::image::Image::from_bytes(include_bytes!("../../icons/tray-icon.png")) {
        Ok(icon) => {
            builder = builder.icon(icon);
        }
        Err(e) => eprintln!("[trayask] failed to load tray icon: {e}"),
    }

    builder.build(app)?;
    Ok(())
}
