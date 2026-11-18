import { invoke } from "@tauri-apps/api/core";

import { FUNCTIONS, type FunctionKind } from "./models";

export type Prefs = {
  provider_id: string;
  model_id: string;
  default_function: FunctionKind;
  auto_submit: boolean;
  custom_base_url: string;
};

export const DEFAULT_PREFS: Prefs = {
  provider_id: "openai",
  model_id: "gpt-4o-mini",
  default_function: "fix_grammar",
  auto_submit: true,
  custom_base_url: "",
};

const KEY = "trayask.prefs.v1";

/**
 * Whether a window should reload saved prefs when it gains focus.
 *
 * The panel does: it may have been hidden while Settings saved. The settings
 * window must not: Linux WebKitGTK fires `focus` when its native `<select>`
 * popup closes, and reloading here snaps the provider dropdown back to the
 * stored value (OpenAI) before Save.
 */
export function shouldReloadPrefsOnWindowFocus(label: string): boolean {
  return label !== "settings";
}

function mergeDefaults(parsed: Partial<Prefs>): Prefs {
  const merged = { ...DEFAULT_PREFS, ...parsed };
  if (!FUNCTIONS.includes(merged.default_function)) {
    return { ...merged, default_function: DEFAULT_PREFS.default_function };
  }
  return merged;
}

export function loadPrefs(): Prefs {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return { ...DEFAULT_PREFS };
    const parsed = JSON.parse(raw) as Partial<Prefs>;
    return mergeDefaults(parsed);
  } catch {
    return { ...DEFAULT_PREFS };
  }
}

export function savePrefs(prefs: Prefs): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(prefs));
  } catch {
    // storage unavailable (private mode etc.) — ignore
  }
}

/** Load prefs from the backend file, falling back to localStorage cache. */
export async function loadPrefsBackend(): Promise<Prefs> {
  try {
    const p = await invoke<Prefs>("get_prefs");
    const merged = mergeDefaults(p);
    savePrefs(merged);
    return merged;
  } catch {
    return loadPrefs();
  }
}

/** Persist prefs to the backend file (which emits `prefs-changed`) + local cache. */
export async function savePrefsBackend(prefs: Prefs): Promise<Prefs> {
  savePrefs(prefs);
  try {
    const saved = await invoke<Prefs>("save_prefs", { prefs });
    const merged = mergeDefaults(saved);
    savePrefs(merged);
    return merged;
  } catch {
    return prefs;
  }
}
