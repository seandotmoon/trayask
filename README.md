# TrayAsk

Tray widget: connect provider, choose model, chat from shortcut or system bar.
(Tauri 2.x + Vite + React + TS. Updater disabled for now.)

## Prereqs

- Rust (1.90+): https://rustup.rs
- Node 20+ + pnpm: `npm install -g pnpm`
- Tauri system deps: https://tauri.app/start/prerequisites/
  - macOS: Xcode CLT (`xcode-select --install`)
  - Linux (Debian/Ubuntu): `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev patchelf` etc. (see link above)
  - Windows: WebView2 + VS Build Tools (see link above)

## Dev

```sh
cd trayask
pnpm install
just dev        # opens empty window (S01); full panel lands in later sessions
```

Or without `just`: `pnpm tauri dev`.

## Build

```sh
just build      # unsigned .dmg on Mac; per-OS bundle elsewhere
```

Or: `pnpm tauri build`.

## Lint / test

```sh
just lint       # eslint + cargo fmt --check
just test       # cargo test + tsc --noEmit
```
