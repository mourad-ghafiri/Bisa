/**
 * The one log door (`log.ts`) is the only way a diagnostic leaves the webview,
 * and nothing is swallowed in silence: a promise failure names itself in the
 * log, and a `catch` that keeps nothing carries the one-line reason it may.
 * A source guard, as `noHappyDom.test.mjs` is — the rule lives here, not in a
 * review comment.
 *
 * Run with `node --test desktop/src/logDoor.test.mjs`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, relative } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { sourceFiles } from "./testWalk.mjs";

const SRC = dirname(fileURLToPath(import.meta.url));
// The models (`.mjs`) are walked with the components: a silence in a model is as silent.
const sources = () => sourceFiles(SRC, (p) => /\.(ts|tsx|mjs)$/.test(p) && !p.endsWith(".d.mts") && !p.endsWith(".test.mjs") && !p.endsWith("types.gen.ts"));

/** The silences that may stay, each with its reason — and the fact that keeps the reason true. */
const SILENT = Object.freeze({
  "shell/singleFlight.mjs": { why: "every joiner holds its own copy of the rejection; this one only keeps a flight nobody waits on from being an unhandled rejection", holds: "the joiners each hold their own copy" },
  "ui/artifact/pageInspector.mjs": { why: "its body is a script that runs inside a page's sandbox, where there is no log door: a page's own broken API must not break the inspector", holds: "const INSPECTOR_CORE = String.raw`" },
});
const silent = (file) => SILENT[relative(SRC, file)];
const stripComments = (text) => text.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/[^\n]*/g, "");

test("console is written by the log door alone — its own fallback when the shell refuses a line", () => {
  const offenders = [];
  for (const file of sources()) {
    if (file.endsWith("/log.ts")) continue;
    if (/\bconsole\.(log|warn|error|info|debug|trace)\(/.test(stripComments(readFileSync(file, "utf8")))) offenders.push(relative(SRC, file));
  }
  assert.deepEqual(offenders, []);
});

test("a promise failure is never swallowed: every catch handler says something, or hands on a value the caller reads", () => {
  const offenders = [];
  for (const file of sources()) {
    if (silent(file)) continue;
    const text = stripComments(readFileSync(file, "utf8"));
    for (const m of text.matchAll(/\.catch\(\(\)\s*=>\s*(\{\s*\}|undefined)\)/g)) offenders.push(`${relative(SRC, file)}: ${m[0]}`);
  }
  assert.deepEqual(offenders, [], "an empty handler is a silence; a `() => null` hands the caller a value it reads as absent");
});

test("a catch that keeps nothing carries its reason in a comment — never a bare `catch {}`", () => {
  const offenders = [];
  for (const file of sources()) {
    if (silent(file)) continue;
    const text = readFileSync(file, "utf8");
    for (const m of text.matchAll(/\bcatch\s*(\([^)]*\))?\s*\{([^{}]*)\}/g)) {
      const body = m[2];
      const hasStatement = stripComments(body).trim().length > 0;
      const hasReason = /\/\/|\/\*/.test(body);
      if (!hasStatement && !hasReason) offenders.push(`${relative(SRC, file)}: catch ${m[2].trim().slice(0, 40)}`);
    }
  }
  assert.deepEqual(offenders, [], "a silence with no reason is a bug waiting for its afternoon");
});

test("a silence that may stay still has its reason: the fact it rests on is in the file", () => {
  for (const [rel, { why, holds }] of Object.entries(SILENT)) {
    const text = readFileSync(`${SRC}/${rel}`, "utf8");
    assert.ok(text.includes(holds), `${rel} may be silent because ${why} — and that must still be so`);
    assert.ok(/catch\s*(\([^)]*\))?\s*\{\s*\}|\.catch\(\(\)\s*=>\s*\{\s*\}\)/.test(text), `${rel} no longer keeps a silence: take it off the list`);
  }
});
