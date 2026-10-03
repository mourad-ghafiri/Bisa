/**
 * The Inbox: the one screen for what concerns you — what is owed to you,
 * and what happened to what you asked for.
 *
 * Rows are **things**, not events — a goal, a channel, a direct channel, a
 * workstream, a project, a workflow — mutated in place as activity arrives.
 * A row carries its **asks** (a gate, a question — answered here, in the
 * very card the goal page's *Your move* band mounts) and its **notices**
 * (a run that finished or failed, a script that failed, a start event that
 * could not start a run, a pull request opened — read here, opened there,
 * worded by the same function a Pulse row gets). The detail pane is the real
 * conversation when the thing has one, so you answer a step's question,
 * adopt the Workflow Agent's proposal or reply to a teammate without being
 * bounced to another screen and losing your place.
 *
 * # A row you read is a row you keep
 *
 * The vocabulary is three states wide — waiting, unread, read — with
 * *handled* riding alongside as the workspace's own fact about a decision.
 * Weight and a dot carry unread, dimming carries read, and a handled row
 * says what was decided rather than going quiet. What is *owed* is the asks
 * alone: a notice never counts toward the sidebar's badge or a goal card.
 * All of that is decided in {@link file://./_studio/inboxModel.mjs}, which
 * has no React in it and is therefore the part a test can reach; this file
 * is paint and effects.
 *
 * # Why the rows are held whole and narrowed here
 *
 * `GET /inbox` takes the same filter and source this screen offers, and the
 * screen still asks for everything and narrows locally. The `inbox` SSE
 * channel patches one row in place, so a row's state changes between
 * fetches — a server-side filter would mean re-requesting the list to
 * discover that the row you just read no longer matches. Holding every row
 * is also what keeps the detail pane pinned to a row a filter has hidden,
 * and what lets the strip's counts say what every tab would show.
 *
 * The filters live in the URL (`?filter=&source=&item=`) like the Pulse's
 * concept, replaced rather than pushed: holding `j` should not bury the way
 * out under fifty history entries. The screen's `<h1>` is the shell's.
 *
 * # Left as it stood
 *
 * The rows are kept for the window, so the list is never empty on the way
 * back: it is drawn at once and read again behind. Where the list was
 * scrolled is the screen's memory (`shell/viewMemoryStore`), and the row
 * picked is brought back in view once. What is remembered of one row — the
 * notices opened under *What happened*, whether all were shown, where its
 * detail was scrolled — is kept under the row's own place
 * (`inboxPlaceModel.rowPlace`). A conversation's place in its thread is the
 * thread's own to keep.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ApiError, api } from "../api";
import { pulseLine } from "../activity";
import { errorFields, log } from "../log";
import { subscribe as busSubscribe } from "../bus";
import { useReloadOnReconnect } from "../ui/useReloadOnReconnect";
import { navigate, setSearch, useSearchValue } from "../router";
import { openHarnessSession } from "../shell/sessionDoors";
import { tabOfSession } from "../shell/terminalsModel.mjs";
import { useHarnessLabels } from "../shell/useHarnesses";
import { useTerminals } from "../shell/useTerminals";
import { useViewScroll } from "../shell/useViewScroll";
import { useWorkspace } from "../shell/useWorkspaceData";
import { placeOf, useViewState } from "../shell/viewMemoryStore";
import { flagValue, wordsOf } from "../shell/viewValuesModel.mjs";
import type { InboxRow, NoticeDto } from "../types";
import {
  AnimatedList,
  Button,
  Chip,
  ContextMenu,
  CountBadge,
  Dot,
  EmptyState,
  useCleared,
  ErrorNote,
  GATE_ICON,
  ICON,
  INBOX_KIND_ICON,
  MoreMenu,
  Pending,
  ReadLine,
  RelativeTime,
  ScreenBar,
  SectionHeader,
  SegmentedControl,
  Tabs,
  cn,
  type LucideIcon,
  type MenuItem,
  useToast, harnessMark } from "../ui";
import { CurrentStepPill } from "./_goals/CurrentStepPill";
import { HolderBadge } from "./_goals/HolderBadge";
import { PulseRow, type PulseItem } from "./_pulse/PulseRow";
import { itemOf, linkOf } from "./_pulse/pulseModel.mjs";
import { readWords } from "./_settings/loadModel.mjs";
import { Chat } from "./_studio/Chat";
import { ConversationHeader } from "./_studio/Conversation";
import { READ_AFTER_MS } from "./_studio/readModel.mjs";
import {
  applyDelta,
  conversationOf,
  counts,
  doorOf,
  filterOf,
  filterSegments,
  groupRows,
  isHandled,
  joinOf,
  joinWords,
  keyAction,
  kindLabel,
  aboutWords,
  rowGlyph,
  needsOf,
  RELOAD_DEBOUNCE_MS,
  doorLabel,
  emptyWords,
  isNewestRead,
  markBatches,
  needsKey,
  needsReload,
  nextSelection,
  noticesOf,
  readOnSelect,
  rowState,
  sourceIdOf,
  sourceTabs,
  summarize,
  type FilterId,
  type SourceId,
  unreadKeys,
  unreadNoticesOf,
  visibleRows,
  waitingOf,
  waitingWords,
  channelOfRow,
  scopeKindOf,
  wantsList,
  withReadMark,
} from "./_studio/inboxModel.mjs";
import { ASK_ATTR, FOCUS_ATTR, NeedsActionCard } from "./_studio/NeedsAction";
import { readKey } from "./_work/keptReadsModel.mjs";
import { keepRead, keptRead } from "./_work/keptReadsStore";
import { parseOpened, toggled } from "./_work/openedModel.mjs";
import { rowPlace } from "./inboxPlaceModel.mjs";
import { t as tr } from "../i18n/l10n.mjs";
import { rich } from "../i18n/rich";

/** How many notices the *What happened* section shows before it folds. */
const NOTICES_SHOWN = 5;

/** Where the screen keeps its memory; a row's is kept beneath it (`rowPlace`). */
const PLACE = placeOf({ name: "inbox" });
/** The key the rows are kept under for the window's life. */
const ROWS_READ = readKey("inbox");
/** No notice opened: what a row begins with. */
const NONE_OPENED: ReadonlySet<string> = new Set();

/**
 * The model names an icon; this is the only place that turns a name into a
 * component. Keeping the map here is what lets the model stay importable by
 * `node --test`, which cannot load the UI kit.
 */
function glyphFor(name: string): LucideIcon | undefined {
  if (name.startsWith("gate:")) return GATE_ICON[name.slice(5) as keyof typeof GATE_ICON];
  if (name.startsWith("kind:")) return INBOX_KIND_ICON[name.slice(5) as keyof typeof INBOX_KIND_ICON];
  if (name.startsWith("notice:")) return ICON.pulse;
  if (name === "waiting") return ICON.waiting;
  if (name === "question") return ICON.question;
  if (name === "handled") return ICON.ok;
  return undefined;
}

/** The tone class a row's second line wears — the accent for an ask, the Pulse's tones for a notice. */
const LINE_TONE: Record<string, string> = {
  accent: "font-medium text-accent-ink",
  ok: "text-ok",
  fail: "text-danger",
  danger: "text-danger",
  wait: "text-accent-ink",
  warn: "text-warn",
  spine: "text-text",
  dim: "text-text-dim",
};

function open(row: InboxRow): void {
  const door = doorOf(row);
  if (!door) return;
  navigate(door.route, door.search ? Object.fromEntries(new URLSearchParams(door.search)) : undefined);
}

/**
 * A harness waiting in its terminal (ide/06 §Reporting): the harness, the
 * wait in the roster's words, how long, and one verb — the workstream in
 * the Project IDE with that terminal tab in front, where the prompt is
 * answered. Nothing here answers it.
 */
function WaitingSessionCard({ row }: { row: InboxRow }) {
  const labels = useHarnessLabels();
  const { sessions: terminals } = useTerminals();
  const waiting = waitingOf(row);
  if (!waiting) return null;
  const Mark = harnessMark(waiting.harness);
  const tab = tabOfSession(terminals, waiting.session);
  const openTerminal = () => {
    if (!openHarnessSession({ id: waiting.session, workstream: waiting.workstream ?? null, terminalKey: tab?.key ?? null })) open(row);
  };
  return (
    <section className="border-b border-hairline px-3 py-2" aria-label={tr("screens-inbox-waiting-terminal")}>
      <div className="flex items-center gap-2 rounded-control border border-accent/40 bg-accent-soft px-2 py-1.5">
        <Mark size={13} aria-hidden className="shrink-0 text-accent-ink" />
        <span className="min-w-0 flex-1 truncate text-xs">{waitingWords(row, labels)}</span>
        <RelativeTime at={waiting.since} className="tnum shrink-0 text-2xs text-text-dim" />
        <Button size="sm" variant="primary" disabled={!waiting.workstream} disabledReason={tr("screens-inbox-open-terminal-no-checkout")} onClick={openTerminal}>{tr("screens-inbox-open-terminal")}</Button>
      </div>
      <p className="mt-1 text-2xs text-text-dim">{tr("screens-inbox-answered-at-harness-prompt")}</p>
    </section>
  );
}

/**
 * What happened to the thing — its notices, newest first, each the Pulse's
 * own row with its door; the unread ones marked; folded past five.
 */
/**
 * The claim waiting on you on a people row (14-collaboration): somebody
 * claimed an invitation while the workspace admits by hand. Two verbs, and
 * the row goes quiet either way.
 */
function JoinCard({ row, onDone }: { row: InboxRow; onDone: () => void }) {
  const toast = useToast();
  const join = joinOf(row);
  const [busy, setBusy] = useState(false);
  if (!join) return null;
  const act = async (admit: boolean) => {
    setBusy(true);
    try {
      if (admit) await api.admitInvite(join.invite);
      else await api.refuseInvite(join.invite);
      toast.ok(admit ? tr("screens-inbox-admitted") : tr("screens-inbox-refused"));
      onDone();
    } catch (e) {
      log.warn("inbox", "a decision could not be sent", errorFields(e));
      toast.error(tr("screens-inbox-could-not-decide"));
    } finally {
      setBusy(false);
    }
  };
  return (
    <section className="border-b border-hairline px-3 py-2" aria-label={tr("screens-inbox-waiting-admitted")}>
      <div className="flex items-center gap-2 rounded-control border border-accent/40 bg-accent-soft px-2 py-1.5">
        <ICON.members size={13} aria-hidden className="shrink-0 text-accent-ink" />
        <span className="min-w-0 flex-1 truncate text-xs">{joinWords(row)}</span>
        <Button size="sm" variant="primary" disabled={busy} onClick={() => void act(true)}>{tr("screens-inbox-admit")}</Button>
        <Button size="sm" disabled={busy} onClick={() => void act(false)}>{tr("screens-inbox-refuse")}</Button>
      </div>
      <p className="mt-1 text-2xs text-text-dim">{tr("screens-inbox-they-join-human-only-role-reach")}</p>
    </section>
  );
}

function WhatHappened({ row, channelKind }: { row: InboxRow; channelKind: (id: string) => "channel" | "dm" | null }) {
  const notices = noticesOf(row);
  const unread = unreadNoticesOf(row);
  // Kept under the row's own place: another row's notices are its own.
  const place = rowPlace(PLACE, row.key);
  const [showAll, setShowAll] = useViewState(place, "notices:all", false, flagValue);
  const [expanded, setExpanded] = useViewState<ReadonlySet<string>>(place, "notices:opened", NONE_OPENED, parseOpened, wordsOf);
  if (notices.length === 0) return null;
  const shown = showAll ? notices : notices.slice(0, NOTICES_SHOWN);
  const itemOfNotice = (n: NoticeDto): PulseItem => itemOf(n, pulseLine(n));
  return (
    <section className="border-b border-hairline px-2 py-1.5" aria-label={tr("screens-inbox-what-happened")}>
      <SectionHeader title={tr("screens-inbox-what-happened")} count={notices.length} trailing={unread > 0 ? <Chip tone="accent">{tr("screens-inbox-new", { unread })}</Chip> : undefined} />
      <div className="flex flex-col">
        {shown.map((n, i) => {
          const item = itemOfNotice(n);
          const door = linkOf(n.source, channelKind);
          return (
            // An unread notice is marked by the accent dot in the gutter, level with
            // its line, rather than a stripe down the row's edge.
            <div key={item.key} className="relative">
              {i < unread && <span aria-hidden className="absolute top-[calc(var(--spacing-row)/2)] -left-0.5 h-1.5 w-1.5 -translate-y-1/2 rounded-full bg-accent" />}
              <PulseRow
                item={item}
                expanded={expanded.has(item.key)}
                hasDoor={door !== null}
                onOpen={() => {
                  if (door) navigate(door);
                }}
                onToggle={() => setExpanded((s) => toggled(s, item.key))}
              />
            </div>
          );
        })}
      </div>
      {notices.length > NOTICES_SHOWN && (
        <button type="button" className="anim mt-1 rounded px-2 text-2xs text-text-dim underline underline-offset-2 hover:text-text" onClick={() => setShowAll((v) => !v)}>
          {showAll ? tr("screens-inbox-show-newest-five") : tr("screens-inbox-show-all", { notices: notices.length })}
        </button>
      )}
    </section>
  );
}

/**
 * A thing with no conversation of its own — a workflow, a project: what
 * happened to it, scrolled back to where it was read. Mounted with the
 * row's key as its `key`, so another row is another detail with its own
 * place. The way to the thing is the header's own door (*Open workflow*,
 * *Open in the IDE*); the empty state names it rather than drawing a
 * second, vaguer one.
 */
function QuietDetail({ row, channelKind }: { row: InboxRow; channelKind: (id: string) => "channel" | "dm" | null }) {
  const root = useRef<HTMLDivElement>(null);
  useViewScroll(root, rowPlace(PLACE, row.key));
  return (
    <div ref={root} className="h-full">
      <div data-scroll-keep="detail" className="flex h-full flex-col overflow-y-auto">
        <WhatHappened row={row} channelKind={channelKind} />
        <div className="flex flex-1 items-center justify-center p-6">
          <EmptyState
            icon={ICON[rowGlyph(row)]}
            title={tr("screens-inbox-nothing-say-here")}
            hint={tr("screens-inbox-has-no-conversation-own-what-happened", { kind: kindLabel(row.kind).toLowerCase(), door: doorLabel(row.kind) })}
            action={null}
          />
        </div>
      </div>
    </div>
  );
}

export default function Inbox() {
  const toast = useToast();
  const ws = useWorkspace();
  /**
   * Every row the node knows, unfiltered — see the note at the top. What the
   * window kept of them is drawn at once, so the list is never empty on the
   * way back, and the node is read behind it.
   */
  const [rows, setRows] = useState<InboxRow[]>(() => keptRead<InboxRow[]>(ROWS_READ) ?? []);
  const [loaded, setLoaded] = useState(() => keptRead<InboxRow[]>(ROWS_READ) !== undefined);
  // However the rows moved — a read, a frame, a mark — the window keeps them as they stand.
  useEffect(() => {
    if (loaded) keepRead(ROWS_READ, rows);
  }, [loaded, rows]);
  const [refreshing, setRefreshing] = useState(false);
  const [at, setAt] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [rawFilter] = useSearchValue("filter");
  const [rawSource] = useSearchValue("source");
  const [selected] = useSearchValue("item");
  const filter: FilterId = filterOf(rawFilter);
  const source: SourceId = sourceIdOf(rawSource);
  const setFilter = useCallback((f: FilterId) => setSearch({ filter: f === "needs_you" ? null : f }, { replace: true }), []);
  const setSource = useCallback((s: SourceId) => setSearch({ source: s === "any" ? null : s }, { replace: true }), []);
  const select = useCallback((key: string | null) => setSearch({ item: key }, { replace: true }), []);
  const pending = useRef<ReturnType<typeof setTimeout> | null>(null);
  /** The number of the last read asked for: an older read's answer, landing later, is not drawn (`isNewestRead`). */
  const reads = useRef(0);
  // The list comes back where it was scrolled, and the row picked comes
  // back in view — once, on a frame of its own, after the scroll was put
  // back: a list left scrolled away from its row would hide what the detail
  // beside it is about. A row already in view moves nothing.
  const list = useRef<HTMLDivElement>(null);
  /** The list's own `<nav>`: where its keys act (`keyAction`'s `onControl`). */
  const listNav = useRef<HTMLElement>(null);
  useViewScroll(list, PLACE);
  const revealed = useRef(false);
  useEffect(() => {
    if (revealed.current || !loaded || !selected) return;
    const frame = requestAnimationFrame(() => {
      revealed.current = true;
      list.current?.querySelector<HTMLElement>('[aria-current="true"]')?.scrollIntoView({ block: "nearest" });
    });
    return () => cancelAnimationFrame(frame);
  }, [loaded, selected]);

  const load = useCallback(async (signal?: AbortSignal) => {
    const asked = ++reads.current;
    setRefreshing(true);
    try {
      const { rows: next } = await api.inbox(undefined, signal);
      if (signal?.aborted || !isNewestRead(asked, reads.current)) return;
      setRows(next);
      setAt(Math.floor(Date.now() / 1000));
      setError(null);
      setLoaded(true);
    } catch (e) {
      if (signal?.aborted || !isNewestRead(asked, reads.current)) return;
      setError(e instanceof ApiError ? e.message : tr("screens-inbox-could-not-read-inbox"));
    } finally {
      if (!signal?.aborted && isNewestRead(asked, reads.current)) setRefreshing(false);
    }
  }, []);

  useEffect(() => {
    const ctrl = new AbortController();
    void load(ctrl.signal);
    return () => ctrl.abort();
  }, [load]);

  // Live. An `inbox` frame carries one row's whole state, so the row is
  // patched where it sits — no refetch, no new array, and nothing moves
  // under the row somebody is reading. A frame that counts a question or a
  // notice the row does not hold, or names a row nobody holds, is the one
  // case that still needs the list. Everything else is debounced and only
  // for frames that move the needle.
  useEffect(() => {
    const schedule = () => {
      if (pending.current) return;
      pending.current = setTimeout(() => {
        pending.current = null;
        void load();
      }, RELOAD_DEBOUNCE_MS);
    };
    const off = busSubscribe({}, (frame) => {
      if (frame.stream === "inbox") {
        const f = frame.payload;
        setRows((prev) => {
          if (needsReload(prev, f)) schedule();
          return applyDelta(prev, f);
        });
        return;
      }
      if (wantsList(frame)) schedule();
    });
    return () => {
      off();
      if (pending.current) clearTimeout(pending.current);
    };
  }, [load]);

  // What a node forgot in a restart — a gate it held, a harness that waited —
  // and what it withdrew at boot reach the list by no frame.
  useReloadOnReconnect(() => void load());

  const visible = useMemo(() => visibleRows(rows, filter, source), [rows, filter, source]);
  // The list emptied while the person worked through it — in this filter, from this source.
  const cleared = useCleared(visible.length, `${filter}:${source}`, loaded);
  const groups = useMemo(() => groupRows(visible), [visible]);
  const tally = useMemo(() => counts(rows, filter), [rows, filter]);

  // The selection never jumps. It is chosen when there is none and replaced
  // only when the thing has left the workspace entirely — never because a
  // filter stopped matching it, and never because you read it.
  useEffect(() => {
    if (!loaded) return;
    const next = nextSelection(visible, rows, selected ?? null);
    if (next !== selected) setSearch({ item: next }, { replace: true });
  }, [loaded, rows, visible, selected]);

  // Looked up in the unfiltered set, so switching to "Needs you" leaves the
  // thing you were reading on the right-hand side.
  const current = useMemo(() => rows.find((r) => r.key === selected) ?? null, [rows, selected]);
  // Read, empty in this filter, and nothing open: no detail beside it.
  const lone = loaded && visible.length === 0 && !current;
  const conversation = current ? conversationOf(current) : null;

  // What a conversation row is about, named: the goal's title, the project's
  // or the workstream's name — the id's tail when the list has moved on
  // (`aboutWords`); a workflow's name is nobody's here, so its tail.
  const names = useMemo(
    () => ({
      goal: (id: string) => ws.goals.find((g) => g.id === id)?.title ?? null,
      project: (id: string) => ws.projects.find((p) => p.project.id === id)?.project.name ?? null,
      workstream: (id: string) => ws.workstreams.find((w) => w.workstream.id === id)?.workstream.name ?? null,
    }),
    [ws.goals, ws.projects, ws.workstreams],
  );

  /**
   * The channel behind the selected row, when it is one: the roster's
   * ordering of the `@`-picker, the channel's own handle in it, and a direct
   * channel's tray seeded with the agent you are talking to.
   */
  const currentChannel = useMemo(() => channelOfRow(current, ws.channels, ws.dms), [current, ws.channels, ws.dms]);

  const channelKind = useCallback((id: string): "channel" | "dm" | null => scopeKindOf(id, ws.channels, ws.dms), [ws.channels, ws.dms]);

  const setRead = useCallback(
    async (row: InboxRow, read: boolean) => {
      // Marked here first (`inboxModel.withReadMark`); the node's `inbox` frame for the
      // marker follows, to this list and to the sidebar's, so nothing is read again.
      setRows((prev) => withReadMark(prev, row.key, read));
      try {
        await (read ? api.markRead(row.key) : api.markUnread(row.key));
      } catch (e) {
        log.warn("inbox", "a row could not be marked", { row: row.key, read, ...errorFields(e) });
        toast.error(tr("screens-inbox-could-not-update-read-state"));
        // The mark did not land: the list says what is so.
        await load();
      }
    },
    [load, toast],
  );

  // A row with no conversation of its own — a workflow, a project — is read
  // by being looked at: after a beat, so a `j` on the way past reads nothing.
  useEffect(() => {
    if (!current || current.read || !readOnSelect(current)) return;
    const row = current;
    const t = setTimeout(() => void setRead(row, true), READ_AFTER_MS);
    return () => clearTimeout(t);
  }, [current, setRead]);

  const markAllRead = async () => {
    const keys = unreadKeys(visible);
    if (keys.length === 0) return;
    try {
      // A few at a time (`markBatches`): a full Inbox is not a burst of a thousand requests.
      for (const batch of markBatches(keys)) await Promise.all(batch.map((k) => api.markRead(k)));
      toast.ok(keys.length === 1 ? tr("screens-inbox-marked-read") : tr("screens-inbox-marked-read-2", { keys: keys.length }));
    } catch (e) {
      log.warn("inbox", "the visible rows could not be marked read", errorFields(e));
      toast.error(tr("screens-inbox-could-not-mark-them-read"));
    } finally {
      // Whichever marks landed, the list says what is so.
      ws.refresh();
      await load();
    }
  };

  const move = useCallback(
    (delta: number) => {
      if (visible.length === 0) return;
      const i = visible.findIndex((r) => r.key === selected);
      const next = Math.min(visible.length - 1, Math.max(0, (i < 0 ? 0 : i) + delta));
      select(visible[next]!.key);
    },
    [visible, select, selected],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // A control that took the key for itself — a menu's trigger opening on
      // Enter, a menu closing on Escape — has answered it.
      if (e.defaultPrevented) return;
      const el = e.target as HTMLElement | null;
      // Where focus stands: the list's keys are the list's — a row of it, the
      // list itself, or the page with nothing focused. A tab, a filter, a
      // menu, a button in the detail keeps its own keys.
      const focus = document.activeElement as HTMLElement | null;
      const inList = !!focus && (focus === listNav.current || focus.closest("[data-inbox-row]") !== null);
      const onControl = !!focus && focus !== document.body && !inList;
      const action = keyAction(e.key, {
        inInput: !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable),
        // The pinned answer form has buttons in it — Send, "I'm not sure" —
        // and a focused button is not an input; scoped to the card rather
        // than to buttons in general, because the list's rows are buttons.
        inAsk: !!el?.closest(`[${ASK_ATTR}]`),
        modifier: e.metaKey || e.ctrlKey || e.altKey,
        hasSelection: current !== null,
        onControl,
      });
      if (!action) return;
      e.preventDefault();
      switch (action) {
        case "next":
          move(1);
          break;
        case "prev":
          move(-1);
          break;
        case "open":
          if (current) open(current);
          break;
        case "focus_ask":
          document.querySelector<HTMLElement>(`[${FOCUS_ATTR}]`)?.focus();
          break;
        case "mark_read":
          if (current) void setRead(current, true);
          break;
        case "mark_unread":
          if (current) void setRead(current, false);
          break;
        case "clear":
          select(null);
          break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [current, move, select, setRead]);

  /**
   * One array behind the row's ⋮ menu, its right-click menu and the header,
   * so the same verbs appear in the same order wherever you reach for them.
   * Read and unread are two glyphs rather than one toggled state, so the
   * item shows which way the action goes.
   */
  const actionsFor = (row: InboxRow): MenuItem[] => [
    { label: tr("screens-inbox-open"), onSelect: () => open(row), icon: ICON.open },
    row.read
      ? { label: tr("screens-inbox-mark-unread"), onSelect: () => void setRead(row, false), icon: ICON.unread }
      : { label: tr("screens-inbox-mark-read"), onSelect: () => void setRead(row, true), icon: ICON.read },
  ];

  const empty = emptyWords(filter, source);

  const listStatus = readWords({ what: tr("screens-inbox-inbox-2"), refreshing: refreshing && loaded, error: loaded ? error : null, at, data: loaded ? rows : null }, Date.now() / 1000);
  const unreadInView = unreadKeys(visible).length;

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* Two controls, because they answer two questions: where a row comes
          from, and what is owed or new. One row across the screen, as the
          Pulse's, so five words with their glyphs and counts always have
          room; the counts say what pressing would show. */}
      <ScreenBar
        tabs={
          <Tabs
            bare
            className="flex-1"
            tabs={sourceTabs(tally.sources).map((t) => ({
              id: t.id,
              label: t.label,
              icon: ICON[t.icon],
              count: t.count,
              badge: t.count > 0 ? <CountBadge count={t.count} tone="quiet" /> : undefined,
            }))}
            active={source}
            onChange={(id) => setSource(id as SourceId)}
          />
        }
        end={
          <>
            <SegmentedControl size="sm" label={tr("screens-inbox-filter-inbox")} options={filterSegments(tally.buckets)} value={filter} onChange={setFilter} />
            {unreadInView > 0 && (
              <Button size="sm" variant="ghost" onClick={() => void markAllRead()} title={tr("screens-inbox-mark-every-row-view-read")}>
                <ICON.read size={12} aria-hidden />
                <span className="ml-1">{tr("screens-inbox-mark-all-read")}</span>
              </Button>
            )}
          </>
        }
      />

      <div className="flex min-h-0 flex-1">
        {/* An empty list with nothing open has nothing to read beside it: the list takes the screen, its empty state the one thing on it. */}
        <div ref={list} className={cn("flex flex-col", lone ? "min-w-0 flex-1" : "w-[clamp(16rem,38%,22rem)] shrink-0 border-r border-border")}>
          <nav ref={listNav} aria-label={tr("screens-inbox-inbox")} data-scroll-keep="list" className="min-h-0 flex-1 overflow-y-auto">
            {!loaded && error && (
              <div className="p-3">
                <ErrorNote error={error} retry={() => void load()} />
              </div>
            )}
            {!loaded && !error && <Pending what={tr("screens-inbox-inbox-2")} rows={8} className="p-3" />}
            {loaded && listStatus?.failed && (
              <div className="px-3 pt-2">
                <ReadLine words={listStatus} busy={refreshing} onReload={() => void load()} />
              </div>
            )}
            {loaded && visible.length === 0 && (
              <div className="p-3">
                <EmptyState
                  icon={ICON.inbox}
                  title={empty.title}
                  hint={empty.hint}
                  cleared={cleared}
                  action={
                    filter === "all" && source === "any" ? (
                      <Button variant="ghost" onClick={() => navigate({ name: "goals" })}>{tr("screens-agents-see-all-goals")}</Button>
                    ) : (
                      <Button
                        variant="ghost"
                        onClick={() => {
                          setFilter("all");
                          setSource("any");
                        }}
                      >{tr("screens-inbox-show-everything")}</Button>
                    )
                  }
                />
              </div>
            )}
            {loaded &&
              groups.map((g) => (
                <section key={g.id} aria-label={g.title} className="pb-1">
                  <div className="px-2 pt-1">
                    <SectionHeader title={g.title} count={g.rows.length} />
                  </div>
                  <AnimatedList
                    items={g.rows}
                    keyOf={(row) => row.key}
                    render={(row) => {
                      const active = row.key === selected;
                      const line = summarize(row);
                      const Glyph = glyphFor(line.icon);
                      // A conversation about a thing wears that thing's glyph and says so; every other row its kind's.
                      const Kind = ICON[rowGlyph(row)];
                      const about = aboutWords(row, names);
                      const state = rowState(row);
                      const unread = state !== "read";
                      const fresh = row.unread_count + unreadNoticesOf(row);
                      return (
                        <ContextMenu items={actionsFor(row)} className="block">
                          <div className="group relative" data-inbox-row>
                            {/* A read row recedes by its words — dim ink, normal weight — never by fading them. */}
                            <button
                              type="button"
                              aria-current={active ? "true" : undefined}
                              onClick={() => select(row.key)}
                              onDoubleClick={() => open(row)}
                              className={`anim min-h-row-lg w-full px-3 py-2 text-left ${active ? "bg-selected text-text" : "hover:bg-surface-2/60"}`}
                            >
                              <div className="flex items-center gap-1.5">
                                {/* Three states, one slot: an accent dot for
                                    something owed, a plain dot for merely new,
                                    and an opened envelope for read. */}
                                {state === "waiting" ? (
                                  <Dot title={tr("screens-inbox-waiting")} />
                                ) : state === "unread" ? (
                                  <Dot tone="neutral" title={fresh > 0 ? tr("screens-inbox-new-2", { fresh }) : tr("screens-inbox-mark-unread-word")} />
                                ) : (
                                  <ICON.read size={11} aria-label={tr("screens-inbox-mark-read-word")} className="shrink-0 text-text-dim" />
                                )}
                                <Kind size={12} aria-label={kindLabel(row.kind)} className="shrink-0 text-text-dim" />
                                <span title={row.title} className={`truncate text-xs ${unread ? "font-semibold" : "font-normal text-text-dim"}`}>{row.title}</span>
                                {about && <span title={about} className="hidden min-w-0 shrink truncate text-2xs text-text-dim sm:inline">{about}</span>}
                                <RelativeTime at={row.latest_at} className="tnum ml-auto shrink-0 text-2xs text-text-dim" />
                              </div>
                              <div className="mt-0.5 flex items-center gap-1.5">
                                {Glyph && <Glyph size={11} aria-hidden className={`shrink-0 ${line.tone === "accent" ? "text-accent-ink" : isHandled(row) ? "text-ok" : "text-text-dim"}`} />}
                                <span className={`truncate text-2xs ${LINE_TONE[line.tone] ?? "text-text-dim"}`}>{line.text}</span>
                                {(() => {
                                  const g = row.kind === "goal" ? ws.goals.find((x) => x.id === row.key) : undefined;
                                  return g ? <CurrentStepPill row={g} /> : null;
                                })()}
                                {fresh > 1 && <CountBadge count={fresh} tone="neutral" />}
                              </div>
                              {row.representative && (
                                <p className="mt-0.5 flex items-baseline gap-1 truncate text-2xs text-text-dim">
                                  <span className="truncate">
                                    {ws.nameOf(row.representative.author)}: {row.representative.snippet}
                                  </span>
                                  <RelativeTime at={row.representative.at} className="tnum shrink-0" />
                                </p>
                              )}
                            </button>
                            <div className="row-actions anim absolute top-1.5 right-2">
                              <MoreMenu label={tr("screens-inbox-actions", { title: row.title })} items={actionsFor(row)} className="h-5 w-5 rounded-control bg-surface hover:bg-surface-2" />
                            </div>
                          </div>
                        </ContextMenu>
                      );
                    }}
                  />
                </section>
              ))}
          </nav>
          <p className="shrink-0 border-t border-hairline px-3 py-1 text-2xs text-text-dim">
            {rich("screens-inbox-keys-legend")}
          </p>
        </div>

        {!lone && <div className="flex min-w-0 flex-1 flex-col">
          {!current ? (
            // Beside an empty list there is nothing to pick: the list's own empty state says it all.
            visible.length > 0 && (
              <div className="flex h-full items-center justify-center p-6">
                <EmptyState icon={ICON.inbox} title={tr("screens-inbox-nothing-selected")} hint={tr("screens-inbox-pick-row-left-j-k-move")} action={null} />
              </div>
            )
          ) : (
            <>
              <ConversationHeader
                icon={(() => {
                  const Kind = ICON[rowGlyph(current)];
                  return (
                    <span className="flex size-6.5 items-center justify-center rounded-full border border-border bg-surface-2 text-text-dim">
                      <Kind size={14} aria-hidden />
                    </span>
                  );
                })()}
                title={current.title}
                subtitle={[kindLabel(current.kind), aboutWords(current, names)].filter(Boolean).join(" · ")}
                chips={
                  <>
                    {(() => {
                      const g = current.kind === "goal" ? ws.goals.find((x) => x.id === current.key) : undefined;
                      return g ? <HolderBadge holder={g.holder} strip={g.strip} /> : null;
                    })()}
                    {needsOf(current).length > 0 && (
                      <Chip tone="accent" icon={ICON.waiting}>
                        {needsOf(current).length === 1 ? tr("screens-inbox-waiting") : tr("screens-inbox-waiting-2", { needsOf: needsOf(current).length })}
                      </Chip>
                    )}
                    {unreadNoticesOf(current) > 0 && <Chip tone="accent">{tr("screens-inbox-new", { unread: unreadNoticesOf(current) })}</Chip>}
                    {isHandled(current) && (
                      <Chip tone="ok" icon={ICON.ok}>
                        {summarize(current).text}
                      </Chip>
                    )}
                  </>
                }
                actions={
                  <>
                    <Button size="sm" onClick={() => open(current)}>
                      {doorLabel(current.kind)}
                    </Button>
                    <MoreMenu label={tr("screens-inbox-row-actions")} items={actionsFor(current)} className="h-7 w-7 rounded-control border border-border hover:bg-surface-2" />
                  </>
                }
              />

              <div className="min-h-0 flex-1">
                {waitingOf(current) && <WaitingSessionCard row={current} />}
                {joinOf(current) && (
                  <JoinCard
                    row={current}
                    onDone={() => {
                      ws.refresh();
                      void load();
                    }}
                  />
                )}
                {conversation ? (
                  <Chat
                    key={current.key}
                    scope={conversation.scope}
                    kind={conversation.kind}
                    channel={currentChannel}
                    placeholder={tr("screens-inbox-reply")}
                    pinned={
                      needsOf(current).length > 0 || noticesOf(current).length > 0 ? (
                        <>
                          {needsOf(current).map((a, i) => (
                            <NeedsActionCard
                              key={needsKey(a)}
                              action={a}
                              autoFocus={i === 0}
                              onResolved={() => {
                                ws.refresh();
                                void load();
                              }}
                            />
                          ))}
                          <WhatHappened row={current} channelKind={channelKind} />
                        </>
                      ) : undefined
                    }
                  />
                ) : (
                  <QuietDetail key={current.key} row={current} channelKind={channelKind} />
                )}
              </div>
            </>
          )}
        </div>}
      </div>
    </div>
  );
}
