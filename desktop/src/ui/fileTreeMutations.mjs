/**
 * What the explorer can do to an entry, as facts (ide/03): which verbs a row
 * offers, what a duplicate is called, what a rename selects, and what the
 * delete confirmation promises — *moved to the Trash* or *removed*, by the
 * disposal the node resolved, never a guess. `fileTreeMutations.tsx` maps
 * these to calls; nothing here touches the network.
 */

import { parentPath } from "./fileTreeModel.mjs";
import { t as tr } from "../i18n/l10n.mjs";

export function basename(path) {
  const at = path.lastIndexOf("/");
  return at === -1 ? path : path.slice(at + 1);
}

/** `name.ext` → `[name, .ext]`; a dot-file keeps its whole name as the stem. */
export function splitExt(name) {
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return [name, ""];
  return [name.slice(0, dot), name.slice(dot)];
}

/**
 * The name a duplicate gets, the way a file manager does it: `foo copy.txt`,
 * then `foo copy 2.txt`, `foo copy 3.txt` — never a name a sibling holds.
 * @param {string} name the entry's own name
 * @param {Iterable<string>} siblings the names beside it
 */
export function nextCopyName(name, siblings) {
  const taken = new Set(siblings);
  const [stem, ext] = splitExt(name);
  const first = tr("ui-file-tree-mutations-copy", { stem, ext });
  if (!taken.has(first)) return first;
  for (let n = 2; n < 1000; n++) {
    const candidate = tr("ui-file-tree-mutations-copy-2", { stem, n, ext });
    if (!taken.has(candidate)) return candidate;
  }
  return tr("ui-file-tree-mutations-copy-3", { stem, now: Date.now(), ext });
}

/** What a rename prompt selects: the stem, so typing replaces the name and keeps the extension. */
export function renameSelection(name) {
  const [stem] = splitExt(name);
  return [0, stem.length];
}

/** Up to `max` names, then how many more — for a confirmation about many. */
function someNames(names, max = 5) {
  const shown = names.slice(0, max).join(", ");
  return names.length > max ? tr("ui-file-tree-mutations-some-names-more", { shown, more: names.length - max }) : shown;
}

/**
 * The delete confirmation's words, for one entry or many. `disposal` is the
 * node's answer (`trash` | `unlink` | null while unknown); `counts` a folder's
 * top-level entries by path (null while counting, -1 when the count failed).
 * @param {{targets: readonly {path: string, dir: boolean}[], counts: Record<string, number | null>, disposal: "trash" | "unlink" | null, gitRoot: boolean}} args
 */
export function deleteCopy({ targets, counts, disposal, gitRoot }) {
  const folders = targets.filter((t) => t.dir);
  const one = targets.length === 1 ? targets[0] : null;
  const title = one ? (one.dir ? tr("ui-file-tree-mutations-delete-folder", { one: one.path }) : tr("ui-file-tree-mutations-delete", { one: one.path })) : tr("ui-file-tree-mutations-delete-items", { targets: targets.length });
  const listed = one ? null : tr("ui-file-tree-mutations-listed", { names: someNames(targets.map((t) => basename(t.path))) });
  const tally = folders.map((f) => counts[f.path] ?? null);
  const holds =
    folders.length === 0
      ? null
      : tally.some((c) => c === null)
        ? tr("ui-file-tree-mutations-counting-what-inside")
        : tally.some((c) => c < 0)
          ? folders.length === 1
            ? tr("ui-file-tree-mutations-everything-inside-goes-too")
            : tr("ui-file-tree-mutations-everything-inside-folders-goes-too")
          : folders.length === 1
            ? tally[0] === 0
              ? one
                ? tr("ui-file-tree-mutations-empty")
                : tr("ui-file-tree-mutations-folder-empty", { folder: basename(folders[0].path) })
              : one
                ? tr("ui-file-tree-mutations-it-holds", { count: tally[0] })
                : tr("ui-file-tree-mutations-folder-holds", { folder: basename(folders[0].path), count: tally[0] })
            : tally.every((c) => c === 0)
              ? tr("ui-file-tree-mutations-folders-empty")
              : tr("ui-file-tree-mutations-folders-hold", { folders: folders.length, entries: tally.reduce((a, b) => a + b, 0) });
  const where =
    disposal === "trash"
      ? tr("ui-file-tree-mutations-moves-trash-where-can-get-back")
      : disposal === "unlink"
        ? gitRoot
          ? tr("ui-file-tree-mutations-removed-git-root-index-still-holds")
          : tr("ui-file-tree-mutations-removed-there-copy-anywhere")
        : tr("ui-file-tree-mutations-checking-how-root-disposes-deleted-files");
  const confirm = disposal === "trash" ? tr("ui-file-tree-mutations-move-trash") : tr("ui-file-tree-mutations-delete-verb");
  return { title, body: [listed, holds, where].filter(Boolean).join(" "), confirm, danger: disposal !== "trash" };
}

/**
 * The words for a verb that ran over several entries and may have stopped:
 * the fan-out is sequential and stops at the first refusal, so what was done
 * is done and what was not is named.
 * @param {{verb: string, done: readonly string[], failed: {path: string, reason: string} | null, total: number}} args
 * @returns {{ok: boolean, text: string}}
 */
export function fanOutSummary({ verb, done, failed, total }) {
  if (!failed) return { ok: true, text: tr("ui-file-tree-mutations-fan-out-done", { done: done.length, verb }) };
  const left = total - done.length - 1;
  const args = { done: done.length, total, verb, file: basename(failed.path), reason: failed.reason, left };
  return { ok: false, text: left > 0 ? tr("ui-file-tree-mutations-fan-out-stopped-more", args) : tr("ui-file-tree-mutations-fan-out-stopped", args) };
}

/**
 * The one verb for showing a file or folder to the OS — *Reveal in Finder*
 * on a Mac — read by every menu, button and card that offers it, so no
 * surface spells its own. The place is the platform's: "Finder" is wrong on
 * Windows and names nothing at all on Linux, and a menu item that promises
 * Finder to a Windows user is the kind of wrongness that reads as
 * carelessness. The verb is the same everywhere. The user agent is the fact
 * the app has.
 * @param {string | null | undefined} userAgent
 */
export function revealLabel(userAgent) {
  const ua = userAgent ?? "";
  if (/Mac|iPhone|iPad/.test(ua)) return tr("ui-file-tree-mutations-reveal-finder");
  if (/Win/.test(ua)) return tr("ui-file-tree-mutations-reveal-file-explorer");
  return tr("ui-file-tree-mutations-reveal-file-manager");
}

/**
 * The verbs the selected rows offer, in order, with separators. One row gets
 * every verb; several get the ones that mean something for a set — cut, copy,
 * duplicate, the paths, delete — each counting what it acts on, and none that
 * needs one place (new, rename, paste, reveal). `desktop` says the shell is
 * here (Reveal needs a file manager); `mutable` that the root is writable — a
 * read-only tree offers only the paths and the reveal; `clipboard` what a
 * paste would take from — the tree's own (`"tree"`), the file manager's
 * (`"os"`, a copy made in Finder, said in the label), the picture on the
 * clipboard (`"image"`, said too), or nothing. Each item
 * names the keymap command that means the same thing (`command`), so a menu
 * can show its chord.
 * @param {{targets: readonly {path: string, dir: boolean}[], desktop: boolean, mutable: boolean, clipboard: "tree" | "os" | "image" | null, reveal: string, serve?: boolean}} args
 * @returns {{id: string, label: string, command?: string, danger?: boolean, disabled?: boolean, separatorBefore?: boolean}[]}
 */
export function menuSpec({ targets, desktop, mutable, clipboard, reveal, serve = false }) {
  const n = targets.length;
  const one = n === 1 ? targets[0] : null;
  const items = [];
  if (mutable) {
    if (one) {
      items.push(
        { id: "new-file", label: tr("ui-file-tree-mutations-new-file"), command: "new_file" },
        { id: "new-dir", label: tr("ui-file-tree-mutations-new-folder"), command: "new_folder" },
        { id: "rename", label: tr("ui-file-tree-mutations-rename"), command: "rename_entry", separatorBefore: true },
      );
    }
    items.push(
      { id: "duplicate", label: tr("ui-file-tree-mutations-duplicate", { n }), command: "duplicate_entry" },
      { id: "cut", label: tr("ui-file-tree-mutations-cut", { n }), command: "cut_entry" },
      { id: "copy", label: tr("ui-file-tree-mutations-copy-verb", { n }), command: "copy_entry" },
    );
    if (one) items.push({ id: "paste", label: pasteLabel(clipboard, one.dir ? tr("ui-file-tree-mutations-into-folder") : tr("ui-file-tree-mutations-beside")), command: "paste_entry", disabled: !clipboard });
  }
  items.push(
    { id: "copy-path", label: one ? tr("ui-file-tree-mutations-copy-relative-path") : tr("ui-file-tree-mutations-copy-relative-paths"), separatorBefore: items.length > 0 },
    { id: "copy-absolute", label: one ? tr("ui-file-tree-mutations-copy-absolute-path") : tr("ui-file-tree-mutations-copy-absolute-paths") },
  );
  if (desktop && one) items.push({ id: "reveal", label: reveal });
  // A folder of a checkout served on this machine and opened in the embedded browser (ide/18).
  if (serve && one && one.dir) items.push({ id: "serve", label: tr("ui-file-tree-mutations-serve-folder"), separatorBefore: true });
  if (mutable) items.push({ id: "delete", label: one ? (one.dir ? tr("ui-file-tree-mutations-delete-folder-2") : tr("ui-file-tree-mutations-delete-2")) : tr("ui-file-tree-mutations-delete-items-2", { n }), command: "delete_entry", danger: true, separatorBefore: true });
  return items;
}

/** The paste's word: where it goes, and — when the machine's clipboard is the source — what comes: the file manager's files, or the picture. */
function pasteLabel(clipboard, where) {
  const what = clipboard === "os" ? tr("ui-file-tree-mutations-paste-from-file-manager") : clipboard === "image" ? tr("ui-file-tree-mutations-paste-picture") : tr("ui-file-tree-mutations-paste");
  return where ? tr("ui-file-tree-mutations-paste-where", { what, where }) : what;
}

/**
 * The verbs the tree's background offers — the root's, with nothing selected.
 * @param {{desktop: boolean, mutable: boolean, clipboard: "tree" | "os" | "image" | null, reveal: string}} args
 */
export function viewportMenuSpec({ desktop, mutable, clipboard, reveal }) {
  const items = [];
  if (mutable) {
    items.push(
      { id: "new-file", label: tr("ui-file-tree-mutations-new-file"), command: "new_file" },
      { id: "new-dir", label: tr("ui-file-tree-mutations-new-folder"), command: "new_folder" },
      { id: "paste", label: pasteLabel(clipboard, ""), command: "paste_entry", disabled: !clipboard },
    );
  }
  if (desktop) items.push({ id: "reveal", label: tr("ui-file-tree-mutations-root", { reveal }), separatorBefore: items.length > 0 });
  items.push({ id: "refresh", label: tr("ui-file-tree-refresh"), separatorBefore: items.length > 0 });
  return items;
}

/**
 * What the name typed into an inline rename must satisfy: something, no
 * slash (a move is the drag or the drop, not the rename), no `..`, and not a
 * sibling's name. Null when it is fine.
 * @param {string} value
 * @param {string} current the entry's own name
 * @param {Iterable<string>} siblings the names beside it
 */
export function renameError(value, current, siblings) {
  const v = value.trim();
  if (!v) return tr("ui-dialog-name-needed");
  if (v.includes("/") || v.includes("\\")) return tr("ui-file-tree-mutations-name-has-slash-drag-entry-move");
  if (v === "." || v === "..") return tr("ui-file-tree-mutations-name");
  if (v !== current && new Set(siblings).has(v)) return tr("ui-file-tree-mutations-already-here", { v });
  return null;
}

/** The folder a new entry lands in when the row is `path`: the folder itself, or the file's parent. */
export function targetDir(path, dir) {
  return dir ? path : parentPath(path);
}
