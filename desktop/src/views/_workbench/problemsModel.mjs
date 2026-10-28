/**
 * The Problems list (ide/10), as facts: what the diagnostics a root's
 * language servers published add up to — the entries a store keeps per
 * root, how a frame changes them, the rows a list draws, the counts, the
 * rail's mark and the words. Plain JavaScript so `node --test` reads it;
 * `diagnosticsStore.ts` hears the frames and `FilesProblems.tsx` draws.
 *
 * An entry is one server's word on one document: `{language, uri,
 * diagnostics}`, where `uri` is the root-relative path the node rewrites
 * to, and a `publishDiagnostics` for the same pair replaces it whole — that
 * is LSP's own rule: the list for a document is the last one published.
 */

import { t } from "../../i18n/l10n.mjs";

/** How many problems one file shows before it folds the rest into "+N more". */
const PROBLEMS_PER_FILE = 50;

/** LSP's severities, by number: Error 1, Warning 2, Information 3, Hint 4. Absent reads as an error, as the gutter's markers do. */
const SEVERITIES = Object.freeze(["error", "warning", "info", "hint"]);

/** @param {unknown} lsp @returns {"error" | "warning" | "info" | "hint"} */
export function severityOf(lsp) {
  return SEVERITIES[Number(lsp) - 1] ?? "error";
}

/** The severity's word. @param {"error" | "warning" | "info" | "hint"} severity */
export function severityWord(severity) {
  switch (severity) {
    case "warning":
      return t("workbench-problems-severity-warning");
    case "info":
      return t("workbench-problems-severity-info");
    case "hint":
      return t("workbench-problems-severity-hint");
    default:
      return t("workbench-problems-severity-error");
  }
}

/**
 * The entries after a `publishDiagnostics` frame: the pair's list replaced
 * — or added, or removed when the list is empty, since a document with
 * nothing wrong earns no row. The same array when nothing changes, so a
 * frame heard twice (two hooks on one root) moves nobody.
 * @template {{language: string, uri: string, diagnostics: readonly unknown[]}} E
 * @param {readonly E[]} entries
 * @param {{language: string, uri: string, diagnostics: readonly unknown[]}} frame
 * @returns {readonly E[]}
 */
export function withDiagnostics(entries, frame) {
  const at = entries.findIndex((e) => e.language === frame.language && e.uri === frame.uri);
  if (frame.diagnostics.length === 0) return at === -1 ? entries : [...entries.slice(0, at), ...entries.slice(at + 1)];
  if (at !== -1 && entries[at].diagnostics === frame.diagnostics) return entries;
  const entry = /** @type {E} */ ({ language: frame.language, uri: frame.uri, diagnostics: frame.diagnostics });
  if (at === -1) return [...entries, entry];
  const next = entries.slice();
  next[at] = entry;
  return next;
}

/**
 * The entries without one server's: a server that stopped or failed took
 * what it knew with it, and its word must not stand for nobody.
 * @template {{language: string}} E
 * @param {readonly E[]} entries
 * @param {string} language
 * @returns {readonly E[]}
 */
export function withoutLanguage(entries, language) {
  const kept = entries.filter((e) => e.language !== language);
  return kept.length === entries.length ? entries : kept;
}

/** The problems of one entry, each with its words read once. */
function problemsOf(entry) {
  return (entry.diagnostics ?? []).map((d) => ({
    severity: severityOf(d.severity),
    message: String(d.message ?? ""),
    line: (d.range?.start?.line ?? 0) + 1,
    column: (d.range?.start?.character ?? 0) + 1,
    source: typeof d.source === "string" ? d.source : entry.language,
    code: d.code == null ? null : String(typeof d.code === "object" ? d.code.value : d.code),
  }));
}

/**
 * The counts over every entry: by severity, and the files with at least one.
 * @param {readonly {uri: string, diagnostics: readonly {severity?: unknown}[]}[]} entries
 * @returns {{errors: number, warnings: number, infos: number, hints: number, files: number}}
 */
export function problemCounts(entries) {
  const counts = { errors: 0, warnings: 0, infos: 0, hints: 0, files: 0 };
  const files = new Set();
  for (const e of entries) {
    if (e.diagnostics.length === 0) continue;
    files.add(e.uri);
    for (const d of e.diagnostics) {
      const s = severityOf(d.severity);
      if (s === "error") counts.errors += 1;
      else if (s === "warning") counts.warnings += 1;
      else if (s === "info") counts.infos += 1;
      else counts.hints += 1;
    }
  }
  counts.files = files.size;
  return counts;
}

/**
 * The flat rows the list draws: a file row per path — paths in order, every
 * server's word on a path under one row — then its problems, errors first
 * and then by line, the first `perFile` shown and the rest counted. Flat
 * because the list is virtualised, as the search's rows are.
 * @param {readonly {language: string, uri: string, diagnostics: readonly unknown[]}[]} entries
 * @param {number} [perFile]
 * @returns {({kind: "file", key: string, path: string, errors: number, warnings: number, count: number} | {kind: "problem", key: string, path: string, line: number, column: number, severity: "error" | "warning" | "info" | "hint", message: string, source: string, code: string | null} | {kind: "more", key: string, path: string, more: number})[]}
 */
export function problemRows(entries, perFile = PROBLEMS_PER_FILE) {
  const byPath = new Map();
  for (const e of entries) {
    if (e.diagnostics.length === 0) continue;
    const list = byPath.get(e.uri) ?? [];
    list.push(...problemsOf(e));
    byPath.set(e.uri, list);
  }
  const paths = [...byPath.keys()].sort((a, b) => a.localeCompare(b));
  const rows = [];
  for (const path of paths) {
    const problems = byPath.get(path).sort((a, b) => SEVERITIES.indexOf(a.severity) - SEVERITIES.indexOf(b.severity) || a.line - b.line || a.column - b.column);
    const errors = problems.filter((p) => p.severity === "error").length;
    const warnings = problems.filter((p) => p.severity === "warning").length;
    rows.push({ kind: "file", key: `f:${path}`, path, errors, warnings, count: problems.length });
    problems.slice(0, perFile).forEach((p, i) => rows.push({ kind: "problem", key: `p:${path}:${i}`, path, ...p }));
    if (problems.length > perFile) rows.push({ kind: "more", key: `m:${path}`, path, more: problems.length - perFile });
  }
  return rows;
}

/**
 * The Files tab's mark on the rail: a danger dot while a server reports an
 * error, with the counts as its sentence; warnings alone earn no mark — a
 * mark is for what stops the work, not for what a linter would like.
 * @param {ReturnType<typeof problemCounts>} counts
 * @returns {{tone: "danger", title: string} | null}
 */
export function problemsBadge(counts) {
  if (counts.errors <= 0) return null;
  return { tone: "danger", title: problemsWords(counts) };
}

/**
 * The counts as a sentence — *3 errors · 1 warning in 2 files* — or that
 * nothing was reported.
 * @param {ReturnType<typeof problemCounts>} counts
 */
export function problemsWords(counts) {
  if (counts.errors + counts.warnings + counts.infos + counts.hints === 0) return t("workbench-problems-none");
  return t("workbench-problems-counts", { errors: counts.errors, warnings: counts.warnings, files: counts.files });
}
