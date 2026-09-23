// Fork (6.5.2): parity between the TS scanner and OS's slop_scrub.py.
//
//   node scripts/fork/smell-parity.mjs [--os C:\Users\Patrick\OS]
//
// Runs `src/fork/smell/scan.ts` and `python slop_scrub.py --json --all
// --plaintext` over the same corpus (src/fork/tests/smell-corpus.json) and
// exits 1 on the first sample where the (category, matched text) sets differ.
// Also run by `node --test src/fork/tests/smell.test.mjs` when the OS checkout
// is present (skipped, loudly, when it is not: a CI box has no OS).
//
// Categories only one side has are excluded by name, never by heuristic:
//   TS only:  dash (Patrick's rule, owned in OS by broker_draft_commit),
//             contrast_repeat (no-smell's document layer).
// Categories the Python side collapses to one row per document are compared
// by presence: invisible_char ("zero-width space (U+200B) x1") and
// quote_dialect ("curly + straight double quotes").
import { spawnSync } from "node:child_process";
import { readFileSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { register } from "node:module";
register("../../src/fork/tests/ts-resolve.mjs", import.meta.url);
const { compile, scan } = await import("../../src/fork/smell/scan.ts");

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const TS_ONLY = new Set(["dash", "contrast_repeat"]);
const PRESENCE_ONLY = new Set(["invisible_char", "quote_dialect"]);

export function osRoot(argv = process.argv.slice(2)) {
  const i = argv.indexOf("--os");
  return i >= 0 ? argv[i + 1] : process.env.SKIM_OS_ROOT || "C:/Users/Patrick/OS";
}

export function pythonFindings(os, text) {
  const r = spawnSync(
    "python",
    [resolve(os, "tools", "outreach", "slop_scrub.py"), "--json", "--no-log", "--all", "--plaintext"],
    {
      input: text,
      encoding: "utf8",
      env: {
        ...process.env,
        PYTHONUTF8: "1",
        PYTHONIOENCODING: "utf-8",
        // the on-disk auto-accept allowlist must not shape a parity verdict
        SLOP_SCRUB_EXEMPTIONS_PATH: resolve(os, "does-not-exist.json"),
      },
    },
  );
  if (r.error) throw r.error;
  if (![0, 1, 2].includes(r.status)) throw new Error(`slop_scrub exited ${r.status}: ${r.stderr}`);
  return JSON.parse(r.stdout);
}

function key(category, matched) {
  return PRESENCE_ONLY.has(category) ? category : `${category}\u0000${matched.toLowerCase()}`;
}

/** Compare one sample; returns the list of differences (empty = parity). */
export function diffSample(compiled, os, text) {
  const py = new Set(pythonFindings(os, text).map((f) => key(f.category, f.matched)));
  const ts = new Set(
    scan(text, compiled, { includeInfo: true }).spans
      .filter((s) => !TS_ONLY.has(s.category))
      .map((s) => key(s.category, s.text)),
  );
  const out = [];
  for (const k of py) if (!ts.has(k)) out.push(`python only: ${k.replace("\u0000", " -> ")}`);
  for (const k of ts) if (!py.has(k)) out.push(`ts only: ${k.replace("\u0000", " -> ")}`);
  return out;
}

export function runParity(os = osRoot()) {
  const rules = JSON.parse(readFileSync(resolve(ROOT, "src", "fork", "smell", "rules.json"), "utf8"));
  const corpus = JSON.parse(readFileSync(resolve(ROOT, "src", "fork", "tests", "smell-corpus.json"), "utf8"));
  const compiled = compile(rules);
  const failures = [];
  for (const s of corpus) {
    const d = diffSample(compiled, os, s.text);
    if (d.length) failures.push(`${s.name}:\n    ${d.join("\n    ")}`);
  }
  return { samples: corpus.length, failures };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const os = osRoot();
  if (!existsSync(resolve(os, "tools", "outreach", "slop_scrub.py"))) {
    console.error(`no OS checkout at ${os} (pass --os or set SKIM_OS_ROOT)`);
    process.exit(2);
  }
  const { samples, failures } = runParity(os);
  if (failures.length) {
    console.error(`smell parity: ${failures.length} of ${samples} samples differ\n  ${failures.join("\n  ")}`);
    process.exit(1);
  }
  console.log(`smell parity: ${samples} samples, TS and slop_scrub.py agree`);
}
