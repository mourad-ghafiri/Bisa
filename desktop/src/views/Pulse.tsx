/**
 * Pulse — everything happening across the platform, as sentences, newest
 * first, narrowed to one concept by a tab: All · Workspace · Goals ·
 * Workflows · Projects · Channels · Agents · Node.
 *
 * **One feed, one renderer.** The node keeps an activity index of every
 * fact — a journal fact, a message, an engine fact — and serves it a page
 * at a time by keyset (`GET /pulse?concept=&before=&before_seq=`); every
 * row carries its payload verbatim and `activity.ts` words it with the same
 * function a live frame gets, so a row reads the same the moment it happens
 * and after a reload. The screen never invents a row from a frame: a frame
 * is a **nudge** to read the head page again, debounced, merged by `seq`.
 *
 * **Lazy.** The list draws a window; when the window comes within a few
 * rows of the end of what is loaded and the last page said there is more,
 * the next page is asked for — one in flight at a time, aborted on a
 * concept change. No button, no end but the feed's.
 *
 * **Left as it stood.** What was loaded is kept for the window, a concept
 * at a time, so coming back draws it at once and reads the head behind it.
 * The rows opened, how far the feed was read and where it was scrolled are
 * the screen's memory (`shell/viewMemoryStore`): after a restart the feed is
 * read again that far — ten pages at most — and the list is put back on the
 * row it stood on. The place is a row, never pixels (`VirtualList`'s
 * `keepAnchor`): the feed grows at its head.
 *
 * The rules — the concepts and their words, when a page is wanted, how one
 * joins, the dividers and headings, the door a row opens — are
 * `_pulse/pulseModel.mjs`, tested; this file paints.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ApiError, api } from "../api";
import { useConversationEvents, useEngineEvents } from "../bus";
import { useReloadOnReconnect } from "../ui/useReloadOnReconnect";
import { pulseLine } from "../activity";
import { errorFields, log } from "../log";
import { navigate, useSearchValue } from "../router";
import { useWorkspace } from "../shell/useWorkspaceData";
import { NEW_GOAL, fire } from "../shell/shortcuts";
import { placeOf, useViewState, viewState } from "../shell/viewMemoryStore";
import { wordsOf } from "../shell/viewValuesModel.mjs";
import type { PulseCursor, PulseRow as PulseRowDto } from "../types";
import { Button, DayDivider, EmptyState, ErrorNote, ICON, ScreenBar, SkeletonRows, Tabs, Tooltip, VirtualList, useTokenPx } from "../ui";
import { parseAnchor, type Anchor } from "../ui/anchorModel.mjs";
import { PulseRow, type PulseItem } from "./_pulse/PulseRow";
import { CONCEPTS, NUDGE_MS, PAGE, conceptOfFact, conceptWords, cursorOf, depthOf, heldFeed, itemOf, joinHead, linkOf, mergePage, parseConcept, parseDepth, wantsDepth, wantsMore, wantsNudge, withDividers } from "./_pulse/pulseModel.mjs";
import type { Concept } from "./_pulse/pulseModel.mjs";
import { readKey } from "./_work/keptReadsModel.mjs";
import { keepRead, keptRead } from "./_work/keptReadsStore";
import { parseOpened, toggled } from "./_work/openedModel.mjs";
import { scopeKindOf } from "./_studio/inboxModel.mjs";
import { t } from "../i18n/l10n.mjs";

/** A stored row as the screen holds it (`pulseModel.itemOf`). */
const itemOfRow = (row: PulseRowDto): PulseItem => itemOf(row, pulseLine(row));

/** The rows loaded and the tail's cursor: what the screen holds, and what the window keeps of a concept. */
interface Feed {
  items: PulseItem[];
  next: PulseCursor | null;
}

/** A feed as the screen holds it: with the concept it is of, and whether its head is still being read. */
interface Held {
  concept: Concept;
  feed: Feed;
  loading: boolean;
}

const EMPTY_FEED: Feed = { items: [], next: null };
/** Where the screen keeps its memory. */
const PLACE = placeOf({ name: "pulse" });
/** No row opened: what the screen begins with. */
const NONE_OPENED: ReadonlySet<string> = new Set();
/** The names a concept's depth and its place in the list are kept under. */
const depthName = (concept: Concept): string => `depth:${concept}`;
const anchorName = (concept: Concept): string => `anchor:${concept}`;

export default function Pulse() {
  // A row is the density token's height (`--spacing-row`), so the Compact setting tightens the feed like every other list.
  const rowPx = useTokenPx("--spacing-row", 36);
  const ws = useWorkspace();
  const [rawConcept, setRawConcept] = useSearchValue("concept");
  const concept: Concept = parseConcept(rawConcept);
  // What the window kept of this concept: drawn at once, read again behind.
  const feedKey = readKey("pulse", concept);
  // The rows and the tail's cursor move together (`pulseModel.joinHead`), with
  // the concept they are of and whether its head was read: one state.
  const [state, setState] = useState<Held>(() => {
    const kept = keptRead<Feed>(feedKey);
    return { concept, feed: kept ?? EMPTY_FEED, loading: kept === undefined };
  });
  // Another tab is another feed: what is drawn is its own — what the window
  // kept of it, or nothing — derived here, in the render that changed the
  // tab, so the last tab's rows are never on screen under this one's name.
  const mine = state.concept === concept;
  const feed = mine ? state.feed : (keptRead<Feed>(feedKey) ?? EMPTY_FEED);
  const loading = mine ? state.loading : keptRead<Feed>(feedKey) === undefined;
  const { items, next } = feed;
  // What is held now, for a page that lands between two renders.
  const held = useRef(feed);
  const hold = useCallback(
    (next: Feed) => {
      held.current = next;
      keepRead(feedKey, next);
      setState({ concept, feed: next, loading: false });
    },
    [feedKey, concept],
  );
  // The concept on screen, for a read that lands after the tab moved: it is not this feed's.
  const shown = useRef(concept);
  shown.current = concept;
  const [error, setError] = useState<string | null>(null);
  const [opened, setOpened] = useViewState<ReadonlySet<string>>(PLACE, "opened", NONE_OPENED, parseOpened, wordsOf);
  /** The page in flight, if any — one at a time; a concept change aborts it. */
  const inFlight = useRef<AbortController | null>(null);
  const range = useRef<[number, number]>([0, 0]);
  const nudge = useRef<number | null>(null);
  /** How many rows the feed is being read back to — after a restart; 0 once it is that deep, or when the window kept it. */
  const depth = useRef(0);

  const channelKind = useCallback((id: string): "channel" | "dm" | null => scopeKindOf(id, ws.channels, ws.dms), [ws.channels, ws.dms]);

  /** The head page, read fresh: what is shown is replaced where it overlaps and kept below. */
  const loadHead = useCallback(
    async (signal?: AbortSignal) => {
      try {
        const page = await api.pulse(concept, null, PAGE, signal);
        if (signal?.aborted || shown.current !== concept) return;
        const rows = page.rows.map(itemOfRow);
        // Read from the ref, not in an updater: the join is said in the log, and an updater says nothing.
        const joined = joinHead({ shown: held.current.items, page: rows, next: cursorOf(page), tail: held.current.next });
        if (joined.restarted) log.info("pulse", "more landed than a page holds: the feed starts over from its head", { concept, rows: rows.length });
        // The head grows for as long as the workspace moves: what is held is bounded below what the window draws.
        hold(heldFeed({ items: joined.items, next: joined.next }, range.current[1]));
        setError(null);
      } catch (e) {
        if (signal?.aborted) return;
        log.warn("pulse", "the head page could not be read", { concept, ...errorFields(e) });
        setError(e instanceof ApiError ? e.message : t("screens-pulse-could-not-load-pulse"));
      } finally {
        if (!signal?.aborted) setState((s) => (s.concept === concept && s.loading ? { ...s, loading: false } : s));
      }
    },
    [concept, hold],
  );

  /** The page after the last, when the window is near the end. */
  const loadMore = useCallback(async () => {
    if (next === null || inFlight.current) return;
    const ac = new AbortController();
    inFlight.current = ac;
    try {
      const page = await api.pulse(concept, next, PAGE, ac.signal);
      if (ac.signal.aborted) return;
      hold({ items: mergePage(held.current.items, page.rows.map(itemOfRow), "older"), next: cursorOf(page) });
    } catch (e) {
      if (ac.signal.aborted) return;
      log.warn("pulse", "an older page could not be read", { concept, ...errorFields(e) });
      setError(e instanceof ApiError ? e.message : t("screens-pulse-could-not-load-older-activity"));
    } finally {
      if (inFlight.current === ac) inFlight.current = null;
    }
  }, [concept, next, hold]);

  // A concept is its own feed: what the window kept of it, else nothing —
  // never what was loaded for another. Kept, it is as deep as it was left;
  // read afresh, it is read back as far as the memory says.
  useEffect(() => {
    inFlight.current?.abort();
    inFlight.current = null;
    const kept = keptRead<Feed>(feedKey);
    const begins = kept ?? EMPTY_FEED;
    held.current = begins;
    setState((s) => (s.concept === concept ? s : { concept, feed: begins, loading: kept === undefined }));
    setError(null);
    depth.current = kept ? 0 : (parseDepth(viewState.read(PLACE, depthName(concept))) ?? 0);
    const ac = new AbortController();
    void loadHead(ac.signal);
    return () => ac.abort();
  }, [loadHead, feedKey, concept]);

  // Live: a frame is a nudge to read the head again, never a row of its own.
  // The node recorded the fact before it sent the frame, so the read finds it.
  const nudgeHead = useCallback(() => {
    if (nudge.current !== null) return;
    nudge.current = window.setTimeout(() => {
      nudge.current = null;
      void loadHead();
    }, NUDGE_MS);
  }, [loadHead]);
  useEffect(() => () => window.clearTimeout(nudge.current ?? undefined), []);
  useEngineEvents((e) => {
    // A fact that is in the feed nudges its concept's tab and All; an agent's
    // tokens, a file changing, a session's state are no rows and nudge nobody.
    if (wantsNudge(concept, conceptOfFact(e.payload))) nudgeHead();
  });
  useConversationEvents(() => {
    if (wantsNudge(concept, "channels")) nudgeHead();
  });
  // What a node did at boot — the steps it resumed or failed, the questions
  // it withdrew — is in the feed and was said by no frame this window heard.
  useReloadOnReconnect(nudgeHead);

  const slots = useMemo(() => withDividers(items), [items]);

  // The window moved: near the end, ask for the next page.
  const onRange = useCallback(
    (_first: number, last: number) => {
      range.current = [_first, last];
      if (wantsMore({ last, total: slots.length, next, inFlight: inFlight.current !== null })) void loadMore();
    },
    [slots.length, next, loadMore],
  );
  // A page landed and the window is still near the end — or the feed is not
  // yet as deep as it was left: ask again.
  useEffect(() => {
    const [, last] = range.current;
    const flying = inFlight.current !== null;
    if (wantsMore({ last, total: slots.length, next, inFlight: flying }) || wantsDepth({ loaded: items.length, wanted: depth.current, next, inFlight: flying })) void loadMore();
  }, [slots.length, items.length, next, loadMore]);

  // How far the feed is read is kept for a restart — never while it is
  // still being read back that far, or a quit in the middle would keep less.
  useEffect(() => {
    if (!mine || loading) return;
    if (wantsDepth({ loaded: items.length, wanted: depth.current, next, inFlight: false })) return;
    depth.current = 0;
    viewState.keepQuietly(PLACE, depthName(concept), depthOf(items.length) || null);
  }, [mine, loading, items.length, next, concept]);

  // Where the list stands, as a row: read once by the list when it mounts, written as the person scrolls.
  const anchor = useMemo(
    () => ({
      read: () => parseAnchor(viewState.read(PLACE, anchorName(concept))),
      write: (at: Anchor | null) => void viewState.keepQuietly(PLACE, anchorName(concept), at),
    }),
    [concept],
  );

  const toggle = (key: string) => setOpened((prev) => toggled(prev, key));

  const words = conceptWords(concept);
  const tabs = CONCEPTS.map((c) => ({ id: c, label: conceptWords(c).label, icon: ICON[conceptWords(c).icon] }));

  return (
    <div className="flex h-full min-h-0 flex-col">
      <ScreenBar
        tabs={<Tabs bare className="flex-1" tabs={tabs} active={concept} onChange={(id) => setRawConcept(id === "all" ? null : id)} />}
        end={
          <>
            <Tooltip label={t("screens-pulse-read-again-from-node")}>
              <Button size="icon" variant="ghost" aria-label={t("screens-pulse-refresh")} onClick={() => void loadHead()}>
                <ICON.refresh size={14} aria-hidden />
              </Button>
            </Tooltip>
          </>
        }
      />

      {error && (
        <div className="px-6 pt-3">
          <ErrorNote error={error} retry={() => void loadHead()} />
        </div>
      )}

      {loading ? (
        // Collapsed rows are all the same height, so the placeholder can
        // promise the real shape rather than an indeterminate spin.
        <SkeletonRows rows={12} className="px-6 py-2" />
      ) : (
        <VirtualList
          // Keyed by the concept: another tab is another list, with its own place.
          key={concept}
          keepAnchor={anchor}
          items={slots}
          // The estimate, not a contract: an open row is as tall as what it
          // has to say, and `measure` is what lets the list know.
          rowHeight={rowPx}
          measure
          className="min-h-0 flex-1 px-4"
          keyOf={(s) => (s.slot === "day" ? s.key : s.item.key)}
          onRange={onRange}
          // While a read failed and nothing was ever drawn, the error above is
          // the whole story: an empty state beside it would say *nothing
          // happened*, which nobody knows.
          empty={
            error ? null : (
              <div className="p-4">
                <EmptyState
                  icon={(ICON as Record<string, typeof ICON.file>)[words.icon] ?? ICON.pulse}
                  title={words.empty}
                  hint={words.hint}
                  action={
                    concept === "all" ? (
                      <Button variant="primary" onClick={() => fire(NEW_GOAL)}>{t("screens-goals-new-goal")}</Button>
                    ) : (
                      <Button variant="ghost" onClick={() => setRawConcept(null)}>{t("screens-inbox-show-everything")}</Button>
                    )
                  }
                />
              </div>
            )
          }
          render={(slot) =>
            slot.slot === "day" ? (
              <DayDivider at={slot.at} />
            ) : (
              <PulseRow
                item={slot.item}
                heading={slot.heading}
                name={slot.item.author ? ws.nameOf(slot.item.author) : undefined}
                expanded={opened.has(slot.item.key)}
                hasDoor={linkOf(slot.item.source, channelKind) !== null}
                onOpen={() => {
                  const door = linkOf(slot.item.source, channelKind);
                  if (door) navigate(door);
                }}
                onToggle={() => toggle(slot.item.key)}
              />
            )
          }
        />
      )}
    </div>
  );
}
