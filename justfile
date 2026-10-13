# TrayAsk justfile — run from `trayask/`

dev:
    pnpm tauri dev

build:
    pnpm tauri build

lint:
    pnpm exec eslint .
    cd src-tauri && cargo fmt --check

test:
    cd src-tauri && cargo test
    pnpm exec tsc --noEmit
