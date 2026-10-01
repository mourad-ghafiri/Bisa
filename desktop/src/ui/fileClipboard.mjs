/**
 * The explorer's clipboard (ide/03): a cut or a copy of one or more entries
 * under one root, and what a paste into a folder would do about it — as
 * facts. A paste is a list of engine calls (`copy` or `move` per entry) or a
 * refusal with its reason; nothing here touches the network. The copy and
 * move routes stay within one root, so the clipboard does too.
 */

import { isDescendant, isImmediateChild, joinPath } from "./fileTreeModel.mjs";
import { basename, nextCopyName } from "./fileTreeMutations.mjs";
import { t } from "../i18n/l10n.mjs";

/**
 * A clipboard entry, or null when there is nothing to hold.
 * @param {"copy" | "cut"} kind
 * @param {string} root the tree's key (`scope:id`) — a paste elsewhere is refused
 * @param {Iterable<string>} paths
 */
export function clipFrom(kind, root, paths) {
  const list = [...new Set(paths)].filter((p) => typeof p === "string" && p !== "");
  if (list.length === 0) return null;
  return { kind, root, paths: list };
}

/** Whether a row reads as cut: dimmed, still there until the paste. */
export function isCut(clip, path) {
  return Boolean(clip && clip.kind === "cut" && clip.paths.includes(path));
}

/**
 * What pasting the clipboard into `targetDir` does.
 * @param {{kind: "copy" | "cut", root: string, paths: string[]} | null} clip
 * @param {{targetDir: string, root: string, existing: Iterable<string>}} into `existing` is the names already in the target
 * @returns {{ops: {from: string, to: string, op: "copy" | "move"}[]} | {refused: string, idle?: true}} `idle`: the paste would change nothing — where it already sits
 */
export function pasteTargets(clip, { targetDir, root, existing }) {
  if (!clip) return { refused: t("ui-file-clipboard-nothing-paste") };
  if (clip.root !== root) return { refused: t("ui-file-clipboard-clipboard-holds-entries-from-another-tree") };
  const taken = new Set(existing);
  const ops = [];
  for (const from of clip.paths) {
    const name = basename(from);
    if (clip.kind === "cut") {
      if (from === targetDir || isDescendant(from, targetDir)) return { refused: t("ui-file-clipboard-cannot-moved-into-itself", { name }) };
      if (isImmediateChild(targetDir, from)) continue; // already here: nothing to do
      if (taken.has(name)) return { refused: targetDir ? t("ui-file-clipboard-already-in", { name, dir: targetDir }) : t("ui-file-clipboard-already-in-root", { name }) };
      taken.add(name);
      ops.push({ from, to: joinPath(targetDir, name), op: "move" });
    } else {
      const to = taken.has(name) ? nextCopyName(name, taken) : name;
      taken.add(to);
      ops.push({ from, to: joinPath(targetDir, to), op: "copy" });
    }
  }
  // Nothing to do is not a refusal worth a toast: `idle` says so, so no caller has to read the words to tell.
  if (ops.length === 0) return { refused: t("ui-file-clipboard-already-there"), idle: true };
  return { ops };
}

/** The clipboard after a paste: a cut is spent, a copy can be pasted again. */
export function afterPaste(clip) {
  return clip && clip.kind === "copy" ? clip : null;
}

/** The words for what a paste did. */
export function pasteSummary(ops) {
  const moved = ops.filter((o) => o.op === "move").length;
  const copied = ops.length - moved;
  if (moved && copied) return t("ui-file-clipboard-summary-both", { moved, copied });
  return moved ? t("ui-file-clipboard-summary-moved", { moved }) : t("ui-file-clipboard-summary-copied", { copied });
}
