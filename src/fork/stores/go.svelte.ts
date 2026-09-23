// Reactive half of the `g` sequence (PLAN.md 3.3). Runes only work in
// `.svelte.ts`, so the flag lives here and `keys.ts` drives it.
let armed = $state(false);

export const goState = {
  get armed() {
    return armed;
  },
  set armed(v: boolean) {
    armed = v;
  },
};
