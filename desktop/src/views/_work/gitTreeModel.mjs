/**
 * The Changes view's rows (ide/04): every changed file once, in the
 * project's own tree — the folders as the explorer draws them
 * (`ui/tree/pathTree.mjs`), or as a flat list of paths — in the order the
 * kit's tree (`ui/tree`) draws them. Pure, so `node --test` reads it;
 * `ChangesTree.tsx` paints, `GitPanel.tsx` composes.
 *
 * A file has one row whatever git says about it: its **standing**
 * (`standingOf`) — staged, unstaged, both, untracked, conflicted — is what
 * the row wears as chips and what its verbs derive from. A folder row
 * carries the **files** under it, never the folder as a pathspec: `git add
 * src` would also take every untracked file under `src/`, and a directory
 * spec with an unmerged file beneath it fails the checkout — so a folder's
 * verb sends exactly the files it lists that the verb applies to, and each
 * verb has a list of its own (*Stage 3* takes the three with something to
 * stage, *Unstage 2* the two staged).
 */

import { filesUnder, orderedChildren, pathTree } from "../../ui/tree/pathTree.mjs";
import { rowActions as rowActionsOf, standingOf } from "./gitFiles.mjs";
import { t } from "../../i18n/l10n.mjs";

export function dirId(path) {
  return `dir:${path}`;
}

export function fileId(path) {
  return `file:${path}`;
}

const pathOf = (row) => row.path;
const byPath = (a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0);

function basename(path) {
  const i = path.lastIndexOf("/");
  return i === -1 ? path : path.slice(i + 1);
}

/** The rows git reported that are rows: a path, once. */
function realRows(files) {
  const seen = new Set();
  const out = [];
  for (const f of Array.isArray(files) ? files : []) {
    if (!f || typeof f.path !== "string" || f.path === "" || seen.has(f.path)) continue;
    seen.add(f.path);
    out.push(f);
  }
  return out;
}

function fileRow(row, depth, label) {
  return { kind: "file", id: fileId(row.path), depth, label, row, standing: standingOf(row) };
}

/** What a folder's verbs take: the files under it that each verb applies to. */
function folderLists(files) {
  const stageable = [];
  const unstageable = [];
  const discardable = [];
  const deletable = [];
  for (const f of files) {
    const s = standingOf(f);
    if (s.unstaged || s.untracked || s.conflicted) stageable.push(f.path);
    if (s.staged) unstageable.push(f.path);
    if (s.unstaged || s.conflicted) discardable.push(f.path);
    if (s.untracked) deletable.push(f.path);
  }
  return { stageable, unstageable, discardable, deletable };
}

function emitDir(out, dir, depth, folded) {
  for (const child of orderedChildren(dir, pathOf)) {
    if (child.kind === "file") {
      out.push(fileRow(child.item, depth, basename(child.item.path)));
      continue;
    }
    const node = child.node;
    const id = dirId(node.path);
    const files = filesUnder(node, pathOf);
    const paths = files.map(pathOf);
    const expanded = !folded.has(id);
    out.push({ kind: "dir", id, depth, label: node.name, path: node.path, paths, count: paths.length, ...folderLists(files), expandable: true, expanded });
    if (expanded) emitDir(out, node, depth + 1, folded);
  }
}

/** The one group the tree draws above the project's folders: the unmerged paths, while any (ide/04 §Conflicts, continued). */
export const CONFLICTED_GROUP = "group:conflicted";

/**
 * The rows of the Changes tree, top to bottom: in the **tree** layout the
 * project's folders, each level folders first then files in the explorer's
 * order; in the **list** layout every file with its full path, by path. A
 * folded folder's children are left out. A file git lists twice — it does
 * not — would be one row. The **conflicted** paths come first, whatever
 * the layout — in the tree under one *Conflicted* group row (a folder-shaped
 * row with `group` set, foldable like one), in the list ahead of the rest —
 * so what an operation left to settle is never hunted for.
 * @param {readonly import("../../types").GitFileRow[] | null | undefined} files
 * @param {"tree" | "list"} layout
 * @param {ReadonlySet<string>} folded the folder ids folded shut
 */
export function changesTreeRows(files, layout, folded) {
  const rows = realRows(files);
  const conflicted = rows.filter((r) => r.conflicted === true).sort(byPath);
  const rest = rows.filter((r) => r.conflicted !== true);
  if (layout === "list") return [...conflicted, ...rest.sort(byPath)].map((row) => fileRow(row, 0, row.path));
  const out = [];
  if (conflicted.length > 0) {
    const expanded = !folded.has(CONFLICTED_GROUP);
    const paths = conflicted.map(pathOf);
    out.push({ kind: "dir", id: CONFLICTED_GROUP, group: "conflicted", depth: 0, label: t("work-git-tree-conflicted"), path: "", paths, count: paths.length, ...folderLists(conflicted), expandable: true, expanded });
    if (expanded) for (const row of conflicted) out.push(fileRow(row, 1, row.path));
  }
  emitDir(out, pathTree(rest, pathOf), 0, folded);
  return out;
}

/** Every folder id the tree layout would draw, folded or not — what *Collapse all* folds. */
export function foldableIds(files) {
  return changesTreeRows(files, "tree", new Set())
    .filter((r) => r.kind === "dir")
    .map((r) => r.id);
}

/** The folds after one row opened or shut. */
export function toggledFold(folds, id, open) {
  const rest = folds.filter((f) => f !== id);
  return open ? rest : [...rest, id];
}

/**
 * The row a selection lights: the file's one row, whichever side is open —
 * the side is the row's chip's to show.
 * @param {{path: string, staged: boolean} | null | undefined} selection
 * @returns {Set<string>}
 */
export function selectedIds(selection) {
  return selection ? new Set([fileId(selection.path)]) : new Set();
}

/**
 * Where the cursor is after the rows changed under it: the same row when it
 * is still there, else the row at its old place (clamped) — a file that was
 * staged away leaves the cursor on its neighbour, not on nothing.
 * @param {readonly {id: string}[]} prevRows
 * @param {string | null} cursor
 * @param {readonly {id: string}[]} rows
 */
export function nextCursor(prevRows, cursor, rows) {
  if (!cursor) return null;
  if (rows.some((r) => r.id === cursor)) return cursor;
  const was = prevRows.findIndex((r) => r.id === cursor);
  if (was === -1 || rows.length === 0) return null;
  return rows[Math.min(was, rows.length - 1)].id;
}

/** The list a folder row keeps for each verb. */
const FOLDER_LIST = Object.freeze({ stage: "stageable", unstage: "unstageable", discard: "discardable", delete: "deletable" });

/**
 * What a row's verb acts on — a file its path when the verb applies to its
 * standing, a folder the files under it the verb applies to — with the
 * folder named for the confirmation. `null` when the verb has nothing to
 * take.
 * @param {readonly object[]} rows
 * @param {string | null} id
 * @param {"stage" | "unstage" | "discard" | "delete"} verb
 * @returns {{kind: "dir" | "file", paths: string[], under: string | null} | null}
 */
export function actOn(rows, id, verb) {
  const row = id ? rows.find((r) => r.id === id) : null;
  if (!row) return null;
  if (row.kind === "file") {
    const applies = rowActionsOf(row.standing).some((a) => a.id === verb);
    return applies ? { kind: "file", paths: [row.row.path], under: null } : null;
  }
  const paths = row[FOLDER_LIST[verb]] ?? [];
  return paths.length > 0 ? { kind: "dir", paths: [...paths], under: row.path } : null;
}

/** What Space does on a row: stage its primary side, or unstage a file that is staged alone. */
export function spaceVerb(row) {
  if (row.kind === "file") return row.standing.staged && !row.standing.unstaged && !row.standing.untracked && !row.standing.conflicted ? "unstage" : "stage";
  return row.stageable.length > 0 ? "stage" : row.unstageable.length > 0 ? "unstage" : null;
}

/** What Delete does on a row: delete an untracked file, discard a working-tree change, nothing on a staged-only one. */
export function deleteVerb(row) {
  if (row.kind === "file") return row.standing.untracked ? "delete" : row.standing.unstaged || row.standing.conflicted ? "discard" : null;
  return row.discardable.length > 0 ? "discard" : row.deletable.length > 0 ? "delete" : null;
}


/**
 * The icon actions a folder row reveals, the mirror of a file's
 * (`rowActions`), each only when it has files to take: *Stage n*, *Unstage
 * n*, then *Discard changes…* over the working-tree changes under it and
 * *Delete files…* over the untracked ones. The label names the folder and
 * the count.
 * @param {{path: string, stageable: string[], unstageable: string[], discardable: string[], deletable: string[]}} dir
 */
export function folderActions(dir) {
  const actions = [];
  if (dir.stageable.length > 0) {
    actions.push({ id: "stage", icon: "stage", label: (path) => t("work-git-tree-stage-under", { stageable: dir.stageable.length, path }), tone: "quiet", hint: t("work-git-tree-put-every-file-listed-under-git") });
  }
  if (dir.unstageable.length > 0) {
    actions.push({ id: "unstage", icon: "unstage", label: (path) => t("work-git-tree-unstage-under", { unstageable: dir.unstageable.length, path }), tone: "quiet", hint: t("work-git-tree-take-every-staged-file-under-back") });
  }
  if (dir.discardable.length > 0) {
    actions.push({ id: "discard", icon: "discard", label: (path) => t("work-git-tree-discard-changes-under", { path }), tone: "danger", hint: t("work-git-tree-discard-changes-every-working-tree-change") });
  }
  if (dir.deletable.length > 0) {
    actions.push({ id: "delete", icon: "delete", label: (path) => t("work-git-tree-delete-untracked-files-under", { path }), tone: "danger", hint: t("work-git-tree-delete-files-untracked-files-under-through") });
  }
  return actions;
}

/**
 * A folder row's context menu: stage or unstage what it holds, throw its
 * working-tree changes away or delete its untracked files, its paths, and
 * where it is in Files — each verb only when it has files to take.
 * @param {{stageable: string[], unstageable: string[], discardable: string[], deletable: string[]}} dir
 * @param {{desktop: boolean}} ctx
 */
export function gitFolderMenu(dir, { desktop }) {
  const items = [];
  if (dir.stageable.length > 0) items.push({ id: "stage", label: t("work-git-tree-stage", { stageable: dir.stageable.length }) });
  if (dir.unstageable.length > 0) items.push({ id: "unstage", label: t("work-git-tree-unstage", { unstageable: dir.unstageable.length }) });
  if (dir.discardable.length > 0) items.push({ id: "discard", label: t("work-git-files-discard-changes-2"), separatorBefore: true, danger: true });
  if (dir.deletable.length > 0) items.push({ id: "delete", label: t("work-git-tree-delete-files"), separatorBefore: dir.discardable.length === 0, danger: true });
  items.push({ id: "copy-path", label: t("work-git-files-copy-relative-path"), separatorBefore: true });
  if (desktop) items.push({ id: "copy-absolute", label: t("work-git-files-copy-absolute-path") });
  items.push({ id: "reveal-files", label: t("work-git-files-reveal-files"), command: "reveal_in_files" });
  return items;
}

/**
 * A folder row's tooltip: the conflicted group says how many paths wait
 * and what follows; a folder says its path and how many changed files it
 * holds.
 * @param {{group?: string | null, path: string, count: number}} row
 */
export function dirRowTitle(row) {
  if (row.group === "conflicted") return t("work-git-tree-conflicted-files-settle-each", { count: row.count });
  return t("work-git-tree-folder-changed-files", { path: row.path, count: row.count });
}
