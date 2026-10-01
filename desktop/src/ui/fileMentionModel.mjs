/**
 * `@` for files (ide/09): the composer's picker offers the root's paths
 * beside its people, and a picked file becomes a **context chip** — never a
 * mention. A mention is a principal the store resolves; a path is not one,
 * and the wire would refuse it. So the text carries `@src/main.rs` as words
 * the reader can see, the chip carries the file the agent gets, and this
 * module keeps the two in step: delete the token and the chip goes.
 *
 * Plain `.mjs`, following `mentionModel.mjs`: the rules are what a test
 * pins; the component is paint.
 */

import { rankPaths } from "../shell/quickOpenScore.mjs";
import { escapeRe } from "./mentionModel.mjs";

/** How many file rows the picker shows under the people. */
const FILE_SUGGESTION_LIMIT = 8;

/**
 * Whether the run after `@` is asking for a file: it has a `/` or a `.` in
 * it — nobody's name does — or nothing else matched it, so a path is the
 * only thing it could still be. An empty run offers people, not every file.
 * @param {string} query
 * @param {number} principalMatches how many people or agents the run matched
 */
export function wantsFiles(query, principalMatches) {
  const q = String(query ?? "").trim();
  if (!q) return false;
  return /[/.]/.test(q) || principalMatches === 0;
}

/**
 * The paths the run suggests, best first — the palette's scorer, so the same
 * typing finds the same file here and in quick open.
 * @param {string} query
 * @param {readonly string[]} paths
 * @param {number} [limit]
 */
export function fileSuggestions(query, paths, limit = FILE_SUGGESTION_LIMIT) {
  const q = String(query ?? "").trim();
  if (!q || !paths || paths.length === 0) return [];
  return rankPaths(q, paths, limit).map((r) => r.path);
}

/**
 * The body with a picked path spliced over the open `@`-run, and where the
 * caret lands after it — the mention's splice, with a path for a name.
 * @param {string} text
 * @param {{at: number, end: number}} span
 * @param {string} path
 */
export function insertFileMention(text, span, path) {
  const body = String(text ?? "");
  const token = `@${path} `;
  return { text: body.slice(0, span.at) + token + body.slice(span.end), caret: span.at + token.length };
}

/**
 * Whether the body still carries `@path` as a token of its own — at the start
 * or after whitespace, and ending at whitespace or the end. `@src/a.ts` does
 * not count as `@src/a.tsx`.
 * @param {string} text
 * @param {string} path
 */
export function fileTokenPresent(text, path) {
  return new RegExp(`(^|\\s)@${escapeRe(path)}(?=\\s|$)`).test(String(text ?? ""));
}

/**
 * Which of the files picked so far are still named in the body, and which
 * were edited out — the ones whose chips should go.
 * @param {string} text
 * @param {readonly string[]} picked
 * @returns {{kept: string[], dropped: string[]}}
 */
export function syncFileMentions(text, picked) {
  const kept = [];
  const dropped = [];
  for (const p of picked ?? []) (fileTokenPresent(text, p) ? kept : dropped).push(p);
  return { kept, dropped };
}
