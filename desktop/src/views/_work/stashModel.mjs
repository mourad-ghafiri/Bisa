/**
 * The words around a stash in the Stashes view (ide/04 §Stash): what a row says
 * about an entry, when *Stash changes…* can be pressed and why not, what the dialog
 * and the confirmations say, what the toasts say after, and how a refusal
 * from the node reads — from its `code`, never guessed from the status. Pure,
 * so `node --test` reads them.
 *
 * The stash list is the repository's: every workstream of a project shares
 * `refs/stash`, so an entry made in another checkout is here too, and its
 * `branch` says where. Every verb is consented and recovery-first; pop and
 * drop pin the entry in Safety, so nothing here can lose work.
 */

import { shortRef } from "./gitDiscardModel.mjs";
import { noCommitsYet } from "./gitFiles.mjs";
import { t } from "../../i18n/l10n.mjs";

/**
 * One row of the Stashes view.
 * @param {{ index: number; message?: string | null; branch?: string | null; untracked: boolean; at: number }} entry
 */
export function stashRow(entry) {
  const message = entry.message?.trim();
  return {
    id: `stash@{${entry.index}}`,
    title: message ? message : t("work-stash-wip", { branch: entry.branch ?? t("work-stash-detached-head") }),
    where: entry.branch ? `on ${entry.branch}` : "detached",
    untracked: entry.untracked === true,
    at: entry.at,
  };
}

/**
 * Whether *Stash…* can be pressed for this tree, and the one reason when not.
 * @param {{ git: boolean; exists: boolean; head?: string | null; staged: number; unstaged: number; untracked: number; conflicted: number; in_progress?: string | null }} status
 * @param {boolean} [includeUntracked]
 */
export function canStash(status, includeUntracked = false) {
  const no = (reason) => ({ ok: false, reason });
  if (!status.git || !status.exists) return no(t("work-stash-not-repository"));
  if (noCommitsYet(status)) return no(t("work-stash-no-commit-yet-stash-against-make"));
  if (status.in_progress) return no(t("work-stash-progress-settle-abort-first", { op: String(status.in_progress).replace("_", "-") }));
  if (status.conflicted > 0) return no(t("work-stash-conflicts-settle-first-stash-cannot-hold"));
  if (status.staged + status.unstaged === 0) {
    if (status.untracked > 0 && includeUntracked) return { ok: true, reason: null };
    if (status.untracked > 0) return no(t("work-stash-only-untracked-files-here-tick-include"));
    return no(t("work-stash-nothing-stash"));
  }
  return { ok: true, reason: null };
}

/**
 * The dialog's words, for the whole tree or for named paths.
 * @param {readonly string[]} paths
 */
export function stashPushCopy(paths) {
  const scoped = paths.length > 0;
  return {
    title: scoped ? (paths.length === 1 ? t("work-stash-stash", { paths: paths[0] }) : t("work-stash-stash-files", { paths: paths.length })) : t("work-stash-stash-working-tree-s-changes"),
    description: scoped
      ? t("work-stash-only-these-paths-parked-everything-else")
      : t("work-stash-staged-unstaged-changes-parked-one-stash"),
    messageHint: t("work-stash-optional-without-one-git-names-entry"),
    untracked: {
      label: t("work-stash-include-untracked-files"),
      hint: t("work-stash-files-git-has-never-seen-go"),
    },
    keepIndex: {
      label: t("work-stash-keep-index"),
      hint: t("work-stash-what-staged-stays-staged-tree-well"),
    },
    confirm: t("work-git-words-stash"),
  };
}

/**
 * The confirmation before a pop or a drop. Apply asks nothing: it changes the
 * tree and keeps the entry, and what it changes was saved first.
 * @param {"pop" | "drop"} kind
 * @param {{ index: number; message?: string | null; branch?: string | null; untracked: boolean; at: number }} entry
 */
export function stashConfirm(kind, entry) {
  const row = stashRow(entry);
  if (kind === "drop") {
    return {
      title: t("work-stash-drop", { row: row.id }),
      body: ` ${t("work-stash-leaves-stash-list")}`,
      confirm: t("work-rebase-editor-drop"),
      danger: true,
    };
  }
  return {
    title: t("work-stash-pop", { row: row.id }),
    body: ` ${t("work-stash-applied-onto-working-tree-when-goes")}`,
    confirm: t("work-git-words-pop"),
    danger: false,
  };
}

/** @param {{ index: number; message?: string | null; branch?: string | null; untracked: boolean; at: number }} entry @param {string} ref */
export function stashedWords(entry, ref) {
  const row = stashRow(entry);
  return t("work-stash-stashed-what-here-saved", { row: row.id, title: row.title, ref: shortRef(ref) });
}

/** @param {{ index: number; message?: string | null; branch?: string | null; untracked: boolean; at: number }} entry @param {string} ref */
export function appliedWords(entry, ref) {
  const row = stashRow(entry);
  return t("work-stash-applied-entry-kept", { id: row.id, title: row.title, ref: shortRef(ref) });
}

/** @param {{ index: number; message?: string | null; branch?: string | null; untracked: boolean; at: number }} entry @param {string} ref */
export function poppedWords(entry, ref) {
  const row = stashRow(entry);
  return t("work-stash-popped-what-here-entry-itself-saved", { row: row.id, title: row.title, ref: shortRef(ref).split("-")[0] });
}

/** @param {{ index: number; message?: string | null; branch?: string | null; untracked: boolean; at: number }} entry @param {string} ref */
export function droppedWords(entry, ref) {
  const row = stashRow(entry);
  return t("work-stash-dropped-pinned-stash-drop-safety-restore", { row: row.id, title: row.title, ref: shortRef(ref).split("-")[0] });
}

/**
 * A refusal from a stash route, read from the node's `code` — never from the
 * status alone, which stands for several things.
 * @param {{ code?: string | null; status?: number; message: string; detail?: Record<string, unknown> | null }} err
 * @param {"push" | "apply" | "pop" | "drop"} verb
 */
export function stashRefusal(err, verb) {
  const code = err.code ?? null;
  if (code === "conflict") {
    const paths = Array.isArray(err.detail?.paths) ? err.detail.paths.map(String) : [];
    const n = paths.length;
    const kept = verb === "pop" ? ` ${t("work-stash-stash-entry-kept")}` : "";
    return {
      kind: "conflict",
      sentence: t("work-stash-stopped-conflicted-file-files-settle-them", { n, paths: paths.join(", "), flag: (n) ? "yes" : "no", kept }),
    };
  }
  if (code === "nothing_to_stash") return { kind: "nothing_to_stash", sentence: t("work-stash-nothing-stash-tree-has-no-change") };
  if (code === "stash_moved") return { kind: "stash_moved", sentence: t("work-stash-stash-list-moved-since-read-push") };
  if (code === "in_progress") return { kind: "in_progress", sentence: err.message };
  if (err.status === 409) return { kind: "dirty", sentence: err.message };
  return { kind: "error", sentence: err.message };
}

/**
 * The paths a stash may be scoped to — the checkout's tracked changes, each
 * once whether staged, unstaged or both; an untracked file travels only with
 * *Include untracked* on the whole tree, and a conflicted path is settled,
 * never stashed. What the dialog lists under *Only these files*.
 * @param {readonly {path: string, staged?: boolean, unstaged?: boolean, untracked?: boolean, conflicted?: boolean}[] | null | undefined} files
 * @returns {string[]}
 */
export function stashablePaths(files) {
  const out = [];
  const seen = new Set();
  for (const f of Array.isArray(files) ? files : []) {
    if (!f || typeof f.path !== "string" || f.path === "" || f.untracked || f.conflicted) continue;
    if (!f.staged && !f.unstaged) continue;
    if (seen.has(f.path)) continue;
    seen.add(f.path);
    out.push(f.path);
  }
  return out;
}
