/**
 * Every member of the `api` object (`api.ts`) is called by a source: a client
 * function nobody calls is a route the desktop claims to speak and never does
 * — dead code the export guard cannot see, since the members of one exported
 * object are not exports. Fifteen were found this way (`heldMessages`,
 * `emitSignal`, `commitWorkstream` among them) and removed. A source guard,
 * as `deadExports.test.mjs` is.
 *
 * Run with `node --test desktop/src/apiMembers.test.mjs`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { sourceFiles } from "./testWalk.mjs";

const SRC = dirname(fileURLToPath(import.meta.url));
const API = join(SRC, "api.ts");
const isSource = (p) => /\.(ts|tsx|mjs)$/.test(p) && !p.endsWith(".d.mts") && !/\.test\.mjs$/.test(p) && !p.endsWith("types.gen.ts") && p !== API;

/** The member names of the one `export const api = { … }` object: every key at the object's own indent. */
export function apiMembers(text) {
  const start = text.indexOf("export const api = {");
  assert.ok(start >= 0, "api.ts exports `api`");
  const end = text.indexOf("\n};", start);
  assert.ok(end > start, "the object closes");
  const names = [];
  for (const m of text.slice(start, end).matchAll(/^ {2}([A-Za-z_$][\w$]*)(?:<[^>]*>)?\s*[:(]/gm)) names.push(m[1]);
  return names;
}

test("every member of `api` is called by a source", () => {
  const members = apiMembers(readFileSync(API, "utf8"));
  assert.ok(members.length > 300, `the client has its members: ${members.length}`);
  const sources = sourceFiles(SRC, isSource).map((p) => readFileSync(p, "utf8")).join("\n");
  // `api.name(` — or the chain broken over lines, `api\n  .name(`.
  const dead = members.filter((name) => !new RegExp(`\\bapi\\s*\\.\\s*${name.replace(/\$/g, "\\$")}\\b`).test(sources));
  assert.deepEqual(dead, [], "a client function nothing calls: remove it, with its route's types when they served it alone");
});

test("the member reader sees a plain member, a generic one and a multi-line one, and stops at the object's end", () => {
  const text = [
    "const before = 1;",
    "export const api = {",
    "  // --- a section ---",
    "  health: (s?: AbortSignal) => get<{ ok: boolean }>(\"/health\", s),",
    "  /// a doc line",
    "  tree: (scope: FileScope, id: string) => {",
    "    return get(`/x/${id}`);",
    "  },",
    "  paged<T>(path: string) {",
    "    return get<T>(path);",
    "  },",
    "};",
    "export const after = { notAMember: 1 };",
  ].join("\n");
  assert.deepEqual(apiMembers(text), ["health", "tree", "paged"]);
});
