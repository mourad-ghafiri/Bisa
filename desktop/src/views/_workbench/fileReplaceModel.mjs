/**
 * Replace across files from the Files tab (ide/12), as facts: the body one
 * question makes for `POST /ide/replace`, whether a kept preview still
 * answers the box, how a preview folds into rows, which files an apply
 * names, what the footer says before and after, and what `$` means in the
 * replacement. Nothing in this file touches the network; `FilesSearch.tsx`
 * asks the node and draws what is here.
 *
 * A preview is the confirmation: it shows every line that would change
 * under the file's hash, and the apply carries those hashes back, so a file
 * an agent edited in between is skipped and said so — never clobbered.
 */

import { t } from "../../i18n/l10n.mjs";

/** How many changed lines one file shows before it folds the rest into "+N more" — the search's own fold. */
const CHANGES_PER_FILE = 20;

/**
 * The body `POST /ide/replace` takes for the box's question and the
 * replacement. `plan` names the files to write, each with the hash its
 * preview carried; `null` previews.
 * @param {{needle: string, include: readonly string[], exclude: readonly string[]}} query
 * @param {{regex?: boolean, caseSensitive?: boolean, word?: boolean}} flags
 * @param {string} replacement
 * @param {readonly {path: string, base_hash: string}[] | null} [plan]
 */
export function replaceBody(query, flags, replacement, plan = null) {
  return {
    q: query.needle,
    regex: flags.regex === true,
    case: flags.caseSensitive === true ? "sensitive" : "smart",
    word: flags.word === true,
    include: [...query.include],
    exclude: [...query.exclude],
    replacement,
    apply: plan !== null,
    files: plan === null ? [] : plan.map((f) => ({ path: f.path, base_hash: f.base_hash })),
  };
}

/**
 * One string for one question — the needle, the globs, the three switches
 * and the replacement. A preview made for another string is stale.
 * @param {{needle: string, include: readonly string[], exclude: readonly string[]}} query
 * @param {{regex?: boolean, caseSensitive?: boolean, word?: boolean}} flags
 * @param {string} replacement
 */
export function questionKey(query, flags, replacement) {
  return JSON.stringify([query.needle, [...query.include], [...query.exclude], flags.regex === true, flags.caseSensitive === true, flags.word === true, replacement]);
}

/**
 * Whether a kept preview no longer answers the box: it was made for another
 * question. No preview is not stale — there is nothing to be stale.
 * @param {{key: string} | null | undefined} preview
 * @param {string} key the box's question now (`questionKey`)
 */
export function previewStale(preview, key) {
  return preview !== null && preview !== undefined && preview.key !== key;
}

/**
 * The flat rows a preview draws, in the node's order: a file row, its
 * changed lines (the first `perFile`), then a "+N more" row when it folded
 * some. Flat because the list is virtualised, as the search's rows are.
 * @param {readonly {path: string, base_hash: string, changes: readonly {line: number, before: string, after: string}[]}[]} files
 * @param {number} [perFile]
 * @returns {({kind: "file", key: string, path: string, count: number, baseHash: string} | {kind: "change", key: string, path: string, line: number, before: string, after: string} | {kind: "more", key: string, path: string, more: number})[]}
 */
export function previewRows(files, perFile = CHANGES_PER_FILE) {
  const rows = [];
  for (const f of files) {
    rows.push({ kind: "file", key: `f:${f.path}`, path: f.path, count: f.changes.length, baseHash: f.base_hash });
    for (const c of f.changes.slice(0, perFile)) rows.push({ kind: "change", key: `c:${f.path}:${c.line}`, path: f.path, line: c.line, before: c.before, after: c.after });
    if (f.changes.length > perFile) rows.push({ kind: "more", key: `m:${f.path}`, path: f.path, more: f.changes.length - perFile });
  }
  return rows;
}

/**
 * The files an apply names — every previewed file, or the one `only` —
 * each with the hash its preview carried. `null` when there is nothing to
 * write: the node refuses an apply that names no file.
 * @param {readonly {path: string, base_hash: string}[]} files
 * @param {string | null} [only]
 * @returns {{path: string, base_hash: string}[] | null}
 */
export function replacePlan(files, only = null) {
  const chosen = only === null ? files : files.filter((f) => f.path === only);
  return chosen.length === 0 ? null : chosen.map((f) => ({ path: f.path, base_hash: f.base_hash }));
}

/**
 * What a preview says under its rows: how many lines in how many files
 * would change — or that nothing would, when no writable file holds a match.
 * @param {readonly {changes: readonly unknown[]}[]} files
 */
export function previewWords(files) {
  if (files.length === 0) return t("workbench-files-replace-nothing-would-change");
  const lines = files.reduce((n, f) => n + f.changes.length, 0);
  return t("workbench-files-replace-would-change", { lines, files: files.length });
}

/**
 * The line an apply leaves in the footer: what was written, what was
 * skipped because it moved on since the preview, what failed — and the tone
 * the worst of those earns.
 * @param {readonly {outcome: "applied" | "skipped_changed" | "failed"}[]} outcomes
 * @returns {{tone: "ok" | "warn" | "danger", text: string, applied: number, skipped: number, failed: number}}
 */
export function replaceOutcomeWords(outcomes) {
  let applied = 0;
  let skipped = 0;
  let failed = 0;
  for (const o of outcomes) {
    if (o.outcome === "applied") applied += 1;
    else if (o.outcome === "skipped_changed") skipped += 1;
    else failed += 1;
  }
  const parts = [t("workbench-files-replace-changed-files", { applied })];
  if (skipped > 0) parts.push(t("workbench-files-replace-skipped-files", { skipped }));
  if (failed > 0) parts.push(t("workbench-files-replace-failed-files", { failed }));
  return { tone: failed > 0 ? "danger" : skipped > 0 ? "warn" : "ok", text: parts.join(" · "), applied, skipped, failed };
}

/**
 * The hint under the replacement field: what `$` means in this mode — the
 * engine's own grammar, literal or the regex crate's — and that an empty
 * replacement deletes the match.
 * @param {boolean} regex
 */
export function captureHint(regex) {
  return regex ? t("workbench-files-replace-hint-regex") : t("workbench-files-replace-hint-literal");
}
