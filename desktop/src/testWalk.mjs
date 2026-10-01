/**
 * The one directory walk the guard tests share. A guard test reads sources —
 * every file under `src/`, or under one directory of it — and asserts a rule
 * over them; three copies of the same walk were three places for the rule's
 * reach to drift. Not a model: it has no `.d.mts` because nothing in the app
 * imports it, only the tests do.
 */

import { readdirSync, statSync } from "node:fs";
import { join } from "node:path";

/**
 * Every file under `dir`, recursively, whose path satisfies `keep` — by
 * default every file. Depth-first, in directory order, so a run is stable.
 * @param {string} dir
 * @param {(path: string) => boolean} [keep]
 * @returns {string[]}
 */
export function sourceFiles(dir, keep = () => true) {
  const out = [];
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) out.push(...sourceFiles(p, keep));
    else if (keep(p)) out.push(p);
  }
  return out;
}
