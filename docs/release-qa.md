# Release QA

## Versioning

Single source of truth: `X.Y.Z` must match in all three files:

- `trayask/package.json` (`version`)
- `trayask/src-tauri/Cargo.toml` (`[package] version`)
- `trayask/src-tauri/tauri.conf.json` (`version`)

Use the bump script from the repo root or `trayask/`:

```bash
# Set a new version (updates all three files)
bash trayask/scripts/bump-version.sh 0.2.0

# Verify all three are in sync (CI-friendly, exits 0/1)
bash trayask/scripts/bump-version.sh --check
```

Tag releases as `vX.Y.Z` (e.g. `git tag v0.2.0 && git push origin v0.2.0`).
The `release.yml` workflow triggers on `v*` tags.

## Signing

Unsigned builds are OK initially for local QA (OS will show
Gatekeeper/SmartScreen warnings; use right-click Open / "More info > Run anyway").

### macOS (Apple Developer ID + notarization) — placeholder steps

1. Enroll in Apple Developer Program; create a Developer ID Application certificate.
2. Export the certificate to the CI Mac keychain (`APPLE_CERTIFICATE`,
   `APPLE_CERTIFICATE_PASSWORD`, `KEYCHAIN_PASSWORD` secrets).
3. Add notarization credentials (`APPLE_ID`, `APPLE_PASSWORD` app-specific,
   `APPLE_TEAM_ID`) and enable `tauri-action` notarization / `afterSign` hook.
4. Verify: `spctl -a -vvv TrayAsk.app`, `xcrun stapler validate TrayAsk.dmg`.

### Windows (Trusted Signing) — placeholder steps

1. Set up Azure Trusted Signing account + identity validation.
2. Add secrets (`AZURE_TENANT_ID`, `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`,
   endpoint/account/certificate-profile names).
3. Sign the installer in CI after `tauri-action` (e.g. `Azure/trusted-signing-action`).
4. Verify: no SmartScreen "Unknown publisher" warning on a clean VM.

Do NOT commit keys, certificates, or `.sig` private material to the repo.
Only `TAURI_SIGNING_PRIVATE_KEY` (updater key) lives as a GitHub secret.

## Updater

Wired via `tauri-plugin-updater` (see `src-tauri/Cargo.toml`,
`src-tauri/src/main.rs`, `plugins.updater` in `tauri.conf.json`,
`updater:default` capability).

### Key generation

```bash
cd trayask
pnpm tauri signer generate -w ~/.tauri/trayask.key
```

- Put the **public** key into `src-tauri/tauri.conf.json` → `plugins.updater.pubkey`.
- Put the **private** key into the GitHub secret `TAURI_SIGNING_PRIVATE_KEY`
  (and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` if a password was set).
- Current placeholders: `pubkey` is `""` and `endpoints` is
  `https://github.com/OWNER/REPO/releases/latest/download/latest.json`.
  Replace `OWNER/REPO` with the real repo before the first real release.

### Dry-run

1. Set a real `pubkey` + `endpoints`, commit.
2. Tag a release candidate: `git tag v0.1.1-rc.1 && git push origin v0.1.1-rc.1`.
3. Wait for `release.yml` to publish the draft/GitHub release with
   `latest.json` + signed artifacts.
4. Install the previous version locally, launch it, trigger "Check for updates".
5. Confirm: update found, downloads, installs, relaunches on the new version.
6. Rollback check: reinstall the old artifact if needed.

## QA matrix

Copy this table per release candidate; check each cell pass (`[x]`) or fail (`[ ]` + issue link).

Surfaces: panel window, settings window, global shortcut.
Providers: openai, anthropic, gemini, ollama.
OSes: macOS, Linux, Windows.

| Function | Provider | macOS (panel / settings / shortcut) | Linux (panel / settings / shortcut) | Windows (panel / settings / shortcut) |
|---|---|---|---|---|
| explain | openai | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] |
| explain | anthropic | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] |
| explain | gemini | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] |
| explain | ollama | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] |
| fix grammar | openai | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] |
| fix grammar | anthropic | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] |
| fix grammar | gemini | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] |
| fix grammar | ollama | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] |
| summarize | openai | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] |
| summarize | anthropic | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] |
| summarize | gemini | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] |
| summarize | ollama | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] |
| translate | openai | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] |
| translate | anthropic | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] |
| translate | gemini | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] |
| translate | ollama | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] | [ ] / [ ] / [ ] |

Notes:

- Streaming works and cancel stops output.
- Errors (bad key, offline, ollama down) show a clear message.
- Autostart toggle persists across restart.
- Updater dry-run passed (see Updater section).

## Release checklist

- [ ] `bash scripts/bump-version.sh --check` passes.
- [ ] `cargo test` passes in `src-tauri/`.
- [ ] `pnpm build` passes in `trayask/`.
- [ ] `tauri.conf.json` / `capabilities/default.json` parse as JSON.
- [ ] `release.yml` parses as YAML; matrix covers macos / ubuntu-22.04 / windows.
- [ ] `pubkey` + `endpoints` set to real values (no `OWNER/REPO` placeholder).
- [ ] Tag `vX.Y.Z`, CI green, artifacts attached (dmg, deb/appimage, msi/nsis) + `latest.json`.
- [ ] QA matrix above fully checked; failures have linked issues.
- [ ] Updater dry-run: old install detects + installs new version.
- [ ] No secrets committed to the repo.
