/**
 * The file tree's state and arithmetic, with no React and no DOM in it.
 *
 * Plain `.mjs` with a `.d.mts` beside it, for the same reason
 * `terminal/xtermTheme.mjs` is: `node --test` imports the real module rather
 * than a transcription of it, and there is no jsdom in this repo to render a
 * component into. Everything a tree can get *wrong* — which node a path is a
 * child of, which directories still need fetching, what a key press means at
 * the cursor — is decided here, so all of it is reachable by a test. The
 * component below is then only paint and effects.
 *
 * # Paths
 *
 * Every path in this module is `FileEntry.path`: `/`-separated and relative
 * to the *scope's root*, never to whatever sub-path was listed. That is the
 * whole reason the state can be one flat map keyed by path — an entry's path
 * is a stable global name for it, so the same string identifies a row, its
 * cached listing, its open/closed bit and the next request's `?path=`. The
 * root itself is the empty string, which is why {@link ROOT} exists rather
 * than a literal `""` scattered through the file: `""` is falsy, and every
 * `if (path)` written by accident would silently mean "not the root".
 *
 * # Why per-directory status, not one loading flag
 *
 * One unreadable directory must not blank the tree. A permissions error three
 * levels down is a fact about *that* directory, and the siblings above it are
 * still perfectly good listings; a single top-level `error` state would throw
 * them away and make the whole panel say nothing. So status lives per node
 * and {@link visibleRows} renders a failed directory as one row in place of
 * its children.
 */

/** The scope's root. Not `""` inline: see the note above about falsiness. */
export const ROOT = "";

/**
 * `base/name`, with the root's empty base handled.
 *
 * Joining onto the root with a naive template gives `"/name"`, which the node
 * reads as absolute and refuses — a bug that only appears at the top level,
 * which is exactly where a hand test is least likely to look.
 */
export function joinPath(base, name) {
  if (!base) return name;
  return `${base.replace(/\/+$/, "")}/${name}`;
}

/** The path of the directory that lists this entry; `ROOT` for a top-level one. */
export function parentPath(path) {
  const cut = path.lastIndexOf("/");
  return cut === -1 ? ROOT : path.slice(0, cut);
}

/** `path` is listed directly by `parent` — not merely somewhere beneath it. */
export function isImmediateChild(parent, path) {
  return Boolean(path) && parentPath(path) === parent;
}

/** `path` is anywhere under `parent`. The root contains everything but itself. */
export function isDescendant(parent, path) {
  if (!path) return false;
  if (parent === ROOT) return true;
  return path.startsWith(`${parent}/`);
}

const UNITS = ["B", "KB", "MB", "GB", "TB"];

/**
 * A byte count a person can read at a glance.
 *
 * Directories carry no size and the node sends `null` for them, so an absent
 * size is the empty string rather than `0 B` — a folder claiming to be zero
 * bytes reads as an empty folder, which is a different and wrong fact.
 *
 * One decimal below ten and none above, so a column of sizes stays the same
 * width and `1.5 KB` does not sit next to `1023.4 KB`.
 */
export function formatSize(bytes) {
  if (typeof bytes !== "number" || !Number.isFinite(bytes) || bytes < 0) return "";
  let n = bytes;
  let u = 0;
  while (n >= 1024 && u < UNITS.length - 1) {
    n /= 1024;
    u++;
  }
  return `${n.toFixed(u === 0 || n >= 10 ? 0 : 1)} ${UNITS[u]}`;
}

/**
 * Directories first, then by name.
 *
 * The node does not promise an order, and an order that changes between two
 * listings of the same directory makes a refresh look like the contents moved.
 * `numeric` so `10.log` sorts after `9.log`; `sensitivity: "base"` so `README`
 * and `readme` do not end up at opposite ends of the list.
 */
export function sortEntries(entries) {
  return [...entries].sort((a, b) => {
    if (a.dir !== b.dir) return a.dir ? -1 : 1;
    const byName = a.name.localeCompare(b.name, undefined, {
      numeric: true,
      sensitivity: "base",
    });
    // A tie on a case-insensitive compare is still two different rows; the
    // path settles it the same way whatever order they came in — uppercase
    // first, so `README` stands before `readme` on every refresh.
    return byName !== 0 ? byName : a.path.localeCompare(b.path, undefined, { caseFirst: "upper" });
  });
}

/**
 * # Cursor, selection, shown
 *
 * Three facts, kept apart because they move apart. The **cursor** is where the
 * keyboard is — one row. The **selection** is what a verb acts on — one row
 * or many (Cmd-click toggles, Shift-click and Shift+arrows take a range from
 * the **anchor**, Cmd+A takes every visible row); a plain click or arrow
 * collapses it to that row, the way every file manager does. **Shown** is the
 * file the tree previews inline when nobody else opens files for it (the goal
 * inspector) — the caller that does open files says which row reads as open.
 *
 * The **open folders** are the one part a caller may hand in: a tree that
 * comes back — after leaving its screen, after a restart — unfolds as it
 * stood (`openFolderPaths` hands them out). A folder that is no longer there
 * is never listed: only the visible spine is walked (`pendingLoads`).
 * @param {readonly string[] | null} [open] the folders to start unfolded
 */
export function initialState(open) {
  const unfolded = {};
  for (const path of open ?? []) if (typeof path === "string" && path) unfolded[path] = true;
  return { dirs: {}, open: unfolded, cursor: null, selection: [], anchor: null, shown: null, root: null };
}

/**
 * The folders that are unfolded, as a caller keeps them: their paths, in
 * the order they were opened.
 * @param {Readonly<Record<string, true>>} open a state's `open`
 * @returns {string[]}
 */
export function openFolderPaths(open) {
  return Object.keys(open ?? {});
}

/** The paths of the entry rows on screen, in order. */
function visiblePaths(state) {
  return visibleRows(state).filter((r) => r.kind === "entry").map((r) => r.path);
}

/**
 * The selection after the rows changed: what is no longer visible leaves it.
 * A selected row inside a folder that folded, or in a listing that failed,
 * would otherwise be acted on unseen.
 */
function pruned(state) {
  if (state.selection.length === 0 && state.anchor === null) return state;
  const visible = new Set(visiblePaths(state));
  const selection = state.selection.filter((p) => visible.has(p));
  const anchor = state.anchor !== null && visible.has(state.anchor) ? state.anchor : null;
  return selection.length === state.selection.length && anchor === state.anchor ? state : { ...state, selection, anchor };
}

/** The selection set to `path` alone, the anchor and the cursor with it. */
function selectOne(state, path) {
  const same = state.cursor === path && state.anchor === path && state.selection.length === 1 && state.selection[0] === path;
  return same ? state : { ...state, cursor: path, anchor: path, selection: [path] };
}

/**
 * The entry paths between two rows, inclusive, in display order — a range
 * runs over what is on screen, so a folded folder counts as one row and its
 * hidden children are not swept in.
 * @param {readonly object[]} rows the output of {@link visibleRows}
 * @param {string | null} a
 * @param {string} b
 */
export function rangeBetween(rows, a, b) {
  const paths = rows.filter((r) => r.kind === "entry").map((r) => r.path);
  const i = a === null ? -1 : paths.indexOf(a);
  const j = paths.indexOf(b);
  if (j === -1) return [];
  if (i === -1) return [b];
  return paths.slice(Math.min(i, j), Math.max(i, j) + 1);
}

/**
 * What a verb acts on when asked at the row `at`: the whole selection when
 * `at` is in it, else `at` alone — and never a row whose folder is already
 * in the list, because deleting or moving the folder takes its children with
 * it and a second request for them would fail on a path that is gone.
 * @param {readonly object[]} rows
 * @param {readonly string[]} selection
 * @param {string | null} at
 * @returns {{path: string, dir: boolean}[]}
 */
export function targetsOf(rows, selection, at) {
  if (at === null) return [];
  const entries = new Map(rows.filter((r) => r.kind === "entry").map((r) => [r.path, r.entry.dir]));
  if (!entries.has(at)) return [];
  const chosen = selection.includes(at) ? selection.filter((p) => entries.has(p)) : [at];
  const folders = chosen.filter((p) => entries.get(p));
  return chosen.filter((p) => !folders.some((f) => f !== p && isDescendant(f, p))).map((path) => ({ path, dir: Boolean(entries.get(path)) }));
}

/** A directory nobody has asked for yet. */
function idleDir() {
  return { status: "idle", entries: [], truncated: false, error: null };
}

function withDir(state, path, patch) {
  const prev = state.dirs[path] ?? idleDir();
  return { ...state, dirs: { ...state.dirs, [path]: { ...prev, ...patch } } };
}

/**
 * @param state {object}
 * @param action {object}
 */
export function reduce(state, action) {
  switch (action.type) {
    /** A different scope or id entirely: nothing cached about the old one applies — it starts as that root was left unfolded. */
    case "reset":
      return initialState(action.open);

    case "expand": {
      if (state.open[action.path]) return state;
      return { ...state, open: { ...state.open, [action.path]: true } };
    }

    case "collapse": {
      if (!state.open[action.path]) return state;
      const open = { ...state.open };
      delete open[action.path];
      // The descendants' open bits are deliberately kept. Reopening a folder
      // and finding it collapsed flat is a small loss of place every time,
      // and the listings are cached anyway so restoring the shape is free.
      return pruned({ ...state, open });
    }

    /** Every folder folds; the listings stay cached, so unfolding is free. */
    case "collapse_all":
      return Object.keys(state.open).length === 0 ? state : pruned({ ...state, open: {} });

    case "toggle":
      return reduce(state, { type: state.open[action.path] ? "collapse" : "expand", path: action.path });

    case "loading":
      return withDir(state, action.path, { status: "loading", error: null });

    case "loaded": {
      const next = withDir(state, action.path, {
        status: "ready",
        entries: sortEntries(action.entries),
        truncated: Boolean(action.truncated),
        error: null,
      });
      // Only the root listing reports the absolute root; a sub-listing repeats
      // it, and overwriting from a sub-listing would be a no-op that still
      // invites someone to "fix" it into something scope-dependent.
      return pruned(action.path === ROOT && action.root ? { ...next, root: action.root } : next);
    }

    case "failed":
      // The entries this node had are dropped: a listing that failed to
      // refresh is not evidence that its old contents are still there.
      return pruned(
        withDir(state, action.path, {
          status: "error",
          entries: [],
          truncated: false,
          error: action.error,
        }),
      );

    /**
     * The keyboard moved. Plain, the selection collapses to the new row;
     * extending (Shift), the range from the anchor grows or shrinks to it.
     */
    case "cursor":
      return action.extend ? reduce(state, { type: "select_range", path: action.path }) : selectOne(state, action.path);

    /**
     * Show this path: every folder above it opens (the loads follow from
     * {@link pendingLoads}) and the cursor lands on it. `selected` is left
     * alone — which row reads as open is the caller's fact, not the reveal's.
     */
    case "reveal": {
      const open = { ...state.open };
      for (let p = parentPath(action.path); p !== ROOT; p = parentPath(p)) open[p] = true;
      return { ...state, open, cursor: action.path };
    }

    case "select":
      return selectOne(state, action.path);

    /** Cmd-click: in or out of the selection, and the anchor moves here. */
    case "select_toggle": {
      const has = state.selection.includes(action.path);
      const selection = has ? state.selection.filter((p) => p !== action.path) : [...state.selection, action.path];
      // The anchor follows a row toggled in; a row toggled out is not a
      // place a range can start from, so the anchor stays where it was.
      return { ...state, cursor: action.path, anchor: has ? state.anchor : action.path, selection };
    }

    /** Shift-click or Shift+arrow: the anchor's range to here; no anchor, just here. */
    case "select_range": {
      // A row that is not on screen — folded away, never listed — is not a
      // place a range can end; asking for one changes nothing.
      const rows = visibleRows(state);
      if (!rows.some((r) => r.kind === "entry" && r.path === action.path)) return state;
      if (state.anchor === null) return selectOne(state, action.path);
      const range = rangeBetween(rows, state.anchor, action.path);
      if (range.length === 0) return state;
      return { ...state, cursor: action.path, selection: range };
    }

    case "select_all": {
      const all = visiblePaths(state);
      return { ...state, selection: all, anchor: state.anchor ?? all[0] ?? null };
    }

    case "select_clear":
      return state.selection.length === 0 && state.anchor === null ? state : { ...state, selection: [], anchor: null };

    /** The inline preview, for a tree that opens files itself. */
    case "show":
      return state.shown === action.path ? state : { ...state, shown: action.path };

    case "hide":
      return state.shown === null ? state : { ...state, shown: null };

    /**
     * Something on disk moved. Every cached listing is suspect, so each goes
     * back to `idle` for {@link pendingLoads} to pick up — but the entries
     * stay on screen until the replacements land, because blanking a tree on
     * a 2s poll is a flicker nobody asked for.
     */
    case "invalidate": {
      const dirs = {};
      for (const [path, dir] of Object.entries(state.dirs)) {
        dirs[path] = { ...dir, status: "idle" };
      }
      return { ...state, dirs };
    }

    /**
     * The watcher named what moved: only the listings it touched go back to
     * `idle`, and only when they were loaded — a folder nobody has opened is
     * not fetched on somebody else's account.
     */
    case "refresh_dirs": {
      let changed = false;
      const dirs = { ...state.dirs };
      for (const path of action.paths) {
        const dir = dirs[path];
        if (dir && dir.status === "ready") {
          dirs[path] = { ...dir, status: "idle" };
          changed = true;
        }
      }
      return changed ? { ...state, dirs } : state;
    }

    default:
      return state;
  }
}

/**
 * The listings a `file_changed` frame makes stale: the parent of the path (and
 * of `from`, for a rename), or the root for a rescan. Deduplicated.
 * @param {{kind: string, path: string, from?: string | null}} change
 */
export function dirsToRefresh(change) {
  if (!change) return [];
  if (change.kind === "rescan") return [ROOT];
  const out = new Set([parentPath(change.path)]);
  if (change.kind === "renamed" && typeof change.from === "string") out.add(parentPath(change.from));
  return [...out];
}

/**
 * The directories that should be fetched right now.
 *
 * It walks only the *visible* open spine rather than every key in `open`,
 * because `collapse` keeps its descendants' open bits (see above). Returning
 * those too would fetch a subtree nobody can see, on every poll, forever.
 */
export function pendingLoads(state) {
  const out = [];
  const walk = (path) => {
    const dir = state.dirs[path];
    if (!dir || dir.status === "idle") {
      out.push(path);
      return; // Its children are unknown until it answers.
    }
    if (dir.status !== "ready") return;
    for (const e of dir.entries) {
      if (e.dir && state.open[e.path]) walk(e.path);
    }
  };
  walk(ROOT);
  return out;
}

/**
 * The flat list of rows to draw, in order, with the nesting depth each sits at.
 *
 * Flat because the list is virtualised: windowing needs an index, and a
 * recursive render has none. Depth comes back as a number the row indents
 * itself by, which is also what `aria-level` wants. Every row is a
 * `TreeRowLike` (`ui/tree/treeListModel.mjs`): `id`, `depth`, and for an
 * entry its `label` and whether it `expandable`s — so the generic tree can
 * navigate and drop on it without knowing what a file is.
 *
 * The non-entry rows — `loading`, `error`, `empty`, `truncated` — stand in for
 * a directory's children rather than replacing the directory, so a failure
 * costs you one subtree and nothing else.
 */
export function visibleRows(state) {
  const rows = [];
  /** A row that stands in for a listing's children; the cursor never lands on it. */
  const notice = (kind, path, depth, extra = {}) => ({ kind, id: `${kind}:${path}`, path, depth, focusable: false, ...extra });
  const walk = (path, depth) => {
    const dir = state.dirs[path];
    if (!dir) return;
    if (dir.status === "error") {
      rows.push(notice("error", path, depth, { error: dir.error }));
      return;
    }
    if (dir.entries.length === 0) {
      rows.push(notice(dir.status === "ready" ? "empty" : "loading", path, depth));
      return;
    }
    for (const entry of dir.entries) {
      const expanded = entry.dir && Boolean(state.open[entry.path]);
      rows.push({ kind: "entry", id: entry.path, path: entry.path, depth, entry, expanded, label: entry.name, expandable: entry.dir });
      if (expanded) walk(entry.path, depth + 1);
    }
    if (dir.truncated) rows.push(notice("truncated", path, depth));
  };
  walk(ROOT, 0);
  return rows;
}

/**
 * The rows the list itself draws: {@link visibleRows} without the root's own
 * `loading` / `empty` / `error` notice. At the root that notice *is* the
 * panel — there is no surrounding listing to give it context — so the panel
 * states it in its frame (a spinner, the empty hint, the error with a retry)
 * and the list shows only what is under the root. A nested directory's
 * notices stay: they sit in place of that directory's children.
 */
export function bodyRows(state) {
  return visibleRows(state).filter((r) => !(r.path === ROOT && (r.kind === "loading" || r.kind === "empty" || r.kind === "error")));
}

/**
 * The rows with a **draft** row spliced in for an in-tree new file/folder,
 * or a picture pasted from the clipboard: the person names it in place
 * rather than in a modal. The draft
 * sits as the first child of its target directory (or at the top for the root),
 * one level deeper than the directory row. When the directory is not visible
 * (not yet expanded) the draft falls back to the top, but callers expand the
 * target first so that is rare.
 * @param {readonly object[]} rows the output of {@link visibleRows}
 * @param {{dir: string, kind: "file"|"dir"|"image"} | null | undefined} draft
 */
export function withDraft(rows, draft) {
  if (!draft) return rows;
  const base = { kind: "draft", id: "__draft__", path: ROOT, dir: draft.dir, entryKind: draft.kind, focusable: false };
  if (!draft.dir) return [{ ...base, depth: 0 }, ...rows];
  const i = rows.findIndex((r) => r.kind === "entry" && r.path === draft.dir);
  if (i === -1) return [{ ...base, depth: 0 }, ...rows];
  const out = rows.slice();
  out.splice(i + 1, 0, { ...base, depth: rows[i].depth + 1 });
  return out;
}

/**
 * What *open* means on the cursor row: a folder toggles, a file opens. Null
 * with no cursor or a cursor on a notice row. The `open_entry` command. The
 * arrows, Home, End and type-ahead are the generic tree's
 * (`ui/tree/treeListModel.mjs`); only what a file *is* stays here.
 */
export function activate(rows, cursor) {
  const at = rows.find((r) => r.kind === "entry" && r.path === cursor);
  if (!at) return null;
  return at.entry.dir ? { kind: "toggle", path: at.path } : { kind: "open", path: at.path };
}

/**
 * The key a component watches to know the set of directories to fetch
 * changed. The root is the empty string, so `[]` and `[""]` would join to the
 * same `""` — and a refresh that re-pends only the root would look like no
 * change at all. The count is part of the key so that can never happen.
 * @param {string[]} pending
 */
export function loadKey(pending) {
  return `${pending.length}\u0000${pending.join("\u0000")}`;
}
