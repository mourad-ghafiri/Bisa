/**
 * Everything a goal's agents — and you — actually made, as a tree.
 *
 * # Why this one fetches, when nothing else in `ui/` does
 *
 * The rest of the kit is paint: you hand it data, it draws. A tree cannot be,
 * because *lazy* is the whole design. Asking for the ceiling depth up front on
 * a goal that has been running for a week means walking a repository
 * checkout to serve a panel showing eight rows, so a directory is listed when
 * somebody opens it — and that makes the load state part of the tree's
 * structure rather than something a parent can hold. Three call sites each
 * re-deriving "which of my open folders still need fetching, and what does
 * one of them failing mean for the other two" is three chances to get it
 * wrong. The state machine itself is in `fileTreeModel.mjs`, where it is
 * testable; what is left here is effects and paint.
 *
 * # What is the tree's and what is the kit's
 *
 * The rows are drawn by `TreeList` (`ui/tree`), which owns the cursor's
 * arrows, Home/End, type-ahead, the indent guides, the scroll-to and the
 * drag projection. This file owns what a *file* is: listing, the rows'
 * content, what open means, the **selection** the verbs act on (a click
 * selects one row, Cmd-click toggles, Shift-click and Shift+arrows take a
 * range, Cmd+A takes all — `fileTreeModel.mjs`), the mutations
 * (`useTreeMutations`), and the keymap's commands in the `files` scope —
 * Enter renames, Space opens, the clipboard, delete, new — which the shell
 * delivers to whichever tree has focus through `explorerStore.ts`. A reveal
 * ("show this path") arrives the same way, addressed to the root. Escape,
 * anywhere in the tree, cancels a draft or a rename in progress, else clears
 * the selection.
 *
 * The **open folders** can be handed in and out (`openFolders`,
 * `onOpenFolders`): the kit keeps no store and knows no place, so a caller
 * that wants the tree to come back unfolded as it stood keeps them where it
 * keeps its view. The cursor and the selection stay the tree's own — they
 * are the moment's.
 *
 * A single click on a file opens it as a **preview** and a double-click keeps
 * it (`onOpenFile(path, { preview })`, ide/03) — the caller that owns the
 * tabs decides what that means.
 *
 * # The name
 *
 * Exported as `FileTreeView`, not `FileTree`, because `FileTree` is already
 * the wire type in `types.gen.ts` and every view that draws a tree is likely
 * to want both. Two things with one name in one import site is a rename
 * waiting to happen at the worst moment.
 */

import { useCallback, useEffect, useMemo, useReducer, useRef, useState, type KeyboardEvent, type MouseEvent } from "react";
import { api } from "../api";
import { useEngineEvents } from "../bus";
import { errorFields, log } from "../log";
import type { WorkbenchScope } from "../routeModel.mjs";
import type { Placement } from "../types";
import { Button } from "./Button";
import { ErrorNote } from "./Card";
import { EmptyState } from "./EmptyState";
import { SkeletonRows } from "./Skeleton";
import { cn } from "./cn";
import { ContextMenu } from "./ContextMenu";
import { pathDrag, type DragData } from "./dnd";
import { explorerFocused, registerExplorer, type ExplorerCommand } from "./explorerStore";
import { CopyText } from "./Field";
import { isCut } from "./fileClipboard.mjs";
import { FileView } from "./FileView";
import { FILE_KIND_ICON, ICON, fileIcon } from "./icons";
import { standingHint, standingMark, standingTone } from "./fileStandingModel.mjs";
import type { Standing, StandingTone } from "./fileStandingModel.mjs";
import { Menu } from "./Menu";
import { SectionHeader } from "./SectionHeader";
import { CURSOR_RING } from "./rings";
import { basename, renameSelection } from "./fileTreeMutations.mjs";
import { useTokenPx } from "./useTokenPx";
import { useTreeMutations } from "./useTreeMutations";
import { useWatchLease } from "./useWatchLease";
import { useReloadOnReconnect } from "./useReloadOnReconnect";
import { isHidden, onVisibilityChange } from "../shell/visibility";
import { TreeList, type DropPlan, type TreeRowState } from "./tree";
import { ROOT, activate, bodyRows, dirsToRefresh, formatSize, initialState, loadKey, openFolderPaths, parentPath, pendingLoads, reduce, targetsOf, type Target, type TreeRow, withDraft } from "./fileTreeModel.mjs";
import { t as tr } from "../i18n/l10n.mjs";
import { rich } from "../i18n/rich";

/**
 * One row per line, so the flat model's index is also a pixel offset. The
 * height is the theme's `--spacing-row-sm`, read live (`useTokenPx`), so the
 * rows grow with the type scale instead of clipping their labels; this is
 * the last-resort value for the frame before the stylesheet has resolved.
 */
const ROW_HEIGHT_FALLBACK = 28;
/** Per nesting level. Enough to read the shape, small enough for a 300px pane. */
const INDENT = 14;

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** The chevron column: disclosure for a directory, blank for a file. */
function Twisty({ dir, expanded }: { dir: boolean; expanded: boolean }) {
  if (!dir) return <span className="w-3 shrink-0" aria-hidden />;
  const Glyph = expanded ? ICON.expanded : ICON.collapsed;
  return <Glyph size={12} aria-hidden className="shrink-0 text-text-dim" />;
}

/**
 * The name field of a row being renamed in place. Enter commits, Escape
 * cancels, leaving the field cancels; the stem is selected so typing replaces
 * the name and keeps the extension. Its keys stop here: the tree's own
 * handler and the keymap must not read a rename's Enter as a command.
 */
function RenameField({
  name,
  error,
  onChange,
  onCommit,
  onCancel,
}: {
  name: string;
  error: string | null;
  onChange: (value: string) => void;
  onCommit: (value: string) => void;
  onCancel: () => void;
}) {
  const input = useRef<HTMLInputElement>(null);
  // The field mounts after the menu that opened it has left (a menu runs its
  // chosen action in its last word, `ui/Menu.tsx`), so one focus sticks.
  useEffect(() => {
    const el = input.current;
    if (!el) return;
    el.focus();
    const [start, end] = renameSelection(name);
    el.setSelectionRange(start, end);
  }, [name]);
  return (
    <span className="relative min-w-0 flex-1">
      <input
        ref={input}
        defaultValue={name}
        aria-label={name ? tr("ui-file-tree-new-name", { name }) : tr("ui-file-tree-name")}
        aria-invalid={error ? true : undefined}
        spellCheck={false}
        className={cn(
          "h-5 w-full rounded-control border bg-surface px-1 font-mono text-2xs text-text outline-none",
          error ? "border-danger" : "border-accent/60",
        )}
        onChange={(e) => onChange(e.currentTarget.value)}
        onPointerDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === "Enter") {
            e.preventDefault();
            onCommit(e.currentTarget.value);
          } else if (e.key === "Escape") {
            e.preventDefault();
            onCancel();
          }
        }}
        // Judged a frame later, so a focus the field wins straight back — the
        // row re-rendering under it — is not a cancel.
        onBlur={() => {
          requestAnimationFrame(() => {
            if (document.activeElement !== input.current) onCancel();
          });
        }}
      />
      {error && (
        <span role="alert" className="absolute left-0 top-full z-10 mt-0.5 rounded-control border border-border bg-surface px-1.5 py-0.5 text-2xs text-danger shadow-sm">
          {error}
        </span>
      )}
    </span>
  );
}

/** The name's ink per standing tone — the theme's three status roles. */
const TONE_TEXT: Record<StandingTone, string> = { ok: "text-ok", warn: "text-warn", danger: "text-danger" };

export function FileTreeView({
  scope,
  id,
  live = false,
  goal,
  showRoot = false,
  emptyHint,
  onOpenFile,
  openPath,
  dirtyPaths,
  standings,
  title,
  fill = false,
  mutable = false,
  onDeleted,
  onServeFolder,
  onCreated,
  onSearch,
  openFolders,
  onOpenFolders,
  className,
}: {
  scope: WorkbenchScope;
  /** The goal, project or workstream id — whichever `scope` names. */
  id: string;
  /**
   * Something is running under this scope. Polls at 2s while true and not at
   * all while false: a tree nobody is writing into does not need a heartbeat.
   */
  live?: boolean;
  /**
   * Refresh on engine frames for this goal. Always pass one — omitting it
   * subscribes to every frame in the workspace, so an unrelated goal's tool
   * call would refetch this tree.
   */
  goal?: string;
  /** Show the absolute root, copyable. For surfaces that don't show it already. */
  showRoot?: boolean;
  emptyHint?: string;
  /**
   * Opening a file is the caller's business.
   *
   * With it, the tree reports the click and renders **no** preview of its own —
   * which is how the workbench turns a click into a document tab: a single
   * click asks for a `preview`, a double-click or the `open_entry` command
   * for a kept tab. Without it the tree shows the file inline, which is what
   * the goal inspector wants: a 300px pane has nowhere else to put it.
   */
  onOpenFile?: (path: string, opts: { preview: boolean }) => void;
  /** Which row reads as open, when the caller owns that. */
  openPath?: string | null;
  /** Files with unsaved changes in an open document, for the dot beside the name. */
  dirtyPaths?: ReadonlySet<string>;
  /**
   * What git says about a path — a file's kind, a folder holding one —
   * folded by the kit's `fileStandingModel`: the name is painted in the
   * kind's tone, a file wears its mark at the right. Absent where there is
   * no repository to ask.
   */
  standings?: ReadonlyMap<string, Standing>;
  /** Draw the toolbar as a section header with this title (the Files occupant does); absent, a plain toolbar row. */
  title?: string;
  /** Fill the height it is given, rather than capping at a preview's worth. */
  fill?: boolean;
  /** Offer new / rename / move / delete / clipboard, through the engine. Only where the root is writable. */
  mutable?: boolean;
  /** Paths that were removed — the open documents on them should close, together. */
  onDeleted?: (paths: string[]) => void;
  /** Serve a folder on this machine and open it in the embedded browser (ide/18); absent where nothing can. */
  onServeFolder?: (path: string) => void;
  /**
   * An entry was created through *New file* / *New folder*. The Files
   * occupant opens a new file as a kept document; a folder is only revealed.
   */
  onCreated?: (path: string, kind: "file" | "dir") => void;
  /** Offer a search button in the toolbar; the caller draws the box (the Files occupant does). */
  onSearch?: () => void;
  /**
   * The folders this root was left unfolded with, from a caller that keeps
   * them. Read when the tree starts and when the root changes — never
   * followed after: from then on the tree's own state is the truth.
   */
  openFolders?: readonly string[];
  /** The open folders changed: what a caller that keeps them is handed. */
  onOpenFolders?: (paths: string[]) => void;
  className?: string;
}) {
  const [state, dispatch] = useReducer(reduce, openFolders, initialState);
  // The caller's two hands, read where they are used: a fresh list or a
  // fresh function per render must cost neither a reset nor a report.
  const folders = useRef({ kept: openFolders, tell: onOpenFolders });
  folders.current = { kept: openFolders, tell: onOpenFolders };
  const rowHeight = useTokenPx("--spacing-row-sm", ROW_HEIGHT_FALLBACK);
  const [nonce, setNonce] = useState(0);
  /** Only consulted when the *root* listing fails; see the effect below. */
  const [placement, setPlacement] = useState<Placement | null>(null);
  const rootKey = `${scope}:${id}`;

  /**
   * In-flight listings by path. Held in a ref rather than in the reducer
   * because an AbortController is not state anybody renders, and putting it
   * there would make every `loaded` action carry a value the tests have to
   * invent.
   */
  const inflight = useRef(new Map<string, AbortController>());

  // A different scope or id is a different tree. Both the cache and anything
  // still in the air belong to the old one; it starts unfolded as that root
  // was left.
  useEffect(() => {
    dispatch({ type: "reset", open: folders.current.kept });
    setPlacement(null);
    const airborne = inflight.current;
    return () => {
      for (const ac of airborne.values()) ac.abort();
      airborne.clear();
    };
  }, [scope, id]);

  // The open folders are handed out as they change. After a change of root
  // the first report is that root's own kept list, read back: the caller's
  // memory keeps a value it already holds as no change at all.
  const open = state.open;
  useEffect(() => {
    folders.current.tell?.(openFolderPaths(open));
  }, [open]);

  const refresh = useCallback(() => {
    // Anything in the air was asked about the previous state of the disk.
    for (const ac of inflight.current.values()) ac.abort();
    inflight.current.clear();
    dispatch({ type: "invalidate" });
    setNonce((n) => n + 1);
  }, []);

  // The explorer holds the watcher's lease while it shows a writable root, so
  // an agent's write appears here without anybody clicking refresh.
  useWatchLease(scope, id, !!mutable);
  // What moved on disk while the node was away was said by no frame.
  useReloadOnReconnect(refresh);
  const childrenOf = useCallback((dir: string) => (state.dirs[dir]?.entries ?? []).map((e: { name: string }) => e.name), [state.dirs]);
  const siblingsOf = useCallback((path: string) => childrenOf(parentPath(path)), [childrenOf]);
  const gitRoot = (state.dirs[""]?.entries ?? []).some((e: { name: string }) => e.name === ".git" || e.name === ".gitignore");
  const mutations = useTreeMutations({ scope, id, rootKey, root: state.root, gitRoot, mutable: !!mutable, siblingsOf, childrenOf, refresh, onDeleted, onServe: onServeFolder, onCreated });

  const pending = pendingLoads(state);
  // The effect must not re-run on every render, and `pending` is a fresh array
  // each time. NUL as the separator because a path may contain a space, and
  // `["a b"]` and `["a", "b"]` joining to one key would skip a load. The count
  // is part of the key because the root is the empty string: `[]` and `[""]`
  // both join to `""`, and a refresh that only re-pended the root produced a
  // key equal to the one before it — so the effect never ran and the tree sat
  // on "listing…" forever after every create, rename or delete. The nonce is
  // in the dependencies for the same reason: a refresh must always load.
  const pendingKey = loadKey(pending);

  useEffect(() => {
    for (const path of pending) {
      if (inflight.current.has(path)) continue;
      const ac = new AbortController();
      inflight.current.set(path, ac);
      dispatch({ type: "loading", path });
      // Depth 1, always. The route clamps at 8, and asking for it would walk
      // a whole checkout to draw the handful of rows that are on screen.
      api
        .tree(scope, id, path || undefined, 1, ac.signal)
        .then((tree) =>
          dispatch({
            type: "loaded",
            path,
            entries: tree.entries,
            truncated: tree.truncated,
            root: tree.root,
          }),
        )
        .catch((e) => {
          if (!ac.signal.aborted) dispatch({ type: "failed", path, error: message(e) });
        })
        .finally(() => {
          // A refresh may already have replaced this entry; deleting blindly
          // would drop the *new* controller and let a duplicate through.
          if (inflight.current.get(path) === ac) inflight.current.delete(path);
        });
    }
    // `pending` is keyed by `pendingKey` above: the array itself is fresh on every render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pendingKey, nonce, scope, id]);

  const rootDir = state.dirs[ROOT];
  const rootFailed = rootDir?.status === "error";

  /**
   * A root that will not list has two very different causes, and only one of
   * them is a fault. `/placement` is the route that tells them apart: a folder
   * that has not been created yet answers `exists: false` rather than failing.
   */
  useEffect(() => {
    if (!rootFailed || placement) return;
    const ac = new AbortController();
    api
      .placement(scope, id, ac.signal)
      .then((p) => setPlacement(p))
      .catch((e: unknown) => {
        // The tree already has an error to show; a second one adds nothing.
        if (ac.signal.aborted) return;
        log.debug("files", "the root's placement could not be read; the tree's own error stands", { scope, id, ...errorFields(e) });
      });
    return () => ac.abort();
  }, [rootFailed, placement, scope, id]);

  // The watcher's frames name what moved: only the listings they
  // touched are re-read, coalesced so a save that writes a temp file and
  // renames it is one refresh, not two. The coarser engine moments — a run
  // ending, a result taken, a workstream committing — still refresh the whole
  // tree, for a root nobody holds a lease on.
  const touched = useRef(new Set<string>());
  const flush = useRef<number | null>(null);
  useEffect(
    () => () => {
      if (flush.current !== null) window.clearTimeout(flush.current);
    },
    [],
  );
  useEngineEvents((e) => {
    const p = e.payload;
    if (p.type === "file_changed") {
      if (p.scope !== scope || p.id !== id) return;
      for (const d of dirsToRefresh(p)) touched.current.add(d);
      if (flush.current === null) {
        flush.current = window.setTimeout(() => {
          flush.current = null;
          const paths = [...touched.current];
          touched.current.clear();
          for (const ac of paths.map((d) => inflight.current.get(d)).filter(Boolean)) ac?.abort();
          for (const d of paths) inflight.current.delete(d);
          dispatch({ type: "refresh_dirs", paths });
          setNonce((n) => n + 1);
        }, 100);
      }
      return;
    }
    const moved =
      p.type === "execution_ended" ||
      p.type === "result_accepted" ||
      p.type === "workstream_committed" ||
      p.type === "workstream_changed" ||
      (p.type === "session" && p.event.tier === "lifecycle" && p.event.event.type === "ended");
    if (moved) refresh();
  }, goal);

  useEffect(() => {
    if (!live) return;
    // A hidden window does not poll; becoming visible refreshes at once.
    const t = window.setInterval(() => {
      if (!isHidden()) refresh();
    }, 2000);
    const off = onVisibilityChange(() => {
      if (!isHidden()) refresh();
    });
    return () => {
      window.clearInterval(t);
      off();
    };
  }, [live, refresh]);

  // A pending inline create — or a picture pasted from the clipboard — opens
  // its target folder and shows a draft row in it; the person types the name
  // there instead of in a modal. The list mounts for the draft even when the
  // root has nothing else — that is the whole point of a draft on an empty
  // folder.
  const creating = mutations.creating;
  useEffect(() => {
    if (creating?.dir) dispatch({ type: "expand", path: creating.dir });
  }, [creating?.dir]);
  const rows = withDraft(bodyRows(state), creating ? { dir: creating.dir, kind: creating.kind } : null);

  // ---- the keymap's door -----------------------------------------------------

  const selected = useMemo(() => new Set(state.selection), [state.selection]);
  /** What a verb acts on when asked at `at`: the selection when it is in it, else that row alone. */
  const targets = (at: string | null): Target[] => targetsOf(rows, state.selection, at);

  /** Open a file: for the caller as a preview or a kept tab, else shown inline here. */
  const openEntry = (path: string, preview: boolean) => {
    if (onOpenFile) onOpenFile(path, { preview });
    else dispatch({ type: "show", path });
  };

  const onCommand = (cmd: ExplorerCommand) => {
    if (cmd === "select_all_entries") {
      dispatch({ type: "select_all" });
      return;
    }
    if (mutations.command(cmd, targets(state.cursor))) return;
    // `open_entry`: the tree's own — a folder toggles, a file opens, kept.
    const a = activate(rows, state.cursor);
    if (!a) return;
    if (a.kind === "open") openEntry(a.path, false);
    else dispatch({ type: "toggle", path: a.path });
  };

  /** A reveal waits for the row to exist — its folders may still be listing. */
  const [revealing, setRevealing] = useState<string | null>(null);
  const [scrollTo, setScrollTo] = useState<{ id: string | null; nonce: number }>({ id: null, nonce: 0 });
  const reveal = (path: string) => {
    dispatch({ type: "reveal", path });
    setRevealing(path);
  };
  useEffect(() => {
    if (revealing === null) return;
    if (rows.some((r) => r.kind === "entry" && r.path === revealing)) {
      setScrollTo((prev) => ({ id: revealing, nonce: prev.nonce + 1 }));
      setRevealing(null);
      return;
    }
    // A folder above it failed to list: nothing more will appear. Stop waiting.
    for (let p = parentPath(revealing); ; p = parentPath(p)) {
      if (state.dirs[p]?.status === "error") {
        setRevealing(null);
        return;
      }
      if (p === ROOT) return;
    }
  }, [revealing, rows, state.dirs]);

  // The latest handlers, so the registration below never goes stale without
  // re-registering on every render.
  const latest = useRef({ onCommand, reveal });
  latest.current = { onCommand, reveal };
  useEffect(
    () =>
      registerExplorer(rootKey, {
        command: (cmd) => latest.current.onCommand(cmd),
        reveal: (path) => latest.current.reveal(path),
      }),
    [rootKey],
  );

  /** Keys the generic tree had no answer for: Escape cancels what is half-typed, else clears the selection. */
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key !== "Escape") return;
    if (mutations.creating) {
      e.preventDefault();
      mutations.cancelCreate();
    } else if (mutations.renaming) {
      e.preventDefault();
      mutations.cancelRename();
    } else if (state.selection.length > 0) {
      dispatch({ type: "select_clear" });
    }
  };

  // ---- drag: rows onto a folder, or onto the root -----------------------------

  const drag = mutable
    ? {
        // A selected row drags the whole selection with it; any other row
        // drags alone. Read at render, so a Shift-click that starts a drag
        // drags the selection as it stood.
        data: (row: TreeRow): DragData | null =>
          row.kind === "entry" && mutations.renaming?.path !== row.path
            ? pathDrag({ scope, id, path: row.path, dir: row.entry.dir, paths: targets(row.path).map((t) => t.path) })
            : null,
        actives: (data: DragData) => (data.type === "path" ? data.paths : []),
        // Only a folder takes a drop; a file's row means its folder.
        canNest: (_: DragData, target: TreeRow) => target.kind === "entry" && target.entry.dir,
        // Entries sort by name, so where among the siblings is not a fact
        // the drop can set: the indicator is the folder, never a gap.
        indicator: "parent" as const,
        onDrop: (data: DragData, plan: DropPlan) => {
          if (data.type !== "path" || data.scope !== scope || data.id !== id || plan.noop) return;
          void mutations.move(data.paths, plan.parent ?? ROOT);
        },
      }
    : undefined;

  const renderRow = (row: TreeRow, rs: TreeRowState) => {
    if (row.kind === "draft") {
      const DraftGlyph = row.entryKind === "dir" ? ICON.folder : row.entryKind === "image" ? ICON.image : ICON.file;
      return (
        <div style={{ marginLeft: rs.indent }} className="mr-1 flex h-full items-center gap-1.5 rounded-control pr-2 text-2xs">
          <span className="w-3 shrink-0" aria-hidden />
          <DraftGlyph size={12} aria-hidden className="shrink-0 text-text-dim" />
          <RenameField
            name={mutations.creating?.name ?? ""}
            error={mutations.creating?.error ?? null}
            onChange={mutations.checkCreate}
            onCommit={(v) => void mutations.commitCreate(v)}
            onCancel={mutations.cancelCreate}
          />
        </div>
      );
    }

    if (row.kind !== "entry") {
      const text =
        row.kind === "loading"
          ? tr("ui-file-tree-listing")
          : row.kind === "empty"
            ? tr("ui-file-tree-empty")
            : row.kind === "truncated"
              ? tr("ui-file-tree-more-listing-hit-size-bound-so")
              : (row.error ?? tr("ui-file-tree-could-read"));
      return (
        <div style={{ marginLeft: rs.indent }} className={cn("mr-1 flex h-full items-center gap-1.5 rounded-control pr-2 text-2xs", row.kind === "error" ? "text-danger" : "text-text-dim")}>
          <span className="w-3 shrink-0" aria-hidden />
          {row.kind === "error" && <ICON.danger size={11} aria-hidden className="shrink-0" />}
          {row.kind === "truncated" && <ICON.warn size={11} aria-hidden className="shrink-0" />}
          <span className="min-w-0 truncate">{text}</span>
        </div>
      );
    }

    const { entry, expanded, path } = row;
    const opened = (openPath ?? state.shown) === path;
    const renaming = mutations.renaming?.path === path;
    const cut = isCut(mutations.clip, path);
    const dirty = dirtyPaths?.has(path) ?? false;
    const standing = standings?.get(path) ?? null;
    const tone = standingTone(standing);
    const mark = standingMark(standing);
    const hint = standingHint(standing);
    // A folder that is open wears the open folder, except where the kind is a
    // domain concept — a workstream stays a workstream whether or not you are
    // looking inside it.
    const Glyph = entry.dir && entry.kind === "dir" && expanded ? ICON.folderOpen : entry.kind === "file" ? fileIcon(entry.name) : FILE_KIND_ICON[entry.kind];
    return (
      // The menu acts on what a right-click here means: the selection when
      // this row is in it, this row alone otherwise.
      <ContextMenu items={mutations.menuFor(targets(path))} className="block h-full">
        <div
          style={{ marginLeft: rs.indent }}
          onClick={(e: MouseEvent) => {
            if (renaming) return;
            // Cmd toggles the row in the selection, Shift takes the range from
            // the anchor; neither opens anything. A plain click selects the row
            // alone (the cursor follows, so arrowing carries on from here) and
            // opens a file as a preview or toggles a folder.
            if (e.metaKey || e.ctrlKey) {
              dispatch({ type: "select_toggle", path });
              return;
            }
            if (e.shiftKey) {
              dispatch({ type: "select_range", path });
              return;
            }
            dispatch({ type: "select", path });
            if (entry.dir) dispatch({ type: "toggle", path });
            else openEntry(path, true);
          }}
          onDoubleClick={() => {
            // A double-click keeps the tab a single click previewed. A folder
            // has had two toggles by now, which is what every file manager does.
            if (!renaming && !entry.dir) openEntry(path, false);
          }}
          onContextMenu={() => {
            // A right-click on a row outside the selection makes it the one
            // selected row; on a selected row it leaves the selection alone.
            if (!selected.has(path)) dispatch({ type: "select", path });
          }}
          data-drop-inside={rs.dropInside || undefined}
          className={cn(
            "anim tree-nest mr-1 flex h-full cursor-default items-center gap-1.5 rounded-control pr-2 text-2xs",
            // One vocabulary: a selected row wears the accent wash, the open
            // document its ink as well, the cursor a ring — three facts, three marks.
            opened ? "bg-accent-soft text-accent-ink" : rs.selected ? "bg-accent-soft" : "hover:bg-surface-2",
            rs.cursor && CURSOR_RING,
            // The folder a drop would join: the wash here, the ring through `.tree-nest`, both easing.
            rs.dropInside && "bg-accent-soft/40",
            // Ignored by the root's rules: dimmed, never hidden — a build output
            // you cannot see is one you cannot delete. A cut row likewise: it is
            // still here until the paste.
            (entry.ignored || cut) && "opacity-60",
          )}
          title={cut ? tr("ui-file-tree-cut-paste-move", { entry: entry.name }) : entry.ignored ? tr("ui-file-tree-ignored-gitignore", { entry: entry.name }) : hint ? `${entry.name} — ${hint}` : undefined}
        >
          <Twisty dir={entry.dir} expanded={expanded} />
          <Glyph size={12} aria-hidden className="shrink-0 text-text-dim" />
          {renaming ? (
            <RenameField
              name={basename(path)}
              error={mutations.renaming?.error ?? null}
              onChange={mutations.checkRename}
              onCommit={(v) => void mutations.commitRename(v)}
              onCancel={mutations.cancelRename}
            />
          ) : (
            // git's standing paints the name — new in `ok`, moved in `warn`, a
            // conflict in `danger` — and keeps it painted on a selected or open
            // row, the way a changed tab stays coloured when it is active.
            <span className={cn("min-w-0 flex-1 truncate font-mono", tone && TONE_TEXT[tone])}>{entry.name}</span>
          )}
          {mark && !renaming && (
            <span aria-hidden className={cn("shrink-0 font-mono", tone && TONE_TEXT[tone])}>
              {mark}
            </span>
          )}
          {dirty && !renaming && (
            <span aria-label={tr("ui-file-tree-unsaved-changes")} className="shrink-0 text-accent">
              ●
            </span>
          )}
          {/* A symlink is listed and never followed, so the row has to say so —
              otherwise an unexpandable directory reads as a broken one. */}
          {entry.symlink && (
            <span className="shrink-0 text-text-dim" title={tr("ui-file-tree-link-listed-never-followed")}>
              ↗
            </span>
          )}
          {!entry.dir && !renaming && <span className="tnum shrink-0 text-text-dim">{formatSize(entry.size)}</span>}
        </div>
      </ContextMenu>
    );
  };

  /**
   * The root gets a richer treatment than the one-line notice row a nested
   * directory gets, because at the root the notice *is* the panel: there is
   * no surrounding listing to give it context, and "empty" alone leaves the
   * reader unsure whether the tree failed, is still loading, or genuinely has
   * nothing in it. The error sits above the rows (a draft may still be under
   * it); the spinner and the empty hint stand in for the rows when there are
   * none — a draft on an empty root is a row, so it shows.
   */
  const rootError = rootFailed ? (
    <div className="p-2">
      <ErrorNote error={rootDir?.error ?? tr("ui-file-tree-folder-could-listed")} retry={refresh} />
      {placement && !placement.exists && (
        <p className="mt-1 px-1 text-2xs text-text-dim">
          {rich("ui-file-tree-nothing-at-yet", { path: <span className="font-mono">{placement.path}</span> })}
        </p>
      )}
    </div>
  ) : null;
  // A list that is empty says so the kit's way, with the one thing to do about
  // it; a list that is still listing shows the rows it is about to have.
  const rootEmpty = rootFailed ? null : rootDir?.status === "ready" ? (
    <EmptyState
      icon={ICON.folder}
      title={tr("ui-file-tree-nothing-here-yet")}
      hint={emptyHint ?? tr("ui-file-tree-anything-agent-writes-shows-up-lands")}
      action={
        mutable ? (
          <Button size="sm" variant="ghost" onClick={() => mutations.startCreate("file", ROOT)}>{tr("ui-file-tree-new-file")}</Button>
        ) : (
          <Button size="sm" variant="ghost" onClick={refresh}>{tr("ui-file-tree-refresh")}</Button>
        )
      }
    />
  ) : (
    <SkeletonRows rows={6} className="px-2 py-1" />
  );

  const toolButton = "anim flex h-6 w-6 items-center justify-center rounded-control border border-border text-text-dim hover:bg-surface-2 hover:text-text";
  const anyOpen = Object.keys(state.open).length > 0;
  const tools = (
    <>
      {onSearch && (
        <button type="button" onClick={onSearch} aria-label={tr("ui-file-tree-search-files-name-content")} title={tr("ui-file-tree-search-files")} className={toolButton}>
          <ICON.search size={12} aria-hidden />
        </button>
      )}
      {mutable && (
        <Menu
          label={tr("ui-file-tree-actions-root")}
          trigger={
            <span className={toolButton}>
              <ICON.add size={12} aria-hidden />
            </span>
          }
          items={mutations.rootItems}
        />
      )}
      {live && (
        <span className="text-2xs text-text-dim" title={tr("ui-file-tree-following-live-run")}>{tr("ui-file-tree-following")}</span>
      )}
      <button type="button" onClick={() => dispatch({ type: "collapse_all" })} disabled={!anyOpen} aria-label={tr("ui-file-tree-collapse-every-folder")} title={tr("ui-file-tree-collapse-all")} className={cn(toolButton, "disabled:opacity-45")}>
        <ICON.collapseAll size={12} aria-hidden />
      </button>
      <button type="button" onClick={refresh} aria-label={tr("ui-file-tree-refresh-listing")} className={toolButton}>
        <ICON.refresh size={12} aria-hidden />
      </button>
    </>
  );

  return (
    // `data-files-tree`: the keymap's `files` scope is live while focus is in here.
    <div data-files-tree className={cn("flex min-w-0 flex-col", fill && "min-h-0 flex-1", className)}>
      {title ? (
        <SectionHeader title={title} action={<span className="flex items-center gap-1">{tools}</span>} />
      ) : (
        <div className="mb-1 flex items-center gap-2">
          {showRoot && state.root && <CopyText value={state.root} />}
          <span className="flex-1" />
          {tools}
        </div>
      )}

      {mutable && mutations.dialogs}
      {/* The background's menu is the root's — new, paste, reveal, refresh —
          and Radix draws it only where no row's menu claimed the click first. */}
      <ContextMenu items={mutations.rootItems} className={cn("block", fill && "flex min-h-0 flex-1 flex-col")}>
        <TreeList
          rows={rows}
          rowHeight={rowHeight}
          indent={INDENT}
          cursor={state.cursor}
          selected={selected}
          onCursor={(path, { extend }) => dispatch({ type: "cursor", path, extend })}
          onAction={(a) => {
            if (a.kind === "expandAll") for (const path of a.ids) dispatch({ type: "expand", path });
            else dispatch({ type: a.kind, path: a.id });
          }}
          onKeyDown={onKeyDown}
          onFocus={() => explorerFocused(rootKey)}
          onBlur={() => explorerFocused(null)}
          render={renderRow}
          drag={drag}
          label={tr("ui-file-tree-files")}
          busy={rootDir?.status === "loading"}
          scrollTo={scrollTo}
          beforeRows={rootError}
          empty={rootEmpty}
          fill={fill}
          className={cn("rounded-control border border-border bg-surface-2 py-1 focus-visible:border-accent/60", fill && "min-h-0 flex-1 overflow-hidden")}
        />
      </ContextMenu>

      {!onOpenFile && state.shown && (
        <FileView
          className="mt-2 max-h-80"
          scope={scope}
          id={id}
          path={state.shown}
          nonce={nonce}
          actions={
            <button type="button" onClick={() => dispatch({ type: "hide" })} aria-label={tr("ui-file-tree-close-file")} className="anim shrink-0 rounded px-1 text-text-dim hover:text-text">
              <ICON.close size={12} aria-hidden />
            </button>
          }
        />
      )}
    </div>
  );
}
