# Linux support

X11 is the primary supported session. Wayland works with a fallback:
click the tray icon and paste instead of relying on the global shortcut.

## Dependencies

```bash
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  patchelf \
  xclip \
  wl-clipboard
```

Runtime deb depends: `libwebkit2gtk-4.1-0`, `libayatana-appindicator3-1`
(see `src-tauri/tauri.conf.json`).

## Shortcut

Global shortcut is `Ctrl+/`. On Wayland compositors the global shortcut
may fail to register; the app shows a Wayland hint and the fallback is
tray-click + paste. X11/XWayland sessions support the shortcut normally.

## Clipboard

- X11: clipboard via `xclip`.
- Wayland: clipboard via `wl-clipboard` (`wl-paste`).
- Run `bash scripts/check-linux-clipboard.sh` to probe installed tools
  (probe only, always exits 0).

## Keyring

Secrets use the system keyring via Secret Service (`gnome-keyring` or
equivalent must be running).

## Autostart

Autostart uses XDG autostart entries on Linux.

## Packaging

Bundle targets: `deb` + `appimage` (see `tauri.conf.json`
`bundle.targets`). Build with `pnpm tauri build` from `trayask/`.

## QA checklist (GNOME + X11)

- [ ] Tray icon appears; left-click toggles the panel.
- [ ] `Ctrl+/` opens the panel with clipboard text.
- [ ] Streaming reply renders in the panel.
