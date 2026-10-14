// Heat (v1.1.15, D67): the row tint for a conversation's heat, the
// "Pastel, stronger" ramp the owner picked: periwinkle (cold) through clear
// to seafoam and mint (hot). Rust side: `fork::heat`.

type Stop = [at: number, rgba: [number, number, number, number]];

const DARK: Stop[] = [
  [0, [160, 176, 255, 0.24]],
  [0.4, [170, 214, 240, 0.08]],
  [0.7, [160, 232, 214, 0.18]],
  [1, [150, 244, 190, 0.32]],
];
// The same hues, deeper, so they read on a light background.
const LIGHT: Stop[] = [
  [0, [92, 112, 230, 0.13]],
  [0.4, [70, 150, 200, 0.04]],
  [0.7, [40, 170, 150, 0.11]],
  [1, [30, 175, 105, 0.19]],
];

/** A Jev score (0..1) as a position on the ramp. Scores sit mostly between
 *  0.05 and 0.75, so the ramp spans 0.1 to 0.75. */
export function rampPosition(heat: number): number {
  return Math.max(0, Math.min(1, (heat - 0.1) / 0.65));
}

export function rampColour(t: number, light = false): [number, number, number, number] {
  const stops = light ? LIGHT : DARK;
  for (let i = 1; i < stops.length; i++) {
    const [at, to] = stops[i];
    if (t <= at) {
      const [from, a] = stops[i - 1];
      const u = (t - from) / (at - from);
      return a.map((v, k) => v + (to[k] - v) * u) as [number, number, number, number];
    }
  }
  return stops[stops.length - 1][1];
}

const rgba = ([r, g, b, a]: number[]) => `rgba(${Math.round(r)},${Math.round(g)},${Math.round(b)},${a.toFixed(3)})`;

/** The row's background: the tint at the left edge, fading out to the right
 *  so the text keeps its contrast. `undefined` when the row has no heat. */
export function heatBackground(heat: number | undefined, light = false): string | undefined {
  if (heat === undefined) return undefined;
  const [r, g, b, a] = rampColour(rampPosition(heat), light);
  return `linear-gradient(90deg, ${rgba([r, g, b, a])} 0%, ${rgba([r, g, b, a * 0.55])} 55%, ${rgba([r, g, b, a * 0.15])} 100%)`;
}
