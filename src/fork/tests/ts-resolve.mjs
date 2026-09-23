// Node module-resolution hook for the fork's TS tests: the app imports siblings
// as `./scan` (Vite / svelte-check resolve that; `allowImportingTsExtensions`
// is off, so `./scan.ts` would fail `npm run check`), while Node's ESM loader
// wants the extension. This appends `.ts` to an extensionless relative import
// whose `.ts` file exists. Registered by the test file itself (and by
// scripts/fork/smell-parity.mjs), so the documented commands stay plain
// `node --test ...` / `node scripts/...`.
//
//   import { register } from "node:module";
//   register("./ts-resolve.mjs", import.meta.url);
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";

export async function resolve(specifier, context, next) {
  if ((specifier.startsWith("./") || specifier.startsWith("../")) && !/\.[a-z]+$/i.test(specifier) && context.parentURL) {
    const candidate = new URL(specifier + ".ts", context.parentURL);
    if (existsSync(fileURLToPath(candidate))) return next(candidate.href, context);
  }
  return next(specifier, context);
}
