import { useEffect, useRef, useState, type SyntheticEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  FUNCTIONS,
  functionLabel,
  PROVIDERS,
  STATIC_MODELS,
  providerIdFromSelect,
  type FunctionKind,
} from "./lib/models";
import {
  DEFAULT_PREFS,
  loadPrefs,
  savePrefsBackend,
  loadPrefsBackend,
  shouldReloadPrefsOnWindowFocus,
  type Prefs,
} from "./lib/prefs";
import { renderMarkdown } from "./lib/markdown";
import helpTrayIcon from "./assets/help/tray-icon.png";
import helpTrayMenu from "./assets/help/tray-menu.png";
import helpPanel from "./assets/help/panel.png";
import helpProvider from "./assets/help/settings-provider.png";
import helpKeys from "./assets/help/settings-keys.png";
import helpGeneral from "./assets/help/settings-general.png";

type StreamPayload = { kind: "token" | "done" | "error"; text: string };
type Status = "idle" | "streaming" | "done" | "error" | "cancelled";
type ProviderStatus = { id: string; has_key: boolean };

function App() {
  const [windowLabel, setWindowLabel] = useState<string>("panel");
  const [prefs, setPrefs] = useState<Prefs>({ ...DEFAULT_PREFS });
  const [provider, setProvider] = useState(DEFAULT_PREFS.provider_id);
  const [model, setModel] = useState(DEFAULT_PREFS.model_id);
  const [fn, setFn] = useState<FunctionKind>(DEFAULT_PREFS.default_function);
  const [query, setQuery] = useState("");
  const [output, setOutput] = useState("");
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<Status>("idle");
  const [copied, setCopied] = useState(false);
  const [saved, setSaved] = useState(false);
  const [keyInputs, setKeyInputs] = useState<Record<string, string>>({});
  const [hasKeys, setHasKeys] = useState<Record<string, boolean>>({});
  const [keyMsg, setKeyMsg] = useState("");
  const [autostart, setAutostart] = useState(false);
  const [customModels, setCustomModels] = useState<string[]>([]);
  const [modelsLoading, setModelsLoading] = useState(false);
  const [modelsError, setModelsError] = useState("");
  const [providerMsg, setProviderMsg] = useState("");
  const lastFetched = useRef<{ base: string; key: string } | null>(null);
  // True after the user edits Settings and before a prefs reload applies.
  // An in-flight get_prefs must not snap the provider dropdown back.
  const settingsDraft = useRef(false);
  const [shortcutErr, setShortcutErr] = useState<{
    kind: string;
    hint: string;
  } | null>(null);
  const [isMaximized, setIsMaximized] = useState(false);
  const isMac =
    /Mac/.test(navigator.userAgent ?? "") ||
    /Mac/.test(navigator.platform ?? "");
  const isWindows =
    /Win/.test(navigator.userAgent ?? "") ||
    /Win/.test(navigator.platform ?? "");

  async function refreshKeys(): Promise<Record<string, boolean>> {
    try {
      const list = await invoke<ProviderStatus[]>("list_providers");
      const m: Record<string, boolean> = {};
      for (const p of list) m[p.id] = p.has_key;
      setHasKeys(m);
      return m;
    } catch (e) {
      // Surface instead of swallowing: a failed refresh leaves pills stale
      // ("no key" after a successful save), which reads as "key not saved".
      console.error("[trayask] list_providers failed:", e);
      throw e;
    }
  }

  useEffect(() => {
    let cancelled = false;
    const cached = loadPrefs();
    setPrefs(cached);
    setProvider(cached.provider_id);
    setModel(cached.model_id);
    setFn(cached.default_function);
    refreshKeys().catch(() => {
      // keychain unavailable on first paint; pills stay "no key" until retry
    });
    try {
      setWindowLabel(getCurrentWindow().label);
    } catch {
      // non-Tauri (browser dev): default to panel = chat-only
    }
    // Backend file is the source of truth across windows + restarts.
    // Skip if the user already moved the provider dropdown (Linux can
    // deliver this IPC after the native select popup has closed).
    void loadPrefsBackend().then((p) => {
      if (cancelled || settingsDraft.current) return;
      setPrefs(p);
      setProvider(p.provider_id);
      setModel(p.model_id);
      setFn(p.default_function);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    const unlisten = listen<StreamPayload>("stream-token", (e) => {
      const p = e.payload;
      if (p.kind === "token") {
        setOutput((o) => o + p.text);
      } else if (p.kind === "error") {
        setOutput((o) => o + `\n[error] ${p.text}`);
        setStatus("error");
        busyRef.current = false;
        setBusy(false);
      } else if (p.kind === "done") {
        setStatus("done");
        busyRef.current = false;
        setBusy(false);
      }
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  const isSettingsWindow = windowLabel === "settings";
  const isHelpWindow = windowLabel === "help";
  const isPanel = !isSettingsWindow && !isHelpWindow;

  useEffect(() => {
    if (!isPanel) return;
    getCurrentWindow()
      .isMaximized()
      .then(setIsMaximized)
      .catch(() => {});
    // Keep the glyph in sync when the OS maximizes/restores outside the
    // button (double-click drag region, Win+Up, Mission Control, etc.).
    let unlisten: (() => void) | undefined;
    getCurrentWindow()
      .onResized(() => {
        getCurrentWindow()
          .isMaximized()
          .then(setIsMaximized)
          .catch(() => {});
      })
      .then((f) => {
        unlisten = f;
      })
      .catch(() => {});
    return () => unlisten?.();
  }, [isPanel]);

  async function handleTitlebarMouseDown(e: React.MouseEvent) {
    // Primary-button drags on the bar background move the frameless panel.
    // Clicks on the traffic/☐ buttons must keep working, so ignore those.
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest("button")) return;
    try {
      await getCurrentWindow().startDragging();
    } catch {
      // non-Tauri (browser dev): ignore
    }
  }

  async function handleHide() {
    try {
      // Matches Esc / X behavior (main.rs CloseRequested → hide).
      await getCurrentWindow().hide();
    } catch {
      // non-Tauri (browser dev): ignore
    }
  }

  async function handleMinimize() {
    try {
      await getCurrentWindow().minimize();
    } catch {
      // non-Tauri (browser dev): ignore
    }
  }

  async function handleToggleMaximize() {
    try {
      await getCurrentWindow().toggleMaximize();
      setIsMaximized(await getCurrentWindow().isMaximized());
    } catch {
      // non-Tauri (browser dev): ignore
    }
  }

  async function openSettings() {
    try {
      await invoke("open_settings");
    } catch {
      // non-Tauri (browser dev): ignore
    }
  }

  async function openHelp() {
    try {
      await invoke("open_help");
    } catch {
      // non-Tauri (browser dev): ignore
    }
  }

  // Keep a live ref so the shortcut listener never reads stale prefs.
  const prefsRef = useRef<Prefs>({ ...DEFAULT_PREFS });
  useEffect(() => {
    prefsRef.current = prefs;
  }, [prefs]);

  // Synchronous busy flag: `setBusy` re-renders async, so a second shortcut
  // fire before the re-render would see a stale `busy === false` and invoke
  // `chat` twice (interleaved streams). This ref is set synchronously in
  // `send()`/`stop()` and makes the second fire a no-op.
  const busyRef = useRef(false);

  // Always-latest send() for the shortcut listeners (avoids stale closure).
  // Declared here (before the effects that use it) and refreshed every render.
  const sendRef = useRef<(q?: string) => Promise<void>>(async () => {});
  sendRef.current = send;

  function applyPrefs(p: Prefs) {
    settingsDraft.current = false;
    setPrefs(p);
    setProvider(p.provider_id);
    setModel(p.model_id);
    setFn(p.default_function);
  }

  useEffect(() => {
    // Live sync: settings Save emits this from the backend.
    const unlisten = listen<Prefs>("prefs-changed", (e) => {
      const p = e.payload;
      if (p && typeof p === "object") {
        applyPrefs({ ...DEFAULT_PREFS, ...p });
      }
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  useEffect(() => {
    // Panel may be hidden while settings change; refresh on focus.
    // Settings must not: see shouldReloadPrefsOnWindowFocus.
    let label = "panel";
    try {
      label = getCurrentWindow().label;
    } catch {
      // non-Tauri (browser dev): treat as the panel.
    }
    if (!shouldReloadPrefsOnWindowFocus(label)) return;
    const onFocus = () => {
      void loadPrefsBackend().then((p) => {
        if (settingsDraft.current) return;
        applyPrefs(p);
      });
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, []);

  useEffect(() => {
    if (isSettingsWindow) {
      refreshKeys().catch(() => {
        // keychain unavailable; pills stay "no key" until retry
      });
      invoke<boolean>("get_autostart")
        .then(setAutostart)
        .catch(() => {});
    }
  }, [isSettingsWindow]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        getCurrentWindow()
          .hide()
          .catch(() => {});
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Local Cmd/Ctrl+/ fallback for the focused panel: if the webview swallows
  // the keystroke so the Rust global shortcut never fires (or in browser
  // dev where there is no global shortcut), fill from the clipboard and
  // auto-send here. Mirrors the backend guards: fresh-within-30s, trim,
  // 4000-char truncate, empty/stale → hint only, `busyRef` single-flight.
  // If the global shortcut ALSO fires for the same press, its `send` is
  // dropped by `busyRef`.
  useEffect(() => {
    if (!isPanel) return;
    const onSlash = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.key !== "/") return;
      e.preventDefault();
      void (async () => {
        let raw = "";
        try {
          // Backend applies the 30s freshness gate (stale → "").
          raw = await invoke<string>("read_clipboard_fresh");
        } catch {
          try {
            // Browser dev: no backend, treat navigator.clipboard as fresh.
            raw = await navigator.clipboard.readText();
          } catch {
            return;
          }
        }
        const text = raw.trim().slice(0, 4000);
        setQuery(text);
        if (!text) {
          setOutput("_Clipboard is empty — type a question below._");
          setStatus("idle");
          return;
        }
        let auto = prefsRef.current.auto_submit;
        try {
          auto = (await loadPrefsBackend()).auto_submit;
        } catch {
          // backend unavailable (browser dev): fall back to the live ref.
        }
        if (auto) {
          void sendRef.current(text);
        }
      })();
    };
    window.addEventListener("keydown", onSlash);
    return () => window.removeEventListener("keydown", onSlash);
  }, [isPanel]);

  function handleProviderChange(e: SyntheticEvent<HTMLSelectElement>) {
    const id = providerIdFromSelect(e.currentTarget);
    settingsDraft.current = true;
    setProvider(id);
    const first = STATIC_MODELS[id]?.[0] ?? "";
    setModel(first);
  }

  async function send(overrideQuery?: string) {
    const q = overrideQuery ?? query;
    if (busyRef.current || !q.trim()) return;
    busyRef.current = true;
    setOutput("");
    setCopied(false);
    setBusy(true);
    setStatus("streaming");
    // The backend file is the source of truth: reload it so `custom_base_url`
    // (edited in the settings window) is never stale/empty here. Building on
    // fresh prefs also stops the write-back below from clobbering a saved URL
    // with panel-local state (e.g. auto-submit racing the mount load).
    const fresh = await loadPrefsBackend();
    const next: Prefs = {
      ...fresh,
      default_function: fn,
    };
    setPrefs(next);
    void savePrefsBackend(next);
    try {
      // Tauri IPC keys are camelCase by default (Rust `base_url` → JS `baseUrl`); do not "fix" to snake_case (B-07, regressed in B-02).
      const res = await invoke<string>("chat", {
        provider: fresh.provider_id,
        model: fresh.model_id,
        function: fn,
        query: q,
        baseUrl:
          fresh.provider_id === "custom" ? next.custom_base_url : undefined,
      });
      // The backend streams tokens via the `stream-token` event and resolves
      // after emitting `done`. Settle the UI here too so a missed event can
      // never wedge the panel in "streaming" forever.
      busyRef.current = false;
      setBusy(false);
      if (res === "cancelled") {
        setStatus("cancelled");
      } else {
        setStatus((s) => (s === "streaming" ? "done" : s));
      }
    } catch (e) {
      setOutput((o) => o + `\n[error] ${String(e)}`);
      setStatus("error");
      busyRef.current = false;
      setBusy(false);
    }
  }

  useEffect(() => {
    const unlisten = listen<{ text: string }>("shortcut-query", (e) => {
      // Both windows run this bundle: only the panel fills + sends, so one
      // shortcut press can never double-invoke `chat` from two windows.
      try {
        if (getCurrentWindow().label !== "panel") return;
      } catch {
        // non-Tauri (browser dev): proceed as the panel.
      }
      const text = e.payload?.text ?? "";
      setQuery(text);
      if (!text.trim()) {
        // Empty/stale clipboard (>30s old arrives as ""): hint only, never call chat.
        setOutput("_Clipboard is empty — type a question below._");
        setStatus("idle");
        return;
      }
      // Frontend owns auto-submit. Read the backend file fresh instead of
      // `prefsRef`: settings may have saved while this panel was hidden and
      // the focus-reload may not have run yet when this event lands.
      void (async () => {
        let auto = prefsRef.current.auto_submit;
        try {
          auto = (await loadPrefsBackend()).auto_submit;
        } catch {
          // backend unavailable (browser dev): fall back to the live ref.
        }
        if (auto) {
          void sendRef.current(text);
        }
      })();
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  useEffect(() => {
    const unlistenErr = listen<{ kind: string; hint: string }>(
      "shortcut-error",
      (e) => {
        setShortcutErr(e.payload);
      },
    );
    return () => {
      unlistenErr.then((f) => f());
    };
  }, []);

  async function stop() {
    try {
      await invoke("cancel_chat");
    } catch {
      // ignore cancel errors
    }
    busyRef.current = false;
    setBusy(false);
    setStatus("cancelled");
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(output);
    } catch {
      const ta = document.createElement("textarea");
      ta.value = output;
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      document.body.removeChild(ta);
    }
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }

  async function saveSettings() {
    const next: Prefs = {
      ...prefs,
      provider_id: provider,
      model_id: model,
      default_function: fn,
    };
    setPrefs(next);
    try {
      await savePrefsBackend(next);
    } catch {
      // local cache already updated; backend error ignored
    }
    setSaved(true);
    setTimeout(() => setSaved(false), 1500);
  }

  async function resetSettings() {
    const next = { ...DEFAULT_PREFS };
    setPrefs(next);
    setProvider(next.provider_id);
    setModel(next.model_id);
    setFn(next.default_function);
    try {
      await savePrefsBackend(next);
    } catch {
      // ignore
    }
  }

  async function saveKey(id: string) {
    setKeyMsg("");
    try {
      await invoke("save_provider", { provider: id, key: keyInputs[id] ?? "" });
      setKeyInputs((m) => ({ ...m, [id]: "" }));
      // Optimistic: reflect the successful save immediately so the pill
      // never reads stale. The refresh below then confirms against the
      // keychain; if it disagrees we say so explicitly instead of silently
      // showing "no key".
      setHasKeys((m) => ({ ...m, [id]: true }));
      try {
        const m = await refreshKeys();
        if (!m[id]) {
          setKeyMsg(
            `${id}: save reported ok but keychain read-back found nothing for "trayask-${id}" — fully restart \`tauri dev\` / rebuild via \`tauri build\` to pick up native keychain support, then check the terminal logs and Keychain Access (macOS) / Credential Manager (Windows) / Secret Service (Linux).`,
          );
          return;
        }
      } catch (e) {
        setKeyMsg(`${id}: saved, but status refresh failed: ${String(e)}`);
        return;
      }
      setKeyMsg(`${id}: saved ✓`);
    } catch (e) {
      setKeyMsg(`${id}: ${String(e)}`);
    }
  }

  async function deleteKey(id: string) {
    setKeyMsg("");
    try {
      await invoke("delete_provider", { provider: id });
      setKeyInputs((m) => ({ ...m, [id]: "" }));
      setHasKeys((m) => ({ ...m, [id]: false }));
      await refreshKeys().catch(() => {});
      setKeyMsg(`${id}: deleted`);
    } catch (e) {
      setKeyMsg(`${id}: ${String(e)}`);
    }
  }

  async function handleSaveProvider() {
    const base = prefs.custom_base_url;
    const key = keyInputs[provider] ?? "";
    const next: Prefs = {
      ...prefs,
      provider_id: provider,
      model_id: model,
      default_function: fn,
    };
    setPrefs(next);
    setProviderMsg("");
    try {
      await savePrefsBackend(next);
    } catch (e) {
      setProviderMsg(String(e));
      return;
    }
    if (provider === "custom") {
      // Persist the custom key together with the prefs so one Save
      // covers all provider info. Empty input leaves the stored key alone
      // (use Delete to remove it).
      if (key.trim()) {
        try {
          await invoke("save_provider", { provider: "custom", key });
          setKeyInputs((m) => ({ ...m, custom: "" }));
          setHasKeys((m) => ({ ...m, custom: true }));
          const m = await refreshKeys().catch(() => null);
          if (m && !m["custom"]) {
            setProviderMsg(
              `custom: save reported ok but keychain read-back found nothing for "trayask-custom" — fully restart \`tauri dev\` / rebuild via \`tauri build\` to pick up native keychain support, then check the terminal logs and Keychain Access (macOS) / Credential Manager (Windows) / Secret Service (Linux).`,
            );
            return;
          }
        } catch (e) {
          setProviderMsg(`key: ${String(e)}`);
          return;
        }
      }
      setProviderMsg("saved ✓");
      await fetchCustomModels(base, key);
    } else {
      // Same one-Save behavior for built-in providers (openai/anthropic/
      // gemini): a non-empty key input for the selected provider is saved
      // to the keychain together with the prefs. Empty input leaves the
      // stored key alone (use Delete in the Keys section to remove it).
      // Without this, typing a key and clicking this Save silently dropped
      // the key, which read as "key not saved".
      if (key.trim()) {
        try {
          await invoke("save_provider", { provider, key });
          setKeyInputs((m) => ({ ...m, [provider]: "" }));
          setHasKeys((m) => ({ ...m, [provider]: true }));
          const m = await refreshKeys().catch(() => null);
          if (m && !m[provider]) {
            setProviderMsg(
              `${provider}: save reported ok but keychain read-back found nothing for "trayask-${provider}" — fully restart \`tauri dev\` / rebuild via \`tauri build\` to pick up native keychain support, then check the terminal logs and Keychain Access (macOS) / Credential Manager (Windows) / Secret Service (Linux).`,
            );
            return;
          }
        } catch (e) {
          setProviderMsg(`key: ${String(e)}`);
          return;
        }
      }
      setProviderMsg("saved ✓");
    }
  }

  async function handleDeleteCustomKey() {
    setKeyMsg("");
    setProviderMsg("");
    try {
      await invoke("delete_provider", { provider: "custom" });
      setKeyInputs((m) => ({ ...m, custom: "" }));
      await refreshKeys();
      setKeyMsg("custom: deleted");
      setProviderMsg("deleted");
    } catch (e) {
      setKeyMsg(`custom: ${String(e)}`);
      setProviderMsg(String(e));
    }
  }

  async function fetchCustomModels(base: string, key: string) {
    const trimmedBase = base.trim();
    if (!trimmedBase) return;
    const prev = lastFetched.current;
    if (
      prev &&
      prev.base === trimmedBase &&
      prev.key === key &&
      customModels.length > 0
    ) {
      return;
    }
    setModelsLoading(true);
    setModelsError("");
    try {
      // Tauri IPC keys are camelCase by default (Rust `base_url` → JS `baseUrl`); do not "fix" to snake_case (B-07, regressed in B-02).
      const list = await invoke<string[]>("list_custom_models", {
        baseUrl: trimmedBase,
        key: key.trim() ? key : undefined,
      });
      setCustomModels(list);
      if (!model.trim() && list.length > 0) {
        setModel(list[0]);
      }
    } catch (e) {
      setModelsError(String(e));
    } finally {
      lastFetched.current = { base: trimmedBase, key };
      setModelsLoading(false);
    }
  }

  const models = STATIC_MODELS[provider] ?? [];

  return (
    <main
      className={
        "container" +
        (isSettingsWindow || isHelpWindow ? " settings-window" : " panel-window") +
        (isMaximized && isPanel ? " is-maximized" : "")
      }
    >
      {isSettingsWindow ? (
        <header className="header">
          <h1>TrayAsk Settings</h1>
          <button type="button" onClick={() => void openHelp()}>
            Help
          </button>
        </header>
      ) : isPanel ? (
        <header
          className={isMac ? "titlebar mac" : "titlebar"}
          data-tauri-drag-region
          onMouseDown={(e) => void handleTitlebarMouseDown(e)}
        >
          {isMac && (
            <div className="traffic" role="group" aria-label="Window controls">
              <button
                type="button"
                className="tl tl-close"
                title="Close"
                aria-label="Close"
                onClick={() => void handleHide()}
              >
                <svg viewBox="0 0 8 8" width="8" height="8" aria-hidden="true">
                  <path
                    d="M1.6 1.6l4.8 4.8M6.4 1.6L1.6 6.4"
                    stroke="currentColor"
                    strokeWidth="1.1"
                    fill="none"
                    strokeLinecap="round"
                  />
                </svg>
              </button>
              <button
                type="button"
                className="tl tl-min"
                title="Minimize"
                aria-label="Minimize"
                onClick={() => void handleMinimize()}
              >
                <svg viewBox="0 0 8 8" width="8" height="8" aria-hidden="true">
                  <path
                    d="M1.6 4h4.8"
                    stroke="currentColor"
                    strokeWidth="1.1"
                    fill="none"
                    strokeLinecap="round"
                  />
                </svg>
              </button>
              <button
                type="button"
                className="tl tl-max"
                title={isMaximized ? "Restore" : "Maximize"}
                aria-label={isMaximized ? "Restore" : "Maximize"}
                onClick={() => void handleToggleMaximize()}
              >
                {isMaximized ? (
                  <svg
                    viewBox="0 0 10 10"
                    width="8"
                    height="8"
                    aria-hidden="true"
                  >
                    <path
                      d="M9 6v3H6M9 9 5.5 5.5M1 4V1h3M1 1l3.5 3.5"
                      stroke="currentColor"
                      strokeWidth="1.2"
                      fill="none"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                    />
                  </svg>
                ) : (
                  <svg
                    viewBox="0 0 10 10"
                    width="8"
                    height="8"
                    aria-hidden="true"
                  >
                    <path
                      d="M6 1h3v3M9 1 5.5 4.5M4 9H1V6M1 9l3.5-3.5"
                      stroke="currentColor"
                      strokeWidth="1.2"
                      fill="none"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                    />
                  </svg>
                )}
              </button>
            </div>
          )}
          {!isMac && (
            <button
              type="button"
              className="win-btn"
              title={isMaximized ? "Restore" : "Maximize"}
              aria-label={isMaximized ? "Restore" : "Maximize"}
              onClick={() => void handleToggleMaximize()}
            >
              <span aria-hidden="true">{isMaximized ? "❐" : "☐"}</span>
            </button>
          )}
        </header>
      ) : null}

      {isPanel && (
        <section className="panel">
          {shortcutErr && (
            <p className="hint">
              Shortcut unavailable ({shortcutErr.kind}): {shortcutErr.hint}
            </p>
          )}
          <p className="hint using-row">
            <span className="using-label">
              <span
                className="using-badge is-provider"
                title={prefs.provider_id}
              >
                {prefs.provider_id}
              </span>
              <span className="using-sep">/</span>
              <span className="using-badge is-model" title={prefs.model_id}>
                {prefs.model_id}
              </span>
            </span>
            <button
              type="button"
              className="change-btn"
              title="Change in Settings"
              aria-label="Change provider or model in Settings"
              onClick={() => void openSettings()}
            >
              <svg
                viewBox="0 0 16 16"
                width="12"
                height="12"
                aria-hidden="true"
              >
                <path
                  d="M11.5 2.5l2 2L5 13l-2.6.6L3 11l8.5-8.5z"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.4"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </svg>
              Change
            </button>
            <button
              type="button"
              className="change-btn"
              title="Open Help"
              aria-label="Open Help"
              onClick={() => void openHelp()}
            >
              Help
            </button>
          </p>
          <div className="seg" role="radiogroup" aria-label="Function">
            {FUNCTIONS.map((f) => (
              <button
                key={f}
                type="button"
                className={fn === f ? "seg-btn is-active" : "seg-btn"}
                aria-pressed={fn === f}
                onClick={() => setFn(f)}
              >
                {functionLabel(f)}
              </button>
            ))}
          </div>
          <textarea
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if ((e.ctrlKey || e.metaKey) && e.key === "Enter") void send();
            }}
            rows={4}
            placeholder="Ask something…"
          />
          <p className="hint">
            <kbd className="kbd">{isMac ? "Cmd" : "Ctrl"}+Enter</kbd> to send.{" "}
            <kbd className="kbd">{isMac ? "Cmd" : "Ctrl"}+/</kbd> fills from
            clipboard + auto-sends.
          </p>
          <div className="row">
            <button
              className="btn-primary"
              onClick={() => void send()}
              disabled={busy || !query.trim()}
            >
              Send
            </button>
            <button onClick={stop} disabled={!busy}>
              Stop
            </button>
            <span
              className={"status-pill is-" + (busy ? "streaming" : status)}
              role="status"
            >
              status: {busy ? "streaming…" : status}
            </span>
          </div>
          {!output && !busy && (
            <p className="empty-state">
              Nothing yet — type a question above and press Send.
            </p>
          )}
          <div className="answer-card">
            <div className="answer-head">
              <span>{output.length} chars</span>
              <button onClick={copy} disabled={!output}>
                {copied ? "Copied!" : "Copy"}
              </button>
            </div>
            <div
              className="answer-body"
              dangerouslySetInnerHTML={{ __html: renderMarkdown(output) }}
            />
          </div>
        </section>
      )}

      {isSettingsWindow && (
        <section
          className={
            modelsLoading ? "settings-panel is-loading" : "settings-panel"
          }
          aria-busy={modelsLoading || undefined}
        >
          <div className="section">
            <h2 className="section-title">Default Provider & Model</h2>
            <div className="card">
              <div className="form-row">
                <label htmlFor="s-provider">Provider</label>
                <select
                  id="s-provider"
                  value={provider}
                  disabled={modelsLoading}
                  onChange={handleProviderChange}
                  onInput={handleProviderChange}
                >
                  {PROVIDERS.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.label}
                    </option>
                  ))}
                </select>
              </div>
              {provider === "custom" && (
                <div className="form-row">
                  <label htmlFor="s-base-url">Custom base URL</label>
                  <input
                    id="s-base-url"
                    value={prefs.custom_base_url}
                    disabled={modelsLoading}
                    onChange={(e) =>
                      setPrefs({ ...prefs, custom_base_url: e.target.value })
                    }
                    onBlur={(e) =>
                      void fetchCustomModels(
                        e.target.value,
                        keyInputs["custom"] ?? "",
                      )
                    }
                    placeholder="http://10.0.1.10:8000/v1"
                  />
                  <p className="hint">
                    OpenAI-compatible endpoint; persisted via Save below.
                  </p>
                </div>
              )}
              {provider === "custom" && (
                <div className="form-row">
                  <label htmlFor="s-custom-key">Custom API key</label>
                  <input
                    id="s-custom-key"
                    type="password"
                    autoComplete="off"
                    disabled={modelsLoading}
                    placeholder="optional — leave empty if none"
                    value={keyInputs["custom"] ?? ""}
                    onChange={(e) =>
                      setKeyInputs((m) => ({ ...m, custom: e.target.value }))
                    }
                    onBlur={(e) =>
                      void fetchCustomModels(
                        prefs.custom_base_url,
                        e.target.value,
                      )
                    }
                  />
                  <span
                    className={hasKeys["custom"] ? "pill is-saved" : "pill"}
                  >
                    {hasKeys["custom"] ? "saved ✓" : "no key"}
                  </span>
                  <p className="hint">
                    Persisted via Save below; Delete below to remove.
                  </p>
                </div>
              )}
              <div className="form-row">
                <label htmlFor="s-model">Model</label>
                {provider === "custom" ? (
                  customModels.length > 0 ? (
                    <select
                      id="s-model"
                      value={
                        customModels.includes(model)
                          ? model
                          : (customModels[0] ?? model)
                      }
                      disabled={modelsLoading}
                      onChange={(e) => setModel(e.target.value)}
                    >
                      {Array.from(
                        new Set(
                          model.trim() && !customModels.includes(model)
                            ? [...customModels, model]
                            : customModels,
                        ),
                      ).map((m) => (
                        <option key={m} value={m}>
                          {m}
                        </option>
                      ))}
                    </select>
                  ) : (
                    <input
                      id="s-model"
                      value={model}
                      disabled={modelsLoading}
                      onChange={(e) => setModel(e.target.value)}
                      placeholder="e.g. llama3.1, qwen2.5, gpt-4o-mini"
                    />
                  )
                ) : (
                  <select
                    id="s-model"
                    value={model}
                    disabled={modelsLoading}
                    onChange={(e) => setModel(e.target.value)}
                  >
                    {models.map((m) => (
                      <option key={m} value={m}>
                        {m}
                      </option>
                    ))}
                  </select>
                )}
              </div>
              {provider === "custom" && modelsLoading && (
                <p className="loading-indicator" role="status">
                  <span className="spinner" aria-hidden="true" />
                  Loading models…
                </p>
              )}
              {provider === "custom" && modelsError && (
                <p className="models-error" role="alert">
                  {modelsError}
                </p>
              )}
              <div className="row">
                <button
                  className="btn-primary"
                  onClick={() => void handleSaveProvider()}
                  disabled={modelsLoading}
                >
                  Save
                </button>
                {provider === "custom" && (
                  <button
                    className="btn-danger"
                    onClick={() => void handleDeleteCustomKey()}
                    disabled={modelsLoading}
                  >
                    Delete
                  </button>
                )}
                {providerMsg && <span className="status">{providerMsg}</span>}
              </div>
              <p className="hint">
                Save persists provider, model, and key together
                {provider === "custom" ? " (base URL too)." : "."} Empty key
                input leaves the stored key alone.
              </p>
            </div>
          </div>
          <div className="section">
            <h2 className="section-title">General</h2>
            <div className="card">
              <div className="form-row">
                <span>Default function</span>
                <div className="radio-row">
                  {FUNCTIONS.map((f) => (
                    <label key={f} className="radio">
                      <input
                        type="radio"
                        name="default-function"
                        value={f}
                        checked={fn === f}
                        disabled={modelsLoading}
                        onChange={() => setFn(f)}
                      />
                      {functionLabel(f)}
                    </label>
                  ))}
                </div>
              </div>
              <div className="form-row">
                <label className="radio">
                  <input
                    type="checkbox"
                    checked={prefs.auto_submit}
                    disabled={modelsLoading}
                    onChange={(e) =>
                      setPrefs({ ...prefs, auto_submit: e.target.checked })
                    }
                  />
                  Auto-submit on shortcut
                </label>
              </div>
              <div className="form-row">
                <label className="radio">
                  <input
                    type="checkbox"
                    checked={autostart}
                    disabled={modelsLoading}
                    onChange={(e) => {
                      const enable = e.target.checked;
                      setAutostart(enable);
                      invoke("set_autostart", { enable }).catch(() =>
                        setAutostart(!enable),
                      );
                    }}
                  />
                  Launch at Login
                </label>
              </div>
              <div className="row">
                <button
                  className="btn-primary"
                  onClick={saveSettings}
                  disabled={modelsLoading}
                >
                  Save
                </button>
                <button
                  className="btn-warn"
                  onClick={resetSettings}
                  disabled={modelsLoading}
                >
                  Reset to defaults
                </button>
                {saved && <span className="status">saved ✓</span>}
              </div>
            </div>
          </div>
          <div className="section">
            <h2 className="section-title">Keys</h2>
            <p className="hint">
              Keys are stored in the OS keychain, never in localStorage. Custom
              key is optional.
            </p>
            {PROVIDERS.filter((p) => p.id !== "custom").map((p) => (
              <div className="compact-row" key={p.id}>
                <span className="compact-name">{p.label}</span>
                <span className={hasKeys[p.id] ? "pill is-saved" : "pill"}>
                  {hasKeys[p.id] ? "saved ✓" : "no key"}
                </span>
                <input
                  id={`key-${p.id}`}
                  type="password"
                  autoComplete="off"
                  disabled={modelsLoading}
                  placeholder={`sk-… for ${p.id}`}
                  value={keyInputs[p.id] ?? ""}
                  onChange={(e) =>
                    setKeyInputs((m) => ({ ...m, [p.id]: e.target.value }))
                  }
                />
                <button
                  className="btn-primary"
                  onClick={() => saveKey(p.id)}
                  disabled={modelsLoading}
                >
                  Save
                </button>
                <button
                  className="btn-danger"
                  onClick={() => deleteKey(p.id)}
                  disabled={modelsLoading}
                >
                  Delete
                </button>
              </div>
            ))}
            {keyMsg && <p className="status">{keyMsg}</p>}
          </div>
        </section>
      )}

      {isHelpWindow && (
        <>
          <header className="header">
            <h1>TrayAsk Help</h1>
          </header>
          <section className="settings-panel">
            <div className="section help-section">
              <h2 className="section-title">What TrayAsk does</h2>
              <div className="card">
                <p>
                  <strong>
                    Look up, search, summarize, or translate with AI — instantly,
                    from anywhere.
                  </strong>
                </p>
                <p>
                  No need to open an app or browser to ask: copy some text (or type
                  a question), press the shortcut, and the answer streams into the
                  panel.
                </p>
                <img className="help-img" loading="lazy" src={helpTrayIcon} alt="TrayAsk tray icon" />
              </div>
            </div>
            <div className="section help-section">
              <h2 className="section-title">Ask from anywhere</h2>
              <div className="card">
                <p>
                  Copy some text, then press{" "}
                  <kbd className="kbd">{isMac ? "Cmd" : "Ctrl"}+/</kbd> to
                  bring up the panel with your clipboard text filled in.
                </p>
                <ul className="help-list">
                  <li>
                    Clipboard text is only used if copied within 30 seconds;
                    older copies are ignored.
                  </li>
                  <li>
                    When auto-submit is on, the question is sent automatically;
                    otherwise press Send.
                  </li>
                </ul>
                <img className="help-img" loading="lazy" src={helpTrayMenu} alt="Tray menu with Open TrayAsk, Settings, Launch at Login, Help, and Quit" />
              </div>
            </div>
            <div className="section help-section">
              <h2 className="section-title">Panel basics</h2>
              <div className="card">
                <ul className="help-list">
                  <li>
                    Pick a function pill (fix grammar, explain, summarize,
                    translate).
                  </li>
                  <li>
                    Press <kbd className="kbd">{isMac ? "Cmd" : "Ctrl"}+Enter</kbd>{" "}
                    or click Send to ask.
                  </li>
                  <li>Click Stop to cancel a running request.</li>
                  <li>The status pill shows idle, streaming, done, error, or cancelled.</li>
                  <li>Click Copy to copy the answer to your clipboard.</li>
                </ul>
                <img className="help-img" loading="lazy" src={helpPanel} alt="Chat panel with function pills, question box, Send and Stop buttons, and answer card" />
              </div>
            </div>
            <div className="section help-section">
              <h2 className="section-title">Functions</h2>
              <div className="card">
                <p>
                  Each function sends your text with a fixed prompt: explain
                  breaks it down, fix grammar cleans up grammar, spelling,
                  and punctuation without changing your voice, summarize
                  condenses it, and translate renders it in another language.
                </p>
              </div>
            </div>
            <div className="section help-section">
              <h2 className="section-title">Providers &amp; models</h2>
              <div className="card">
                <ul className="help-list">
                  <li>OpenAI, Anthropic, and Gemini each need an API key saved in Settings.</li>
                  <li>
                    Custom needs an http(s) base URL for an OpenAI-compatible
                    endpoint, plus an optional key.
                  </li>
                  <li>Use the Model dropdown to pick a model.</li>
                  <li>While models load, a “Loading models” indicator is shown.</li>
                  <li>
                    If no models are listed, you can still type a model name by hand.
                  </li>
                </ul>
                <img className="help-img" loading="lazy" src={helpProvider} alt="Settings provider card with Custom base URL, API key, model, Save and Delete buttons" />
              </div>
            </div>
            <div className="section help-section">
              <h2 className="section-title">Keys</h2>
              <div className="card">
                <p>
                  {isMac
                    ? "Keys are stored in the macOS keychain (Keychain Access) under the service name trayask-<provider>. They are never written to files."
                    : isWindows
                      ? "Keys are stored in Windows Credential Manager under the service name trayask-<provider>. They are never written to files."
                      : "Keys are stored in the Linux Secret Service (GNOME Keyring / KWallet) under the service name trayask-<provider>. They are never written to files."}
                </p>
                <img className="help-img" loading="lazy" src={helpKeys} alt="Settings Keys section with per-provider key rows and status pills" />
              </div>
            </div>
            <div className="section help-section">
              <h2 className="section-title">Settings</h2>
              <div className="card">
                <ul className="help-list">
                  <li>Choose your default provider, model, and function.</li>
                  <li>Turn auto-submit on to send immediately on the shortcut.</li>
                  <li>Turn launch at login on to start TrayAsk with your system.</li>
                  <li>Use Reset to defaults to restore the original settings.</li>
                </ul>
                <img className="help-img" loading="lazy" src={helpGeneral} alt="Settings General section with default function, auto-submit, launch at login, Save and Reset buttons" />
              </div>
            </div>
            <div className="section help-section">
              <h2 className="section-title">Troubleshooting</h2>
              <div className="card">
                <ul className="help-list">
                  <li>
                    Empty answer hint: the clipboard was empty or older than 30
                    seconds — copy again or type your question.
                  </li>
                  <li>
                    Custom base URL empty: set it in Settings (e.g.
                    http://10.0.1.10:8000/v1).
                  </li>
                  <li>
                    Keychain read-back mismatch: fully restart the dev command or
                    rebuild, then check {isMac
                      ? "Keychain Access"
                      : isWindows
                        ? "Credential Manager"
                        : "Secret Service"} for the trayask entry.
                  </li>
                  <li>401 means the API key is invalid — re-save it in Settings.</li>
                  <li>429 means you are rate limited — wait and try again.</li>
                  <li>
                    {isMac
                      ? "Shortcut unavailable: make sure TrayAsk has Accessibility permission and no other app uses the same shortcut — otherwise click the tray icon, paste your text, and press Send."
                      : isWindows
                        ? "Shortcut unavailable over elevated (administrator) windows: run TrayAsk as administrator — otherwise click the tray icon, paste your text, and press Send."
                        : "Shortcut unavailable on Wayland: click the tray icon, paste your text, and press Send."}
                  </li>
                </ul>
              </div>
            </div>
            <div className="section help-section">
              <h2 className="section-title">Limits</h2>
              <div className="card">
                <ul className="help-list">
                  <li>Queries are truncated to 4000 characters.</li>
                  <li>
                    One request runs at a time; Stop cancels the running request.
                  </li>
                </ul>
              </div>
            </div>
          </section>
        </>
      )}
    </main>
  );
}

export default App;
