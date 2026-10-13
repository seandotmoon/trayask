# Windows support

Windows 11 is supported. The app uses the system WebView2 Runtime,
Windows Credential Manager for secrets, and a tray icon with a
`Ctrl+/` global shortcut.

## Shortcut

Global shortcut is `Ctrl+/`. If registration fails with an
elevation/admin/UAC error, the app shows an elevation hint: run TrayAsk
as administrator (or register the shortcut from an elevated process),
then restart.

## WebView2

TrayAsk requires the WebView2 Runtime. The installer embeds the
WebView2 bootstrapper (`webviewInstallMode: embedBootstrapper` in
`src-tauri/tauri.conf.json`), which downloads the Runtime on install.

- Bootstrapper URL: `https://go.microsoft.com/fwlink/p/?LinkId=2124703`
- Offline machines: install the WebView2 Runtime manually from the URL
  above first, then install TrayAsk.
- Run `pwsh scripts/check-windows-env.ps1` to probe for an installed
  Runtime (probe only, always exits 0).

## Keyring

Secrets use Windows Credential Manager via the same `keys.rs` API as
other platforms (service `trayask-<provider>`, account `api-key`).
No code change is needed — the `keyring` crate routes to Credential
Manager automatically.

## Tray behavior

Click the tray icon to toggle the panel; `Esc` or focus loss hides it.
The panel is a frameless always-on-top window (380x560); Settings is a
separate 520x480 window.

## Autostart

Autostart uses the login-item toggle (same autostart plugin as other
platforms); enable it in Settings to start TrayAsk at login.

## Packaging

Bundle targets: `msi` + `nsis` (see `tauri.conf.json`
`bundle.targets`). Build with `pnpm tauri build` from `trayask/`.

## QA checklist (Windows 11)

- [ ] Tray icon appears; click toggles the panel.
- [ ] `Ctrl+/` opens the panel with clipboard text.
- [ ] Streaming reply renders in the panel.
