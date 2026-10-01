/**
 * The Logging panel's facts: whose a family is, what a file is called and
 * when it was written, what a crash report says in a line, how big the
 * folder is, and the one sentence the panel owes — that nothing leaves
 * this machine. The names are `bisa-log`'s (`<process>.<period>.jsonl`
 * for a file, `<process>.<stamp>.<pid>.json` for a report); the process
 * words here mirror the crate's `Process::prefix()` and a test holds them
 * equal.
 */

import { relative } from "../../i18n/format.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The sentence the panel owes. */
export const LOCAL_ONLY =
  t("settings-logging-written-machine-never-sent-anywhere-report");

/** Whose a family is, by the crate's prefix. */
export const FAMILIES = Object.freeze({
  node: t("settings-node-panel-node-2"),
  cli: "a command",
  mcp: t("settings-logging-mcp-server-hook"),
  desktop: t("settings-logging-desktop"),
});

/** What each kind of report says, as a verb phrase after the process. */
export const CRASH_KINDS = Object.freeze({
  panic: "panicked",
  abrupt_end: t("settings-logging-ended-without-goodbye"),
  child_exit: t("settings-logging-saw-node-exit"),
});

/**
 * Whose file this is.
 * @param {string} name
 */
export function familyOf(name) {
  const prefix = name.split(".")[0] ?? "";
  return FAMILIES[/** @type {keyof typeof FAMILIES} */ (prefix)] ?? "unknown";
}

/**
 * The period a name carries — `2026-09-08` for a daily file,
 * `2026-09-08 14:00` for an hourly one — or the name when it carries none.
 * @param {string} name
 */
export function periodOf(name) {
  const m = name.match(/^[a-z]+\.(\d{4}-\d{2}-\d{2})(?:-(\d{2}))?\.jsonl$/);
  if (!m) return name;
  return m[2] ? `${m[1]} ${m[2]}:00` : m[1];
}

/**
 * The moment a report's name carries — `20260911T102233Z` as
 * `2026-09-11 10:22:33 UTC` — or the name when it carries none.
 * @param {string} name
 */
export function stampOf(name) {
  const m = name.match(/^[a-z]+\.(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})Z\.\d+\.json$/);
  if (!m) return name;
  return `${m[1]}-${m[2]}-${m[3]} ${m[4]}:${m[5]}:${m[6]} UTC`;
}

/**
 * A size in the fewest characters that say it.
 * @param {number} bytes
 */
export function bytesWords(bytes) {
  const n = Number.isFinite(bytes) && bytes > 0 ? bytes : 0;
  if (n < 1024) return `${n} B`;
  const kb = n / 1024;
  if (kb < 1024) return `${kb.toFixed(kb < 10 ? 1 : 0)} KB`;
  const mb = kb / 1024;
  if (mb < 1024) return `${mb.toFixed(mb < 10 ? 1 : 0)} MB`;
  return `${(mb / 1024).toFixed(1)} GB`;
}

/**
 * The rows one family's table draws: newest first, each with its period,
 * its size and how long ago it was written.
 * @param {readonly { name: string, bytes: number, modified_at: number }[]} files
 * @param {number} [now] unix seconds
 */
export function fileRows(files, now = Date.now() / 1000) {
  return [...files]
    .sort((a, b) => b.modified_at - a.modified_at || (a.name < b.name ? 1 : -1))
    .map((f) => ({
      name: f.name,
      family: familyOf(f.name),
      period: periodOf(f.name),
      size: bytesWords(f.bytes),
      age: relative(f.modified_at, now),
    }));
}

/**
 * One block per family, in the answer's order: whose it is, its folder and
 * its rows. A family with no file yet keeps its block, so the panel can say
 * so in place.
 * @param {readonly { process: string, dir: string, files: readonly { name: string, bytes: number, modified_at: number }[] }[]} families
 * @param {number} [now] unix seconds
 */
export function familyRows(families, now = Date.now() / 1000) {
  return families.map((f) => ({
    process: f.process,
    whose: FAMILIES[/** @type {keyof typeof FAMILIES} */ (f.process)] ?? "unknown",
    dir: f.dir,
    files: fileRows(f.files, now),
  }));
}

/**
 * The rows the crash list draws: newest first, each with whose it is, the
 * moment its name carries, its size and how long ago it was written.
 * @param {readonly { name: string, bytes: number, modified_at: number }[]} crashes
 * @param {number} [now] unix seconds
 */
export function crashRows(crashes, now = Date.now() / 1000) {
  return [...crashes]
    .sort((a, b) => b.modified_at - a.modified_at || (a.name < b.name ? 1 : -1))
    .map((f) => ({
      name: f.name,
      family: familyOf(f.name),
      at: stampOf(f.name),
      size: bytesWords(f.bytes),
      age: relative(f.modified_at, now),
    }));
}

/**
 * The newest report in one sentence — *The node panicked 3m ago: one bad
 * row.* — or nothing when there is none.
 * @param {{ process: string, kind: string, at: string, message: string } | null | undefined} latest
 * @param {number} [now] unix seconds
 */
export function latestCrashWords(latest, now = Date.now() / 1000) {
  if (!latest) return null;
  const whose = FAMILIES[/** @type {keyof typeof FAMILIES} */ (latest.process)] ?? latest.process;
  const did = CRASH_KINDS[/** @type {keyof typeof CRASH_KINDS} */ (latest.kind)] ?? latest.kind;
  const at = Date.parse(latest.at);
  const when = Number.isFinite(at) ? t("settings-logging-ago", { now: relative(at / 1000, now) }) : latest.at;
  const subject = whose.charAt(0).toUpperCase() + whose.slice(1);
  return `${subject} ${did} ${when}: ${latest.message}`;
}

/**
 * The folder in one line: how many files and reports and how much, or that
 * there is nothing yet.
 * @param {{ families: readonly { files: readonly { bytes: number }[] }[], crashes: readonly { bytes: number }[] }} listing
 */
export function totalWords(listing) {
  const files = listing.families.reduce((n, f) => n + f.files.length, 0);
  const crashes = listing.crashes.length;
  if (files === 0 && crashes === 0) return t("settings-logging-file-yet-nothing-has-gone-wrong");
  const bytes =
    listing.families.reduce((sum, f) => sum + f.files.reduce((s, x) => s + x.bytes, 0), 0) +
    listing.crashes.reduce((sum, c) => sum + c.bytes, 0);
  const parts = [];
  if (files > 0) parts.push(t("settings-logging-files", { files }));
  if (crashes > 0) parts.push(t("settings-logging-crash-report-reports", { crashes }));
  return t("settings-logging-summary", { parts: parts.length === 2 ? t("settings-logging-and", { a: parts[0], b: parts[1] }) : parts[0], bytes: bytesWords(bytes) });
}
