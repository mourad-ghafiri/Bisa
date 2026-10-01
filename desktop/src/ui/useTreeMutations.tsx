/**
 * What the explorer can *do* to a tree (ide/03): new file, new folder,
 * rename inline, duplicate, cut, copy, paste, move, copy a path, reveal it,
 * delete — each an engine call through `/ide/files`, each confined to the
 * writable root by the node, each landing on the watcher stream so every
 * window sees it. The facts — which verbs, what a duplicate is called, what
 * a paste does, what the confirmation promises — are `fileTreeMutations.mjs`'s
 * and `fileClipboard.mjs`'s; this hook (its own file, so the bundler never
 * mistakes it for the model) owns the dialogs and the calls, and
 * `FileTree.tsx` decides where they hang.
 *
 * Every verb has two doors: a menu item and a keymap command
 * (`explorerStore.ts` delivers the command to the focused tree). Both arrive
 * at {@link run} with the **targets** — the selection, or the one row asked
 * about (`fileTreeModel.targetsOf`) — so the menu and the chord can never
 * disagree, and a verb over many is the same verb over one, run in turn.
 *
 * The engine's file routes take one path each and there is no batch route,
 * so a verb over many fans out here: sequentially, stopping at the first
 * refusal, and saying what was done and what was not (`fanOutSummary`). The
 * listings refresh from the watcher's frames as they do for one.
 *
 * A drag within the tree is the kit's (`ui/dnd`, `ui/tree`): the rows carry
 * a `pathDrag` payload, the tree projects where they would land, and the drop
 * arrives here as {@link move} — a paste of a cut, with its refusals.
 */

import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { api, inDesktopShell, pasteImageInto, pasteInto, pasteboardHolds, revealPath } from "../api";
import type { Disposal, FileScope } from "../types";
import { ConfirmDialog } from "./Dialog";
import type { ExplorerCommand } from "./explorerStore";
import { afterPaste, clipFrom, pasteSummary, pasteTargets } from "./fileClipboard.mjs";
import { setFileClipboard, useFileClipboard } from "./fileClipboardStore";
import type { MenuItem } from "./Menu";
import { copyText as copyToClipboard } from "./clipboard";
import { ICON } from "./icons";
import { refreshOsClipboard, useOsClipboard } from "./osClipboardStore";
import { pasteDestination, pasteRefusal, pasteSource, pasteWords } from "./osPasteModel.mjs";
import { imageNameError, pastedImageName, withExtension } from "./pastedImageModel.mjs";
import { chordHint } from "./keymapHints";
import { useToast } from "./Toast";
import {
  basename,
  deleteCopy,
  fanOutSummary,
  menuSpec,
  nextCopyName,
  renameError,
  revealLabel,
  targetDir,
  viewportMenuSpec,
} from "./fileTreeMutations.mjs";
import type { MenuId, MenuSpecItem } from "./fileTreeMutations.mjs";
import { joinPath, parentPath, type Target } from "./fileTreeModel.mjs";
import { t as tr } from "../i18n/l10n.mjs";

/** Computed once: the platform does not change while the app runs. */
const REVEAL_LABEL = revealLabel(typeof navigator === "undefined" ? "" : navigator.userAgent);

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/**
 * An in-tree draft being named: a new file or folder (an empty field), or
 * a picture pasted from the clipboard (`name` pre-filled, the stem selected,
 * written by the shell under the name confirmed).
 */
type Creating = { dir: string; kind: "file" | "dir" | "image"; name?: string; error: string | null } | null;

/** The entries a delete would take, and each folder's top-level count (null while counting, -1 when it failed). */
type Deleting = { targets: Target[]; counts: Record<string, number | null> } | null;

/** The root, as a verb's target when no row is under the cursor. */
const ROOT_TARGET: Target = { path: "", dir: true };

const MENU_ICON: Record<MenuId, (typeof ICON)[keyof typeof ICON]> = {
  "new-file": ICON.file,
  "new-dir": ICON.folder,
  rename: ICON.edit,
  duplicate: ICON.duplicate,
  cut: ICON.cut,
  copy: ICON.copy,
  paste: ICON.paste,
  "copy-path": ICON.copy,
  "copy-absolute": ICON.copy,
  reveal: ICON.reveal,
  serve: ICON.play,
  delete: ICON.delete,
  refresh: ICON.refresh,
};

/** The menu id a keymap command is another door to; `open_entry` is the tree's own. */
const COMMAND_MENU: Record<ExplorerCommand, MenuId | null> = {
  rename_entry: "rename",
  open_entry: null,
  delete_entry: "delete",
  copy_entry: "copy",
  cut_entry: "cut",
  paste_entry: "paste",
  duplicate_entry: "duplicate",
  new_file: "new-file",
  new_folder: "new-dir",
  select_all_entries: null,
};

/** The commands that need a row under the cursor; the others take the root. */
const NEEDS_ROW: readonly MenuId[] = ["rename", "duplicate", "cut", "copy", "delete"];

export function useTreeMutations({
  scope,
  id,
  rootKey,
  root,
  gitRoot,
  mutable,
  siblingsOf,
  childrenOf,
  refresh,
  onDeleted,
  onServe,
  onCreated,
}: {
  scope: FileScope;
  id: string;
  /** The tree's key (`scope:id`) — what the clipboard is scoped by. */
  rootKey: string;
  /** The absolute root, once the listing has said it; Reveal and *Copy absolute path* need it. */
  root: string | null;
  /** Whether the root is a repository — what an unlink means differs. */
  gitRoot: boolean;
  /** Whether the root is writable. A read-only tree offers only the paths and the reveal. */
  mutable: boolean;
  /** The names beside a path, for a duplicate's name and a rename's collision check. */
  siblingsOf: (path: string) => string[];
  /** The names inside a folder (the root is `""`), for a paste's collision check. */
  childrenOf: (dir: string) => string[];
  refresh: () => void;
  /** Files that were deleted, in one word, so their open tabs can close together. Renames follow through the watcher's frame. */
  onDeleted?: (paths: string[]) => void;
  /** Serve a folder of the checkout on this machine and open it in the embedded browser (ide/18); absent where nothing can. */
  onServe?: (path: string) => void;
  /** An entry was created here — a new file is the caller's to open. */
  onCreated?: (path: string, kind: "file" | "dir") => void;
}) {
  const toast = useToast();
  const clip = useFileClipboard();
  // A copy made in the file manager, or a picture: the shell holds them, and
  // a paste with nothing on the tree's own clipboard takes them (ide/03).
  const held = useOsClipboard();
  /** An in-tree new file/folder being named, and what is wrong so far. */
  const [creating, setCreating] = useState<Creating>(null);
  const [deleting, setDeleting] = useState<Deleting>(null);
  const [disposal, setDisposal] = useState<Disposal | null>(null);
  const [busy, setBusy] = useState(false);
  /** The row being renamed in place, and what is wrong with the name so far. */
  const [renaming, setRenaming] = useState<{ path: string; error: string | null } | null>(null);

  // A different root is a different tree: a draft or a rename half-typed in
  // the old one names nothing here, and would otherwise stick until cancelled.
  useEffect(() => {
    setCreating(null);
    setRenaming(null);
  }, [rootKey]);

  const fail = (e: unknown) => toast.error(message(e));

  /**
   * The same call for each target in turn, stopping at the first refusal: the
   * engine takes one path per request, so this is the batch. What was done
   * stays done; the toast says how far it got. Answers the paths it did.
   */
  const fanOut = async (verb: string, targets: readonly Target[], each: (t: Target) => Promise<void>): Promise<string[]> => {
    const done: string[] = [];
    let failed: { path: string; reason: string } | null = null;
    for (const t of targets) {
      try {
        await each(t);
        done.push(t.path);
      } catch (e) {
        failed = { path: t.path, reason: message(e) };
        break;
      }
    }
    if (targets.length > 1 || failed) {
      const words = fanOutSummary({ verb, done, failed, total: targets.length });
      if (words.ok) toast.ok(words.text);
      else toast.error(words.text);
    }
    return done;
  };

  // How this root disposes of a delete, asked when the confirmation opens —
  // the setting may have changed since the last time.
  const deletingKey = deleting ? deleting.targets.map((t) => t.path).join("\0") : null;
  useEffect(() => {
    if (deletingKey === null) return;
    let alive = true;
    setDisposal(null);
    api
      .ideDisposal(scope, id)
      .then((r) => alive && setDisposal(r.disposal))
      .catch(() => alive && setDisposal(null));
    return () => {
      alive = false;
    };
  }, [deletingKey, scope, id]);

  // ---- inline create (a draft row in the tree) ------------------

  const startCreate = (kind: "file" | "dir", dir: string) => {
    if (!mutable) return;
    setCreating({ dir, kind, error: null });
  };
  const cancelCreate = () => setCreating(null);

  /** Why the draft's name will not do, by the draft's kind: a picture's name has the store's extra rule. */
  const draftError = (c: NonNullable<Creating>, value: string) =>
    c.kind === "image" ? imageNameError(withExtension(value, "png"), childrenOf(c.dir)) : renameError(value, "", childrenOf(c.dir));

  /** Validate the draft name as typed, against the target folder's children. */
  const checkCreate = (value: string) => {
    setCreating((c) => (c ? { ...c, error: draftError(c, value) } : c));
  };

  const commitCreate = async (value: string) => {
    if (!creating || busy) return;
    const problem = draftError(creating, value);
    if (problem) {
      setCreating({ ...creating, error: problem });
      return;
    }
    setBusy(true);
    try {
      const path = creating.kind === "image" ? await writePicture(creating.dir, value) : await createEntry(creating.dir, creating.kind, value);
      setCreating(null);
      // Before the refresh: the caller opens the file, and the reveal that
      // follows the open document selects the row once the listing has it.
      onCreated?.(path, creating.kind === "dir" ? "dir" : "file");
      refresh();
    } catch (e) {
      // The refusal — the node's, or the shell's — stays under the field, like a rename's does.
      setCreating((c) => (c ? { ...c, error: e instanceof Error ? e.message : String(e) } : c));
    } finally {
      setBusy(false);
    }
  };

  /** A new file or folder, by the node. Answers its path. */
  const createEntry = async (dir: string, kind: "file" | "dir", value: string) => {
    const path = joinPath(dir, value.trim());
    await api.ideCreate(scope, id, path, kind);
    toast.ok(tr("ui-use-tree-mutations-created", { kind, path }));
    return path;
  };

  /**
   * The clipboard's picture, written by the shell under the confirmed name
   * — read as it is written, so what lands is what the clipboard holds
   * then. Answers its path.
   */
  const writePicture = async (dir: string, value: string) => {
    const dest = pasteDestination(root, dir);
    if (!dest) throw new Error("The folder's place on this machine is not known yet.");
    const landed = await pasteImageInto(dest, withExtension(value, "png"));
    toast.ok(pasteWords({ pasted: [landed], skipped: [] }, dir).text);
    return joinPath(dir, landed.name);
  };

  // ---- inline rename ------------------------------------------------------

  const startRename = (path: string) => {
    if (!mutable || !path) return;
    setRenaming({ path, error: null });
  };

  const cancelRename = () => setRenaming(null);

  const othersBeside = (path: string) => {
    const current = basename(path);
    return siblingsOf(path).filter((n) => n !== current);
  };

  /** Validate as typed, so the row says what is wrong before Enter. */
  const checkRename = (value: string) => {
    if (!renaming) return;
    setRenaming({ path: renaming.path, error: renameError(value, basename(renaming.path), othersBeside(renaming.path)) });
  };

  const commitRename = async (value: string) => {
    if (!renaming || busy) return;
    const problem = renameError(value, basename(renaming.path), othersBeside(renaming.path));
    if (problem) {
      setRenaming({ path: renaming.path, error: problem });
      return;
    }
    const to = joinPath(parentPath(renaming.path), value.trim());
    if (to === renaming.path) {
      setRenaming(null);
      return;
    }
    setBusy(true);
    try {
      await api.ideMove(scope, id, renaming.path, to);
      toast.ok(tr("ui-use-tree-mutations-renamed", { to }));
      setRenaming(null);
      refresh();
    } catch (e) {
      // The node's refusal stays under the row rather than in a toast: the
      // field is still open and the words belong beside it.
      setRenaming({ path: renaming.path, error: e instanceof Error ? e.message : String(e) });
    } finally {
      setBusy(false);
    }
  };

  // ---- copy / cut / paste --------------------------------------------------

  const cut = (targets: readonly Target[]) => setFileClipboard(clipFrom("cut", rootKey, targets.map((t) => t.path)));
  const copy = (targets: readonly Target[]) => setFileClipboard(clipFrom("copy", rootKey, targets.map((t) => t.path)));

  /**
   * Apply a paste plan — the clipboard's, or a drag's, which is a cut pasted
   * where it landed. A cut that stops partway is spent only for what moved:
   * the rest is still on the clipboard, still where it was.
   */
  const applyPaste = async (from: { kind: "copy" | "cut"; root: string; paths: string[] } | null, ops: { from: string; to: string; op: "copy" | "move" }[]) => {
    setBusy(true);
    try {
      const done = await fanOut(
        ops.every((o) => o.op === "move") ? "moved" : ops.every((o) => o.op === "copy") ? "copied" : "pasted",
        ops.map((o) => ({ path: o.from, dir: false })),
        async (t) => {
          const op = ops.find((o) => o.from === t.path);
          if (!op) return;
          if (op.op === "move") await api.ideMove(scope, id, op.from, op.to);
          else await api.ideCopy(scope, id, op.from, op.to);
        },
      );
      if (done.length === ops.length) {
        if (from === clip) setFileClipboard(afterPaste(clip));
        if (ops.length === 1) toast.ok(pasteSummary(ops));
      } else if (from && from.kind === "cut") {
        const moved = new Set(done);
        setFileClipboard(clipFrom("cut", from.root, from.paths.filter((p) => !moved.has(p))));
      }
      refresh();
    } finally {
      setBusy(false);
    }
  };

  /**
   * Paste into a folder: the tree's clipboard when it holds something, else
   * what the machine's holds — files copied by the shell into the folder's
   * absolute path, the watcher announcing what landed; a picture named in a
   * draft row first, written on Enter (`commitCreate`).
   */
  const paste = async (intoDir: string) => {
    if (clip) {
      const plan = pasteTargets(clip, { targetDir: intoDir, root: rootKey, existing: childrenOf(intoDir) });
      if ("refused" in plan) {
        toast.error(plan.refused);
        return;
      }
      await applyPaste(clip, plan.ops);
      return;
    }
    const holds = await pasteboardHolds();
    if (holds.paths.length === 0) {
      // A picture: named in place, as a new file is, and written on Enter.
      if (holds.image && mutable) setCreating({ dir: intoDir, kind: "image", name: pastedImageName(), error: null });
      else toast.info(pasteRefusal());
      return;
    }
    const sources = holds.paths;
    const dest = pasteDestination(root, intoDir);
    if (!dest) {
      toast.error(tr("ui-use-tree-mutations-folder-s-place-machine-known-yet"));
      return;
    }
    setBusy(true);
    try {
      const report = await pasteInto(sources, dest);
      const words = pasteWords(report, intoDir);
      if (words.tone === "ok") toast.ok(words.text);
      else if (words.tone === "warn") toast.info(words.text);
      else toast.error(words.text);
      refresh();
    } catch (e) {
      toast.error(message(e));
    } finally {
      setBusy(false);
    }
  };

  const duplicate = async (targets: readonly Target[]) => {
    const done = await fanOut("duplicated", targets, async (t) => {
      await api.ideCopy(scope, id, t.path, joinPath(parentPath(t.path), nextCopyName(basename(t.path), siblingsOf(t.path))));
    });
    if (targets.length === 1 && done.length === 1) toast.ok(tr("ui-use-tree-mutations-duplicated", { path: targets[0].path }));
    refresh();
  };

  const copyText = (text: string) => {
    void copyToClipboard(text).then((ok) => (ok ? toast.ok(tr("ui-field-copied")) : toast.error(tr("ui-mermaid-view-clipboard-refused"))));
  };

  const reveal = async (path: string) => {
    if (!root) return;
    try {
      await revealPath(joinPath(root, path));
    } catch (e) {
      fail(e);
    }
  };

  const askDelete = async (targets: readonly Target[]) => {
    if (targets.length === 0) return;
    const counts: Record<string, number | null> = {};
    for (const t of targets) if (t.dir) counts[t.path] = null;
    setDeleting({ targets: [...targets], counts });
    // The confirmation shows what a recursive delete would take with it: each
    // folder's top-level count, asked for together.
    await Promise.all(
      targets
        .filter((t) => t.dir)
        .map((t) =>
          api
            .tree(scope, id, t.path, 1)
            .then((tree) => tree.entries.length)
            .catch(() => -1)
            .then((n) => setDeleting((d) => (d && d.targets === targets ? { ...d, counts: { ...d.counts, [t.path]: n } } : d))),
        ),
    );
  };

  const confirmDelete = async () => {
    if (!deleting || busy) return;
    setBusy(true);
    try {
      const { targets } = deleting;
      let disposalOf: Disposal | null = null;
      const done = await fanOut("deleted", targets, async (t) => {
        disposalOf = (await api.ideDelete(scope, id, t.path, t.dir)).disposal;
      });
      if (done.length > 0) onDeleted?.(done);
      if (targets.length === 1 && done.length === 1) {
        toast.ok(disposalOf === "trash" ? tr("ui-use-tree-mutations-moved-trash", { path: targets[0].path }) : tr("ui-use-tree-mutations-deleted", { path: targets[0].path }));
      }
      setDeleting(null);
      refresh();
    } finally {
      setBusy(false);
    }
  };

  /** A drop onto a folder: the dragged rows pasted there as a cut, with a paste's refusals. */
  const move = useCallback(
    async (paths: readonly string[], toDir: string) => {
      const plan = pasteTargets(clipFrom("cut", rootKey, paths), { targetDir: toDir, root: rootKey, existing: childrenOf(toDir) });
      if ("refused" in plan) {
        if (!plan.idle) toast.error(plan.refused);
        return;
      }
      await applyPaste(null, plan.ops);
    },
    // `applyPaste` is a plain function remade every render; listing it would
    // give `move` a new identity each render and re-wire every drop target.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [scope, id, rootKey, childrenOf, refresh],
  );

  /**
   * One door for every verb: the menu and the keymap both arrive here with
   * the targets. A verb that needs one place — new, rename, paste, reveal —
   * takes the first.
   */
  const run = (menuId: MenuId, targets: readonly Target[]) => {
    const first = targets[0] ?? ROOT_TARGET;
    const paths = targets.map((t) => t.path);
    switch (menuId) {
      case "new-file":
        return startCreate("file", targetDir(first.path, first.dir));
      case "new-dir":
        return startCreate("dir", targetDir(first.path, first.dir));
      case "rename":
        return startRename(first.path);
      case "duplicate":
        return void duplicate(targets);
      case "cut":
        return cut(targets);
      case "copy":
        return copy(targets);
      case "paste":
        return void paste(targetDir(first.path, first.dir));
      case "copy-path":
        return copyText(paths.join("\n"));
      case "copy-absolute":
        return copyText(paths.map((p) => (root ? joinPath(root, p) : p)).join("\n"));
      case "reveal":
        return void reveal(first.path);
      case "serve":
        return onServe?.(first.path);
      case "delete":
        return void askDelete(targets);
      case "refresh":
        return refresh();
      default:
        return undefined;
    }
  };

  /**
   * A keymap command on the targets (the root, with none). Returns false for
   * the commands the tree answers itself (`open_entry`, `select_all_entries`),
   * so the caller can.
   */
  const command = (cmd: ExplorerCommand, targets: readonly Target[]): boolean => {
    const menuId = COMMAND_MENU[cmd];
    if (!menuId) return false;
    // A read-only tree has no verb for it; the chord is still consumed.
    if (!mutable) return true;
    if (targets.length === 0 && NEEDS_ROW.includes(menuId)) return true;
    run(menuId, targets);
    return true;
  };

  const toItem = (item: MenuSpecItem, targets: readonly Target[]): MenuItem => ({
    label: item.label,
    icon: MENU_ICON[item.id],
    danger: item.danger,
    disabled: item.disabled,
    separatorBefore: item.separatorBefore,
    shortcut: chordHint(item.command),
    onSelect: () => run(item.id, targets),
  });

  const desktop = inDesktopShell() && !!root;
  // The machine's clipboard counts only where the shell can write: a root on this machine.
  const source = pasteSource(clip, desktop ? held : { files: false, image: false });

  /** The context menu for the rows a right-click acts on — read again as it opens, in case a copy was just made in the file manager. */
  const menuFor = useCallback(
    (targets: readonly Target[]): MenuItem[] => {
      void refreshOsClipboard();
      return menuSpec({ targets, desktop, mutable, clipboard: source, reveal: REVEAL_LABEL, serve: !!onServe }).map((item) => toItem(item, targets));
    },
    // `toItem` and the verbs behind it are plain functions remade every
    // render; the menu is rebuilt on the facts that change what it offers —
    // the clipboard among them: *Cut A* then *Copy B* keeps `source` at
    // `tree`, and a menu keyed on `source` alone would still paste A.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [scope, id, root, siblingsOf, childrenOf, mutable, source, clip, onServe],
  );

  /** The tree's background menu and the toolbar's items: the root's verbs. */
  const rootItems = useMemo<MenuItem[]>(
    () => viewportMenuSpec({ desktop, mutable, clipboard: source, reveal: REVEAL_LABEL }).map((item) => toItem(item, [ROOT_TARGET])),
    // The same rule as `menuFor`: `toItem` is remade every render, and
    // `desktop` follows `root`; the clipboard is a fact the menu offers.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [root, mutable, source, clip, childrenOf],
  );

  const words = deleting ? deleteCopy({ targets: deleting.targets, counts: deleting.counts, disposal, gitRoot }) : null;

  const dialogs: ReactNode = (
    <>
      <ConfirmDialog
        open={deleting !== null}
        onClose={() => setDeleting(null)}
        danger={words?.danger ?? true}
        title={words?.title ?? ""}
        confirmLabel={words?.confirm ?? tr("ui-file-tree-mutations-delete-verb")}
        body={<p>{words?.body}</p>}
        onConfirm={() => void confirmDelete()}
      />
    </>
  );

  return {
    menuFor,
    rootItems,
    move,
    dialogs,
    busy,
    command,
    clip,
    renaming,
    startRename,
    cancelRename,
    checkRename,
    commitRename,
    creating,
    startCreate,
    cancelCreate,
    checkCreate,
    commitCreate,
  };
}
