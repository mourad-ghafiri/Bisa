/**
 * The bar above the composer that says what the agents changed in this
 * conversation (ide/20 §The desktop surfaces): every file still pending,
 * once, whoever touched it and how much — the same `ChangesView` the turn
 * cards read, arranged for a summary rather than a timeline. No DOM, no
 * fetch. Git's own working tree is the Git panel's; nothing of it is here,
 * which is what tells the two apart.
 */

import { fileChipTone, fileChipWords, fileVerbs } from "./turnChangesModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export { fileChipTone, fileChipWords, fileVerbs };

/**
 * Every pending path once — a path two turns touched shows the later turn's
 * state and counts — sorted by path.
 * @param {{turns?: readonly {files?: readonly {path: string, state: string}[]}[]} | null | undefined} view
 */
export function changedFileRows(view) {
  const byPath = new Map();
  for (const turn of view?.turns ?? []) for (const file of turn.files ?? []) byPath.set(file.path, file);
  return [...byPath.values()].filter((f) => f.state === "pending").sort((a, b) => a.path.localeCompare(b.path));
}

/** The agents whose turns still hold a pending file, first seen first. @param {object} view */
export function changedByAgents(view) {
  const seen = [];
  for (const turn of view?.turns ?? []) {
    if ((turn.files ?? []).some((f) => f.state === "pending") && !seen.includes(turn.agent)) seen.push(turn.agent);
  }
  return seen;
}

/**
 * *3 files changed by Reviewer · +40 −12* — or *by 2 agents*; `names` maps an
 * agent id to the name a person knows. Empty when nothing is pending.
 * @param {object} view
 * @param {(agent: string) => string} [names]
 */
export function changedFilesHeaderWords(view, names = (a) => a) {
  const rows = changedFileRows(view);
  if (rows.length === 0) return "";
  const agents = changedByAgents(view);
  const who = agents.length === 1 ? t("studio-changed-files-by", { who: names(agents[0]) }) : t("studio-changed-files-agents", { agents: agents.length });
  const added = rows.reduce((n, f) => n + (f.added ?? 0), 0);
  const removed = rows.reduce((n, f) => n + (f.removed ?? 0), 0);
  return t("studio-changed-files-file-files-changed", { rows: rows.length, who, added, removed });
}

/** The path for a row: the folders dimmed, the file's own name plain. @param {string} path */
export function splitPathForRow(path) {
  const at = path.lastIndexOf("/");
  return at === -1 ? { dir: "", base: path } : { dir: path.slice(0, at + 1), base: path.slice(at + 1) };
}

/** The glyph a change's kind wears: `+` created, `−` removed, `~` modified. @param {string} kind */
export function changeKindMark(kind) {
  return kind === "created" ? "+" : kind === "removed" ? "−" : "~";
}

/** The kind's tone for the kit: created is `ok`, the rest `warn`. @param {string} kind */
export function changeKindTone(kind) {
  return kind === "created" ? "ok" : "warn";
}

/** The bar's footer words — the bulk verbs, and the way to the first pending file. */
export function barWords() {
  return { keepAll: t("studio-turn-changes-card-keep-all"), undoAll: t("studio-turn-changes-card-undo-all"), review: t("studio-changed-files-review"), attach: t("studio-changed-files-attach") };
}

/**
 * The question *Undo all* asks first: how many files, whose changes, and —
 * when some were also edited by someone else since — that those stay as they
 * are (an undo without `force` leaves an overlapped file alone).
 * @param {object} view
 */
export function undoAllConfirmWords(view) {
  const rows = changedFileRows(view);
  const agents = changedByAgents(view).length;
  const overlapped = rows.filter((f) => f.overlapped).length;
  const body = t("studio-changed-files-undo-all-body");
  return {
    title: t("studio-changed-files-undo-all-title", { files: rows.length, agents }),
    body: overlapped > 0 ? `${body} ${t("studio-changed-files-undo-all-overlapped", { overlapped })}` : body,
    confirm: t("studio-turn-changes-card-undo-all"),
  };
}

/**
 * The toast once a bulk word landed — how many files it reached; `null` when
 * it reached none (everything was left alone, which the skipped line says).
 * @param {"keep" | "undo"} verdict
 * @param {number} files
 */
export function settledAllWords(verdict, files) {
  if (!(files > 0)) return null;
  return verdict === "keep" ? t("studio-changed-files-kept-all", { files }) : t("studio-changed-files-undid-all", { files });
}
