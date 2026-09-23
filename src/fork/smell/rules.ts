// Fork (6.5.1): the vendored rule set, compiled once for the app. rules.json is
// written by scripts/fork/sync-smell-rules.ps1 from the OS repo; `osCommit`
// says which OS commit it came from.
import rulesFile from "./rules.json";
import { compile, type Compiled, type RulesFile } from "./scan";

const file = rulesFile as RulesFile;

let cached: Compiled | null = null;

/** The compiled rules (lazy: a bad pattern throws on first use, not at import). */
export function rules(): Compiled {
  if (!cached) cached = compile(file);
  return cached;
}

export const osCommit: string | null = file.os_commit;
