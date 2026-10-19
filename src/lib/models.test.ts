import assert from "node:assert/strict";
import { test } from "node:test";
import {
  providerIdFromSelect,
  resolveProviderId,
  type ProviderSelectReading,
} from "./models.ts";
import { shouldReloadPrefsOnWindowFocus } from "./prefs.ts";

function select(
  value: string,
  selectedValue: string | null,
  selectedIndex: number,
): ProviderSelectReading {
  return {
    value,
    selectedIndex,
    options: {
      item(index: number) {
        if (index !== selectedIndex || selectedValue === null) return null;
        return {
          getAttribute(name: string) {
            return name === "value" ? selectedValue : null;
          },
        };
      },
    },
  };
}

test("resolveProviderId maps labels and ids", () => {
  assert.equal(resolveProviderId("anthropic"), "anthropic");
  assert.equal(resolveProviderId("Anthropic"), "anthropic");
  assert.equal(resolveProviderId(" Gemini "), "gemini");
  assert.equal(
    resolveProviderId("Custom (OpenAI-compatible)"),
    "custom",
  );
  assert.equal(resolveProviderId("openai"), "openai");
});

test("providerIdFromSelect prefers a WebKitGTK label over a lagging option", () => {
  assert.equal(
    providerIdFromSelect(select("Anthropic", "openai", 0)),
    "anthropic",
  );
  assert.equal(
    providerIdFromSelect(select("Custom (OpenAI-compatible)", "openai", 0)),
    "custom",
  );
});

test("providerIdFromSelect keeps the option value on macOS and Windows", () => {
  assert.equal(
    providerIdFromSelect(select("anthropic", "anthropic", 1)),
    "anthropic",
  );
  assert.equal(
    providerIdFromSelect(select("openai", "gemini", 2)),
    "gemini",
  );
});

test("settings does not reload prefs when its select menu closes", () => {
  assert.equal(shouldReloadPrefsOnWindowFocus("settings"), false);
  assert.equal(shouldReloadPrefsOnWindowFocus("panel"), true);
  assert.equal(shouldReloadPrefsOnWindowFocus("help"), true);
});
