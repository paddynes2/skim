// Fork (6.5.5): the three smell settings, parsed from `get_settings`. Pure
// (no Tauri import) so the node tests can read it; api.ts does the I/O.

export interface SmellSettings {
  /** `fork_smell`: the scanner is on (default on). */
  enabled: boolean;
  /** `fork_smell_block_hard`: Send stops on hard findings (default on). */
  blockHard: boolean;
  /** `fork_smell_ignored`: rule ids switched off from the popover. */
  ignored: Set<string>;
}

export const SMELL_KEYS = {
  enabled: "fork_smell",
  blockHard: "fork_smell_block_hard",
  ignored: "fork_smell_ignored",
} as const;

export function parseSmellSettings(all: Record<string, string>): SmellSettings {
  let ignored: string[] = [];
  try {
    const raw = JSON.parse(all[SMELL_KEYS.ignored] || "[]");
    if (Array.isArray(raw)) ignored = raw.filter((x): x is string => typeof x === "string");
  } catch {
    ignored = [];
  }
  return {
    enabled: (all[SMELL_KEYS.enabled] ?? "on") !== "off",
    blockHard: (all[SMELL_KEYS.blockHard] ?? "on") !== "off",
    ignored: new Set(ignored),
  };
}
