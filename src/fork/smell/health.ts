// Fork (6.5.3): the one-line health bar under the editor. Pure: turns a scan
// result into the parts to show ("3 AI tells · 'rather than' ×2 · no clear
// position"), only the parts that fired. SmellHealth.svelte renders them.
// `t` is the i18n function: a `{ n }` param picks the `_one` / `_other` key.
import type { ScanResult, Span } from "./scan";

export interface HealthPart {
  kind: "tells" | "hard" | "contrast" | "position";
  text: string;
  /** Where a click should put the caret: the first span of that kind. */
  jumpTo: Span | null;
}

/** The parts of the line, in display order. Empty when the draft is clean. */
export function healthParts(result: ScanResult, t: (key: string, vars?: Record<string, string | number>) => string): HealthPart[] {
  const parts: HealthPart[] = [];
  const hard = result.spans.filter((s) => s.severity === "hard");
  const tells = result.spans.filter((s) => s.severity !== "hard" && s.category !== "contrast_repeat");
  if (hard.length) {
    parts.push({
      kind: "hard",
      text: t("fork.smell.health_hard", { n: hard.length }),
      jumpTo: hard[0],
    });
  }
  if (tells.length) {
    parts.push({
      kind: "tells",
      text: t("fork.smell.health_tells", { n: tells.length }),
      jumpTo: tells[0],
    });
  }
  const rep = result.health.contrastRepeat;
  if (rep) {
    parts.push({
      kind: "contrast",
      text: `‘${rep.phrase}’ ×${rep.count}`,
      jumpTo: result.spans.find((s) => s.category === "contrast_repeat") ?? null,
    });
  }
  if (result.health.noPosition) {
    parts.push({ kind: "position", text: t("fork.smell.health_no_position"), jumpTo: null });
  }
  return parts;
}

/** The spans Send must stop on, phrased for the one-line prompt. */
export function sendPrompt(result: ScanResult, t: (key: string, vars?: Record<string, string | number>) => string): string | null {
  const n = result.spans.filter((s) => s.severity === "hard").length;
  if (!n) return null;
  return t("fork.smell.send_must_fix", { n });
}
