/**
 * A remote's branches as the Remotes section draws them (ide/04 §Remotes):
 * a **tree** in the explorer's shape — `feature/x` and `feature/y` under
 * `feature`, folders first, the explorer's order, through the kit's
 * `pathTree` — or a flat **list**, newest first. Pure, so `node --test`
 * reads it; `RemotesSection.tsx` paints the rows with the kit's `TreeList`.
 *
 * A row never carries the remote's prefix: `origin/` is the remote's row's
 * to say, once. What a row carries beyond its name is its **standing** —
 * the project's default branch, the local branch that tracks it — and the
 * commit it points at, as the row's tooltip.
 */

import { orderedChildren, pathTree } from "../../ui/tree/pathTree.mjs";
import { branchFilter } from "./branchActionsModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The id a folder folds under: the collapsed store's, per checkout and remote. */
export function folderId(wid, remote, path) {
  return `remote.${wid}.${remote}.${path}`;
}

function branchId(remote, name) {
  return `remote-branch:${remote}/${name}`;
}

const nameOf = (b) => b.name;
const newestFirst = (a, b) => b.timestamp - a.timestamp;

function basename(path) {
  const i = path.lastIndexOf("/");
  return i === -1 ? path : path.slice(i + 1);
}

/**
 * What a remote branch stands for, as its chips: `default` when it is the
 * project's default branch, `trackedBy` the local branch whose upstream is
 * it. Both may hold; neither says anything a person did not set.
 * @param {{name: string, trackedBy?: string | null}} branch
 * @param {{defaultBranch?: string | null}} [ctx]
 * @returns {{isDefault: boolean, trackedBy: string | null}}
 */
export function standingOf(branch, { defaultBranch = null } = {}) {
  return {
    isDefault: defaultBranch !== null && defaultBranch !== undefined && branch.name === defaultBranch,
    trackedBy: branch.trackedBy ?? null,
  };
}

/**
 * The row's tooltip: the whole name, the commit's subject and its sha —
 * what the row itself no longer spends its width on.
 * @param {{remote: string, name: string, subject?: string | null, head?: string | null}} branch
 */
export function branchTitle(branch) {
  const parts = [`${branch.remote}/${branch.name}`];
  if (branch.subject) parts.push(String(branch.subject));
  if (branch.head) parts.push(String(branch.head).slice(0, 12));
  return parts.join(" · ");
}

function branchRow(wid, remote, branch, depth, label, ctx) {
  return { kind: "branch", id: branchId(remote, branch.name), depth, label, branch, standing: standingOf(branch, ctx), title: branchTitle(branch) };
}

/**
 * A folder's branches, in the tree's order — what a folder's count says.
 * @param {import("../../ui/tree/pathTree.mjs").PathNode<any>} dir
 */
function countUnder(dir) {
  let n = dir.files.length;
  for (const child of dir.dirs.values()) n += countUnder(child);
  return n;
}

function emitDir(out, wid, remote, dir, depth, folded, forceOpen, ctx) {
  for (const child of orderedChildren(dir, nameOf)) {
    if (child.kind === "file") {
      out.push(branchRow(wid, remote, child.item, depth, basename(child.item.name), ctx));
      continue;
    }
    const id = folderId(wid, remote, child.node.path);
    const expanded = forceOpen || !folded.has(id);
    out.push({ kind: "dir", id, depth, label: child.node.name, path: child.node.path, count: countUnder(child.node), expandable: true, expanded });
    if (expanded) emitDir(out, wid, remote, child.node, depth + 1, folded, forceOpen, ctx);
  }
}

/**
 * The rows of one remote's branches, top to bottom. In the **tree** layout
 * the folders as the explorer draws them, a folded folder's children left
 * out; in the **list** layout every branch with its whole name, newest
 * first. A filter keeps the branches it matches and, in the tree, the
 * folders holding one — opened, so a hit is never behind a fold.
 * @param {string} wid
 * @param {{remote: {name: string}, branches: readonly {remote: string, name: string, head: string, subject: string, timestamp: number, trackedBy?: string | null}[]}} group
 * @param {{layout: "tree" | "list", folded: ReadonlySet<string>, filter?: string, defaultBranch?: string | null}} opts
 */
export function remoteBranchRows(wid, group, { layout, folded, filter = "", defaultBranch = null }) {
  const remote = group.remote.name;
  const ctx = { defaultBranch };
  const filtering = String(filter ?? "").trim() !== "";
  const branches = filtering ? branchFilter(group.branches, filter) : [...group.branches];
  if (layout === "list") {
    return branches.sort(newestFirst).map((b) => branchRow(wid, remote, b, 0, b.name, ctx));
  }
  const out = [];
  emitDir(out, wid, remote, pathTree(branches, nameOf), 0, folded, filtering, ctx);
  return out;
}

/** Every folder id the tree layout would draw for the group, folded or not. */
export function foldableIds(wid, group) {
  return remoteBranchRows(wid, group, { layout: "tree", folded: new Set() })
    .filter((r) => r.kind === "dir")
    .map((r) => r.id);
}

/** The words the count chip wears on a remote's row. */
export function countWords(n) {
  return n === 1 ? "1 branch" : t("work-remote-tree-branches", { n });
}

/** Whether the section is long enough to want its filter field. */
export function wantsFilter(groups, threshold = 6) {
  return groups.reduce((n, g) => n + g.branches.length, 0) > threshold;
}
