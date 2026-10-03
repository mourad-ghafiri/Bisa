/**
 * The commit graph (ide/05): every commit reachable from a
 * ref, newest first, with lanes and edges the engine laid out, drawn in an
 * SVG cell per row and windowed by `VirtualList`.
 *
 * The rows are **sparse**: an array of `total` slots that windows land in as
 * the list scrolls (`graphModel.mjs`: `mergeWindow`, `holesIn`), so the first
 * screen paints from the first page and a jump to row 87,000 fetches one
 * window, not two hundred pages. The header says *laying out… N so far* until
 * `done`.
 *
 * Search is the node's (`GET …/graph/…/search`): it scans the whole laid-out
 * log and answers row indexes, so `Enter`, `n` and `N` jump to a match wherever
 * it is; the rows on screen that match are highlighted by the same rule
 * (`matches`), so what dims and what was found agree. The topology stays
 * where it was while you look through it — which is the point of a graph.
 *
 * What can be done to a commit is one list (`commitActionsModel.mjs`), run
 * through one hook (`useCommitActions`) and asked about by one set of
 * dialogs (`CommitActionDialogs`): a row reveals the four a person reaches
 * for on hover — cherry-pick, branch here, tag here, open — its context
 * menu holds them all, the commit document's toolbar draws the same list with
 * icons and the reason an action is off, and a ref chip is a menu of its own
 * (switch to a branch, delete a tag, copy). The consented ones confirm first
 * and write a recovery ref before anything moves.
 *
 * A row's click **opens the commit in the centre** (`onOpenCommit`, a
 * `commit:<sha>` document) and keeps the row pressed; the graph is the
 * list and the lanes, never the place a commit is read.
 */

import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import { api } from "../../api";
import { errorFields, log } from "../../log";
import type { GitInProgress, GraphMatches, GraphRow, GraphWindow } from "../../types";
import { Button, CURSOR_RING, ContextMenu, EmptyState, ErrorNote, ICON, MoreMenu, RelativeTime, Skeleton, TextInput, Tooltip, VirtualList, WorkingDot, failureText, useTokenPx } from "../../ui";
import type { MenuItem } from "../../ui";
import { SEARCH_HISTORY, onDoor } from "../../shell/shortcuts";
import { useViewState } from "../../shell/viewMemoryStore";
import { idValue, textValue } from "../../shell/viewValuesModel.mjs";
import type { GraphRefScope } from "../../types";
import { rootKey } from "../_workbench/workbenchModel.mjs";
import { gitPlace } from "../_workbench/idePlacesModel.mjs";
import { RefChip } from "./CommitActionControls";
import { CommitActionDialogs } from "./CommitActionDialogs";
import { headOf, menuActions } from "./commitActionsModel.mjs";
import type { ActionContext } from "./commitActionsModel.mjs";
import { patchSession, useGitSession } from "./gitPanelStore";
import { LANE_W, NODE_R, PAGE, commitCount, currentBranchOf, cursorAfterKey, emptyRows, historyMenu, holesIn, laneColor, laneX, lanesWidth, layoutWord, matches, mergeWindow, nextIndex, searchStatus, strokesFor } from "./graphModel.mjs";
import type { SparseRows } from "./graphModel.mjs";
import { useCommitActions } from "./useCommitActions";
import type { ActionTarget, CommitActions } from "./useCommitActions";
import { t as tr } from "../../i18n/l10n.mjs";

/** A graph row is one theme row (`--spacing-row-sm`), and the lane pitch is the same number; this is the frame-zero fallback. */
const ROW_H_FALLBACK = 28;
const POLL_MS = 800;
const SEARCH_DEBOUNCE_MS = 180;

function Lanes({ row, prev, width, height }: { row: GraphRow; prev: GraphRow | null; width: number; height: number }) {
  const strokes = strokesFor(row, prev);
  const h = height;
  return (
    <svg width={width} height={h} className="shrink-0" aria-hidden>
      {strokes.map((s, i) => {
        const y1 = s.y1 * h;
        const y2 = s.y2 * h;
        const d =
          s.kind === "line"
            ? `M ${s.x1} ${y1} L ${s.x2} ${y2}`
            : // A cubic that leaves vertically and arrives vertically reads as a branch, not a diagonal.
              `M ${s.x1} ${y1} C ${s.x1} ${(y1 + y2) / 2}, ${s.x2} ${(y1 + y2) / 2}, ${s.x2} ${y2}`;
        return <path key={i} d={d} fill="none" stroke={laneColor(s.lane)} strokeWidth={1.5} />;
      })}
      <circle /* terminology-lint-ignore: circle - SVG element name, third-party API */
        cx={laneX(row.lane)}
        cy={h / 2}
        r={row.parents > 1 ? NODE_R + 1 : NODE_R}
        fill={row.parents > 1 ? "var(--color-surface)" : laneColor(row.lane)}
        stroke={laneColor(row.lane)}
        strokeWidth={1.5}
      />
    </svg>
  );
}

/** The context menu of a commit: every action, in `MENU_SECTIONS`, with the reason an item is off. */
function menuFor(target: ActionTarget, actions: CommitActions, ctx: ActionContext): MenuItem[] {
  return menuActions(target, ctx).map((a) => ({
    label: a.disabled && a.reason ? `${a.label} — ${a.reason}` : a.label,
    icon: ICON[a.icon],
    disabled: a.disabled,
    separatorBefore: a.separatorBefore,
    onSelect: () => actions.start(a.id, target),
  }));
}

/** A slot no window has landed in yet. */
function SkeletonRow({ width, height }: { width: number; height: number }) {
  return (
    <div className="flex items-center gap-2 pr-2" style={{ height }} aria-busy>
      <span style={{ width }} className="shrink-0" />
      <Skeleton className="h-3 w-14" />
      <Skeleton className="h-3 w-1/2" />
    </div>
  );
}

export function CommitGraph({
  wid,
  refreshKey = 0,
  inProgress = null,
  onChanged,
  onOpenCommit,
}: {
  wid: string;
  refreshKey?: number;
  /** An operation left half-done in the checkout, from its status: the consented actions wait for it. */
  inProgress?: GitInProgress | null;
  onChanged?: () => void;
  /** Show a commit in the centre — what a row's click and *Open the commit* do. */
  onOpenCommit: (sha: string) => void;
}) {
  /** The last window's facts — total, done, stale — and the sparse rows. */
  const [meta, setMeta] = useState<Pick<GraphWindow, "total" | "done" | "stale"> | null>(null);
  const [rows, setRows] = useState<SparseRows>(() => emptyRows(0));
  const rowHeight = useTokenPx("--spacing-row-sm", ROW_H_FALLBACK);
  const [error, setError] = useState<string | null>(null);
  // What was searched for and the commit that was open are how the history
  // stood: a view switch keeps them, and a restart. The matches are read again.
  const place = gitPlace(rootKey("workstream", wid));
  const [query, setQuery] = useViewState(place, "history.q", "", textValue);
  const [found, setFound] = useState<GraphMatches | null>(null);
  const searchBox = useRef<HTMLInputElement>(null);
  /**
   * One row the keyboard is on — the list is a single tab stop and this
   * moves inside it. The search steps (`n`/`N`) land the same cursor on a
   * match, so "where am I" and "which match" are one answer.
   */
  const [cursor, setCursor] = useState(-1);
  const [scrollTo, setScrollTo] = useState<{ index: number; nonce: number }>({ index: -1, nonce: 0 });
  const [open, setOpen] = useViewState<string | null>(place, "history.open", null, idValue);
  const range = useRef<[number, number]>([0, 0]);
  const inflight = useRef(new Set<number>());
  /** The fetches of the current layout; a relayout or an unmount ends them. */
  const fetches = useRef(new AbortController());
  /** The layout's row count once known, so no window is asked for past it. */
  const layoutTotal = useRef<number | null>(null);
  useEffect(() => () => fetches.current.abort(), []);

  // The one filter, kept in the checkout's session so a view switch keeps
  // it (ide/05); the search line shows on demand and hides on Esc.
  const scope = rootKey("workstream", wid);
  const session = useGitSession(scope);
  const refs: GraphRefScope = session.graphRefs;
  const setRefs = (next: GraphRefScope) => patchSession(scope, { graphRefs: next });
  // A search kept from the last visit shows its line.
  const [searching, setSearching] = useState(() => query.trim() !== "");
  /** The rows' ids, so the list can name the row the cursor is on (`aria-activedescendant`). */
  const optionBase = useId();
  const optionId = (i: number) => `${optionBase}-row-${i}`;

  /** Fetch one window and lay it into the sparse rows. */
  const load = useCallback(
    async (from: number, count: number, refresh = false) => {
      if (inflight.current.has(from) && !refresh) return;
      // A hole the layout does not reach — the rows shrank under a relayout —
      // is not a window to ask for; asking would answer nothing and ask again.
      if (!refresh && layoutTotal.current !== null && from >= layoutTotal.current) return;
      const signal = fetches.current.signal;
      inflight.current.add(from);
      try {
        const w = await api.ideGraph("workstream", wid, from, count, refresh, refs, signal);
        if (signal.aborted) return;
        layoutTotal.current = w.total;
        setError(null);
        setMeta({ total: w.total, done: w.done, stale: w.stale });
        setRows((prev) => mergeWindow(prev, w));
      } catch (e) {
        if (!signal.aborted) setError(failureText("work", "commit-graph-failed", e));
      } finally {
        inflight.current.delete(from);
      }
    },
    [wid, refs],
  );

  const loaded = useMemo(() => rows.filter((r): r is GraphRow => r !== undefined), [rows]);
  // Where HEAD is and which branch it is on, from the rows in hand: what
  // makes *checkout here* and *switch to this branch* pointless is said.
  const headId = useMemo(() => headOf(rows), [rows]);
  const currentBranch = useMemo(() => currentBranchOf(headId, loaded), [headId, loaded]);

  // Every action runs through one hook against the checkout's session; a
  // landed one bumps the session's `stale`, and the graph relayouts on it.
  const actions = useCommitActions(wid, {
    current: currentBranch,
    onOpen: (sha) => {
      setOpen(sha);
      onOpenCommit(sha);
    },
  });
  useEffect(() => {
    if (session.stale === 0) return;
    void load(0, PAGE, true);
    onChanged?.();
    // The nonce is the trigger: `load` changes with the root and the refs,
    // which the effect below reloads for already, and `onChanged` is the caller's.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session.stale]);

  // First load, and a reload when the caller says the tree changed: the rows
  // start over, so a relaid graph is never shown half old.
  useEffect(() => {
    fetches.current.abort();
    fetches.current = new AbortController();
    layoutTotal.current = null;
    setMeta(null);
    setRows(emptyRows(0));
    setFound(null);
    setCursor(-1);
    inflight.current.clear();
    void load(0, PAGE);
  }, [load, refreshKey]);

  // While the layout is running or stale, poll the first page; then stop.
  // A relayout invalidates every window: the rows are emptied so the
  // holes refill from the new layout as they scroll into view.
  useEffect(() => {
    if (!meta || (meta.done && !meta.stale)) return;
    const t = window.setTimeout(() => {
      if (meta.stale) setRows(emptyRows(0));
      void load(0, PAGE);
    }, POLL_MS);
    return () => window.clearTimeout(t);
  }, [meta, load]);

  /** A row's key, stable across renders so the list's offsets are not rebuilt over every row each render. */
  const keyOf = useCallback((r: GraphRow | undefined, i: number) => r?.id ?? `hole:${i}`, []);
  /** The list says what is on screen; every page with a hole in it is asked for. */
  const onRange = useCallback(
    (first: number, last: number) => {
      range.current = [first, last];
      for (const h of holesIn(rows, first, last)) void load(h.from, h.count);
    },
    [rows, load],
  );
  // Rows changed (a window landed, or the array was emptied): re-check the screen.
  useEffect(() => {
    const [first, last] = range.current;
    for (const h of holesIn(rows, first, last)) void load(h.from, h.count);
  }, [rows, load]);

  // The search: the node scans the whole laid log; debounced, the previous
  // one aborted. Re-run when the layout finishes, so a match in the tail is found.
  useEffect(() => {
    const q = query.trim();
    if (!q) {
      setFound(null);
      return;
    }
    const ac = new AbortController();
    const t = window.setTimeout(() => {
      api
        .ideGraphSearch("workstream", wid, q, 0, 500, refs, ac.signal)
        .then((m) => {
          if (!ac.signal.aborted) setFound(m);
        })
        .catch((e: unknown) => {
          // A search that does not land leaves the last answer standing.
          if (ac.signal.aborted) return;
          log.debug("graph", "the log search could not be run; the last answer stands", { workstream: wid, ...errorFields(e) });
        });
    }, SEARCH_DEBOUNCE_MS);
    return () => {
      window.clearTimeout(t);
      ac.abort();
    };
  }, [query, wid, refs, meta?.done]);

  // ⌘⇧H from the keymap: show the search line and put the caret in it.
  useEffect(() => {
    return onDoor(SEARCH_HISTORY, () => setSearching(true));
  }, []);
  const was = useRef(searching);
  useEffect(() => {
    // Only a change of the line: at mount a search that came back stands as
    // it was left — it neither takes the focus nor is cleared.
    if (was.current === searching) return;
    was.current = searching;
    if (searching) searchBox.current?.focus();
    else {
      setQuery("");
      setCursor(-1);
    }
  }, [searching, setQuery]);

  const width = useMemo(() => Math.max(LANE_W, lanesWidth(loaded)), [loaded]);
  const total = meta?.total ?? 0;
  const ctx = { busy: actions.busy, headId, inProgress, currentBranch };

  /** Jump to the next match, wherever it is: the window there is fetched as the list lands on it. */
  const jump = (dir: 1 | -1) => {
    if (!found) return;
    const i = nextIndex(found.indices, cursor, dir);
    if (i < 0) return;
    setCursor(i);
    setScrollTo((prev) => ({ index: i, nonce: prev.nonce + 1 }));
    const row = rows[i];
    if (row) setOpen(row.id);
  };

  /** Put the cursor on a row and bring it on screen. */
  const moveCursor = (to: number) => {
    if (total === 0) return;
    const i = Math.max(0, Math.min(total - 1, to));
    setCursor(i);
    setScrollTo((prev) => ({ index: i, nonce: prev.nonce + 1 }));
  };
  /** What a click on a row does; Enter and Space do the same from the keyboard: the row is pressed, the commit is the centre. */
  const toggleRow = (row: GraphRow) => {
    setOpen(row.id);
    onOpenCommit(row.id);
  };

  if (error && !meta) return <ErrorNote error={error} retry={() => void load(0, PAGE)} />;
  if (!meta) return <Skeleton className="h-32 w-full" />;
  if (total === 0) {
    return (
      <EmptyState
        title={tr("work-commit-graph-no-commits-yet")}
        hint={tr("work-commit-graph-graph-starts-first-one")}
        className="py-4"
        action={
          <Button size="sm" variant="ghost" onClick={() => void load(0, PAGE, true)}>
            <ICON.refresh size={12} aria-hidden />{tr("work-commit-graph-refresh")}</Button>
        }
      />
    );
  }

  // The header's `⋮`: the model says the items; this binds each to its act.
  const menu: MenuItem[] = historyMenu({ refs, searching }).map((item) => ({
    label: item.label,
    icon: item.icon ? (ICON as Record<string, typeof ICON.file>)[item.icon] : undefined,
    separatorBefore: item.separatorBefore,
    onSelect: () => {
      if (item.id === "search") setSearching((s) => !s);
      else if (item.id === "relayout") void load(0, PAGE, true);
      else if (item.id.startsWith("refs:")) setRefs(item.id.slice(5) as GraphRefScope);
    },
  }));

  return (
    <div className="flex h-full min-h-0 min-w-0 flex-col gap-2">
      {/* One line: the count, one status word while something happens, and
          the ⋮ — the search and the one filter live behind it. A stable
          number a person can read, never a sentence with a number inside it. */}
      <div className="flex shrink-0 items-center gap-2 text-2xs text-text-dim">
        <span className="tnum text-text">{commitCount(meta)}</span>
        {refs === "head" && <span className="shrink-0">{tr("work-commit-graph-branch")}</span>}
        {(layoutWord(meta) || actions.busy) && (
          <span className="inline-flex min-w-0 items-center gap-1 truncate">
            <WorkingDot title={layoutWord(meta) ?? tr("work-commit-graph-running")} />
            {layoutWord(meta) ?? tr("work-commit-graph-running-action")}
          </span>
        )}
        <span className="flex-1" />
        <MoreMenu label={tr("work-commit-graph-search-filter-lay-out")} items={menu} className="h-7 w-7 rounded-control border border-border hover:bg-surface-2" />
      </div>
      {searching && (
        <div className="flex shrink-0 items-center gap-1.5 text-2xs text-text-dim">
          <TextInput
            ref={searchBox}
            value={query}
            placeholder={tr("work-commit-graph-search-whole-log-author-subject-id")}
            aria-label={tr("work-commit-graph-search-commit-history")}
            className="h-6 min-w-0 flex-1 text-2xs"
            onChange={(e) => {
              setQuery(e.target.value);
              setCursor(-1);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") jump(e.shiftKey ? -1 : 1);
              if (e.key === "Escape") setSearching(false);
            }}
          />
          {query.trim() && (
            <span className="shrink-0 tnum" aria-live="polite">
              {searchStatus(found, cursor)}
            </span>
          )}
          <Tooltip label={tr("work-commit-graph-previous-match-n")}>
            <span className="inline-flex">
              <Button size="icon" variant="ghost" disabled={!found || found.indices.length === 0} onClick={() => jump(-1)} aria-label={tr("work-commit-graph-previous-match")}>
                <ICON.back size={12} aria-hidden />
              </Button>
            </span>
          </Tooltip>
          <Tooltip label={tr("work-commit-graph-next-match-n")}>
            <span className="inline-flex">
              <Button size="icon" variant="ghost" disabled={!found || found.indices.length === 0} onClick={() => jump(1)} aria-label={tr("work-commit-graph-next-match")}>
                <ICON.forward size={12} aria-hidden />
              </Button>
            </span>
          </Tooltip>
          <Tooltip label={tr("work-commit-graph-hide-search-esc")}>
            <span className="inline-flex">
              <Button size="icon" variant="ghost" onClick={() => setSearching(false)} aria-label={tr("work-commit-graph-hide-search")}>
                <ICON.close size={12} aria-hidden />
              </Button>
            </span>
          </Tooltip>
        </div>
      )}
      {/*
        One tab stop for the whole list, however long it is: the rows are not
        focusable, the cursor below is what moves. The dialogs further down
        share this element, so the navigation keys act only when the list
        itself is what has focus — never on a keypress inside a dialog.
      */}
      <div
        role="listbox"
        tabIndex={0}
        aria-label={tr("work-commit-graph-commit-history")}
        aria-activedescendant={cursor >= 0 && rows[cursor] ? optionId(cursor) : undefined}
        className="min-h-0 flex-1 overflow-hidden rounded-control border border-border bg-surface-2 focus:outline-none"
        onFocus={(e) => {
          if (e.target === e.currentTarget && cursor < 0) moveCursor(0);
        }}
        onKeyDown={(e) => {
          if (e.target instanceof HTMLInputElement) return;
          if (e.key === "n") return jump(1);
          if (e.key === "N") return jump(-1);
          if (e.target !== e.currentTarget) return;
          const to = cursorAfterKey(e.key, cursor, total);
          if (to !== null) {
            moveCursor(to);
            e.preventDefault();
            return;
          }
          switch (e.key) {
            case "Enter":
            case " ": {
              const row = rows[cursor];
              if (row) toggleRow(row);
              break;
            }
            default:
              return;
          }
          e.preventDefault();
        }}
      >
        <VirtualList
          items={rows}
          rowHeight={rowHeight}
          measure
          keyOf={keyOf}
          className="h-full overflow-auto"
          scrollToIndex={scrollTo.index}
          scrollNonce={scrollTo.nonce}
          onRange={onRange}
          render={(row, i) => {
            if (!row) return <SkeletonRow width={width} height={rowHeight} />;
            const hit = matches(row, query);
            const isOpen = open === row.id;
            const atCursor = i === cursor;
            return (
              <ContextMenu items={menuFor(row, actions, ctx)} className="group block" selected={isOpen}>
                <div
                  id={optionId(i)}
                  role="option"
                  aria-selected={atCursor}
                  data-cursor={atCursor || undefined}
                  onClick={() => {
                    setCursor(i);
                    toggleRow(row);
                  }}
                  // The height is the theme's row token, the same number the
                  // list positions by — a fixed class here is what kept the
                  // compact density from compacting.
                  style={{ height: rowHeight }}
                  // The open row is where you are (neutral); the cursor ring is the
                  // keyboard's place and where `n`/`N` land a search hit.
                  className={`flex min-w-0 items-center gap-2 pr-2 text-2xs ${isOpen ? "bg-selected" : "hover:bg-surface"} ${hit ? "" : "opacity-35"} ${
                    atCursor ? CURSOR_RING : ""
                  }`}
                >
                  <Lanes row={row} prev={i > 0 ? (rows[i - 1] ?? null) : null} width={width} height={rowHeight} />
                  <span className="shrink-0 font-mono text-text-dim">{row.short}</span>
                  {row.refs.map((r) => (
                    <RefChip key={`${r.kind}:${r.name}`} commit={row} refName={r} actions={actions} ctx={ctx} />
                  ))}
                  <span className="min-w-0 flex-1 truncate text-text" title={row.subject}>
                    {row.subject}
                  </span>
                  <span className="max-w-[6rem] shrink-0 truncate text-text-dim">{row.author}</span>
                  <RelativeTime at={row.timestamp} className="shrink-0 text-text-dim" />
                  {/* Every verb behind one `⋮` at the row's end — the same menu a
                      right-click opens — so the subject keeps the row (ide/05
                      §One action list); revealed on hover, focus and the open row. */}
                  <span className="row-actions anim flex shrink-0 items-center" onClick={(e) => e.stopPropagation()}>
                    <MoreMenu label={tr("work-commit-graph-more", { short: row.short })} items={menuFor(row, actions, ctx)} />
                  </span>
                </div>

              </ContextMenu>
            );
          }}
        />
        <CommitActionDialogs actions={actions} />
      </div>
    </div>
  );
}
