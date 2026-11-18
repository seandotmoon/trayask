export const STATIC_MODELS: Record<string, string[]> = {
  openai: ["gpt-4o-mini", "gpt-4o", "gpt-4.1-mini"],
  anthropic: ["claude-3-5-sonnet-latest", "claude-3-5-haiku-latest"],
  gemini: ["gemini-1.5-flash", "gemini-1.5-pro"],
  custom: [],
};

export type ProviderInfo = { id: string; label: string; needsKey: boolean };

export const PROVIDERS: ProviderInfo[] = [
  { id: "openai", label: "OpenAI", needsKey: true },
  { id: "anthropic", label: "Anthropic", needsKey: true },
  { id: "gemini", label: "Gemini", needsKey: true },
  { id: "custom", label: "Custom (OpenAI-compatible)", needsKey: false },
];

/**
 * Map a dropdown reading to a provider id.
 *
 * WebKitGTK sometimes reports the visible label ("Anthropic") instead of the
 * option value ("anthropic"). An unknown string is returned trimmed so the
 * caller can still show it; known labels and ids collapse to the id.
 */
export function resolveProviderId(raw: string): string {
  const trimmed = raw.trim();
  const lowered = trimmed.toLowerCase();
  const known = PROVIDERS.find(
    (p) =>
      p.id === trimmed ||
      p.id === lowered ||
      p.label === trimmed ||
      p.label.toLowerCase() === lowered,
  );
  return known ? known.id : trimmed;
}

function isKnownProviderId(id: string): boolean {
  return PROVIDERS.some((p) => p.id === id);
}

/** The bits of a `<select>` this resolver reads. `HTMLSelectElement` satisfies it. */
export type ProviderSelectReading = {
  value: string;
  selectedIndex: number;
  options: {
    item(index: number): { getAttribute(name: string): string | null } | null;
  };
};

/**
 * Read the selected provider id from a `<select>`.
 *
 * macOS and Windows report the option `value` (`anthropic`). Linux WebKitGTK
 * often reports the visible label (`Anthropic`) on `HTMLSelectElement.value`,
 * and `selectedIndex` can still point at the previous option. A label that
 * maps to a known id wins; otherwise the option's `value` attribute does.
 * An unmatched string would make this controlled select fall back to OpenAI.
 */
export function providerIdFromSelect(select: ProviderSelectReading): string {
  const rawValue = select.value.trim();
  const fromValue = resolveProviderId(rawValue);
  const option =
    select.selectedIndex >= 0
      ? select.options.item(select.selectedIndex)
      : null;
  const fromAttribute = resolveProviderId(option?.getAttribute("value") ?? "");
  const valueWasLabel =
    isKnownProviderId(fromValue) && rawValue !== fromValue;
  if (valueWasLabel) return fromValue;
  if (isKnownProviderId(fromAttribute)) return fromAttribute;
  if (isKnownProviderId(fromValue)) return fromValue;
  return fromAttribute.length > 0 ? fromAttribute : fromValue;
}

export const FUNCTIONS = [
  "explain",
  "fix_grammar",
  "summarize",
  "translate",
] as const;
export type FunctionKind = (typeof FUNCTIONS)[number];

/** Display labels for function ids (ids are snake_case; labels are what the UI shows). */
export const FUNCTION_LABELS: Record<FunctionKind, string> = {
  explain: "explain",
  fix_grammar: "fix grammar",
  summarize: "summarize",
  translate: "translate",
};

export function functionLabel(f: FunctionKind): string {
  return FUNCTION_LABELS[f] ?? f;
}
