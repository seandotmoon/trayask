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

export const FUNCTIONS = ["explain", "chat", "summarize", "translate"] as const;
export type FunctionKind = (typeof FUNCTIONS)[number];
