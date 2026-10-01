/**
 * Windowing — enough for feeds and message lists, which are the only places
 * the app can reach thousands of rows.
 *
 * Two modes, and the difference matters:
 *
 * - **Fixed** (the default). Every row is `rowHeight` tall, so an offset is a
 *   multiplication and nothing is ever measured. This is the right mode
 *   whenever it is true; the file tree has tens of thousands of rows and
 *   should never pay for anything else.
 * - **Measured** (`measure`). Rows size themselves and report back through a
 *   `ResizeObserver`; `rowHeight` becomes the estimate used until one has.
 *   This exists because a row that can be *opened* is a row whose height is
 *   not knowable in advance, and the alternatives are worse: a fixed height
 *   large enough for the longest detail wastes a screen on every collapsed
 *   row, and dropping virtualization to get variable heights trades a bounded
 *   cost for one that grows with the workspace.
 *
 * Measured mode is O(n) per height change over the *rendered* item list, not
 * the DOM, and heights are cached by key — so scrolling back over a row that
 * has already been measured costs nothing.
 *
 * The window is `virtualListModel.mjs`'s arithmetic over a **measured**
 * viewport: the scroll element is one `div` with a callback ref, rendered in
 * every state (an `empty` node draws inside it), observed from the element
 * itself before paint — never a mount effect that could miss it, never a
 * guessed height. An `unbounded` list is as tall as its rows and no scroll
 * container: it windows against the nearest ancestor marked
 * `data-scrollport` — its scroll, its height, and where the list starts in
 * it. When the total shrinks under a deep scroll, the scroll is clamped in
 * the same commit.
 *
 * A list that **grows at its head** — a feed — keeps its place as a row, not
 * as pixels (`keepAnchor`, `anchorModel.mjs`): the first row in view and how
 * far into it the top edge stood, read once when the list mounts and put
 * back as the rows arrive and are measured. As `useKeptScroll` does, the
 * restore gives up the moment the person scrolls, clicks or types, or when
 * the grace runs out: it never fights the hand on the wheel.
 */

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { anchorOf, firstVisible, scrollFor, type Anchor, type AnchorRow } from "./anchorModel.mjs";
import { RESTORE_GRACE_MS } from "./keptScrollModel.mjs";
import { clampScroll, localFrame, spacerHeight, windowAt, windowOf } from "./virtualListModel.mjs";

/** Where a list's anchor lives: the caller's, as a document's places are (`useKeptScroll`). */
export interface KeptAnchor {
  /** The anchor kept for this list; read once, when the list mounts. */
  read: () => Anchor | null;
  /** Keep — or, with null, forget — the list's place. */
  write: (anchor: Anchor | null) => void;
}

/** What says the person has taken over: any of these ends a restore. */
const HANDS = ["wheel", "pointerdown", "keydown", "touchstart"] as const;

export function VirtualList<T>({
  items,
  rowHeight,
  render,
  keyOf,
  overscan = 8,
  stickToBottom = false,
  measure = false,
  className = "",
  empty,
  scrollToIndex = null,
  scrollNonce = 0,
  onRange,
  overlay,
  unbounded = false,
  keepScroll,
  keepAnchor,
}: {
  items: T[];
  /** Row height in fixed mode; the estimate for an unmeasured row otherwise. */
  rowHeight: number;
  render: (item: T, index: number) => ReactNode;
  keyOf: (item: T, index: number) => string;
  overscan?: number;
  /** Keep the viewport pinned to the newest row unless the user scrolled up. */
  stickToBottom?: boolean;
  /** Let rows size themselves. Costs a `ResizeObserver` per list. */
  measure?: boolean;
  className?: string;
  empty?: ReactNode;
  /** Bring this row into view. Read whenever it or `scrollNonce` changes. */
  scrollToIndex?: number | null;
  /** Bump to jump to the same index again. */
  scrollNonce?: number;
  /** The rendered window, `[first, last)` — what a sparse list fills in. */
  onRange?: (first: number, last: number) => void;
  /**
   * Drawn once over the rows, in the list's own pixels — a drop indicator
   * that slides between rows rather than remounting inside each. Given a
   * row's top and height by index, in either mode.
   */
  overlay?: (geom: { topOf: (index: number) => number; heightOf: (index: number) => number }) => ReactNode;
  /** As tall as its rows and no scroll container: windowed against the nearest ancestor `[data-scrollport]`. */
  unbounded?: boolean;
  /** Name the list's scrollport so a document keeps its place across an unmount (`useKeptScroll`); nothing for an unbounded list, which scrolls nothing. */
  keepScroll?: string;
  /** Keep the list's place as a row and an offset into it — for a list that grows at its head, where pixels land on another row. Key the list by what it shows: the anchor is read once a mount. Nothing for an unbounded list. */
  keepAnchor?: KeptAnchor;
}) {
  // A callback ref, so the observers follow the element wherever it mounts.
  const [node, setNode] = useState<HTMLDivElement | null>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [height, setHeight] = useState(0);
  // An unbounded list's top inside the panel that scrolls it.
  const [offsetTop, setOffsetTop] = useState(0);
  const pinned = useRef(true);

  // Measured heights, by row key — so a row keeps its height across a filter
  // change or a refetch that reorders the list, and is measured once.
  const measured = useRef(new Map<string, number>());
  const [measureEpoch, setMeasureEpoch] = useState(0);

  // Created in the initializer rather than an effect: an effect runs *after*
  // the first render, so the first screenful of rows would attach to nothing
  // and never report a height.
  const [observer] = useState(() => {
    if (!measure || typeof ResizeObserver === "undefined") return null;
    const keys = new WeakMap<Element, string>();
    const ro = new ResizeObserver((entries) => {
      let changed = false;
      for (const entry of entries) {
        const key = keys.get(entry.target);
        if (key === undefined) continue;
        const h = entry.contentRect.height;
        // A row mid-unmount reports 0; taking it would collapse the list and
        // then immediately re-expand it, which reads as the feed flickering.
        if (h <= 0) continue;
        if (Math.abs((measured.current.get(key) ?? -1) - h) > 0.5) {
          measured.current.set(key, h);
          changed = true;
        }
      }
      if (changed) setMeasureEpoch((n) => n + 1);
    });
    return { ro, keys };
  });

  useEffect(() => () => observer?.ro.disconnect(), [observer]);

  const attach = useCallback(
    (key: string) => (el: HTMLDivElement | null) => {
      if (!el || !observer) return;
      observer.keys.set(el, key);
      observer.ro.observe(el);
      return () => observer.ro.unobserve(el);
    },
    [observer],
  );

  // The viewport, observed from the element itself and read before paint —
  // the element is the scroll box, or for an unbounded list the panel that
  // scrolls it, whose scroll and size are the frame the window is cut from.
  useLayoutEffect(() => {
    if (!node) return;
    const scroller = unbounded ? ((node.closest("[data-scrollport]") as HTMLElement | null) ?? node.parentElement) : node;
    if (!scroller) return;
    const read = () => {
      setHeight(scroller.clientHeight);
      if (unbounded) {
        setOffsetTop(node.getBoundingClientRect().top - scroller.getBoundingClientRect().top + scroller.scrollTop);
        setScrollTop(scroller.scrollTop);
      }
    };
    read();
    const ro = new ResizeObserver(read);
    ro.observe(scroller);
    if (unbounded) ro.observe(node);
    const onPanelScroll = () => setScrollTop(scroller.scrollTop);
    if (unbounded) scroller.addEventListener("scroll", onPanelScroll, { passive: true });
    return () => {
      ro.disconnect();
      if (unbounded) scroller.removeEventListener("scroll", onPanelScroll);
    };
  }, [node, unbounded]);

  // --- the anchor: the place as a row ---------------------------------------
  const anchorHands = useRef(keepAnchor);
  anchorHands.current = keepAnchor;
  const anchored = keepAnchor !== undefined && !unbounded;
  // The anchor being put back: `undefined` until it is read, `null` once the
  // restore is over — and only then is a scroll the person's place.
  const wanted = useRef<Anchor | null | undefined>(undefined);
  // Whether the anchor's row was ever found, and how many rows there were
  // when the grace last started.
  const found = useRef(false);
  const arrived = useRef(0);
  const grace = useRef(0);
  // The rows as they were laid out by the last render, for a scroll that
  // happens between two.
  const rowsBetween = useRef<(from: number, to: number) => AnchorRow[]>(() => []);
  const drawn = useRef<[number, number]>([0, 0]);
  // The place is read when the scroll happens and only *written* on the
  // frame, with the writer it was read for: by the time an unmount's cleanup
  // flushes, the element is detached and reads 0.
  const waiting = useRef<{ anchor: Anchor | null; write: KeptAnchor["write"] } | null>(null);
  const anchorFrame = useRef(0);
  const flushAnchor = useCallback(() => {
    anchorFrame.current = 0;
    const noted = waiting.current;
    waiting.current = null;
    if (noted) noted.write(noted.anchor);
  }, []);
  const endRestore = useCallback(() => {
    window.clearTimeout(grace.current);
    grace.current = 0;
    if (!wanted.current) return;
    wanted.current = null;
    // An anchor whose row never came is no place: the next visit does not look for it again.
    if (!found.current) anchorHands.current?.write(null);
  }, []);
  const startGrace = useCallback(() => {
    window.clearTimeout(grace.current);
    grace.current = window.setTimeout(endRestore, RESTORE_GRACE_MS);
  }, [endRestore]);
  const noteAnchor = useCallback(
    (el: HTMLDivElement) => {
      const hands = anchorHands.current;
      // A scroll the restore itself made is not the person's place.
      if (!hands || wanted.current !== null) return;
      const top = el.scrollTop;
      let rows = rowsBetween.current(drawn.current[0], drawn.current[1]);
      // A jump past the window drawn — a dragged scrollbar — is looked up in the whole list.
      if (rows.length === 0 || rows[0]!.top > top || firstVisible(rows, top) < 0) rows = rowsBetween.current(0, Number.MAX_SAFE_INTEGER);
      waiting.current = { anchor: anchorOf(rows, top), write: hands.write };
      if (!anchorFrame.current) anchorFrame.current = requestAnimationFrame(flushAnchor);
    },
    [flushAnchor],
  );

  const onScroll = useCallback(() => {
    if (!node) return;
    setScrollTop(node.scrollTop);
    pinned.current = node.scrollHeight - node.scrollTop - node.clientHeight < rowHeight * 2;
    noteAnchor(node);
  }, [node, rowHeight, noteAnchor]);

  useEffect(() => {
    if (!stickToBottom || !pinned.current) return;
    if (node) node.scrollTop = node.scrollHeight;
  }, [node, items.length, stickToBottom]);

  // A jump: the row's top, minus a third of the viewport so it is not glued
  // to the edge. Fixed mode multiplies; measured mode reads the offsets.
  useEffect(() => {
    if (scrollToIndex === null || scrollToIndex === undefined || scrollToIndex < 0) return;
    const el = node;
    if (!el) return;
    const top = offsets ? (offsets[scrollToIndex] ?? 0) : scrollToIndex * rowHeight;
    const view = el.clientHeight;
    if (view <= 0) return;
    if (top >= el.scrollTop && top + rowHeight <= el.scrollTop + view) return; // already on screen
    el.scrollTop = Math.max(0, top - Math.floor(view / 3));
    setScrollTop(el.scrollTop);
    // `offsets` is deliberately not a dependency: a re-measure must not re-jump.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [node, scrollToIndex, scrollNonce, rowHeight]);

  // `offsets[i]` is the top of row `i`; the last entry is the total height.
  // Null in fixed mode, where multiplying is both cheaper and exact.
  const offsets = useMemo(() => {
    if (!measure) return null;
    const out = new Array<number>(items.length + 1);
    let y = 0;
    for (let i = 0; i < items.length; i++) {
      out[i] = y;
      y += measured.current.get(keyOf(items[i]!, i)) ?? rowHeight;
    }
    out[items.length] = y;
    return out;
    // `measureEpoch` is the dependency that matters: the map it reads is a
    // ref, so nothing else here changes when a row reports a new height.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [items, keyOf, measure, measureEpoch, rowHeight]);

  // The window painted this render: the model's arithmetic over the measured
  // frame — the list's own scroll, or the panel's frame for an unbounded list.
  const total = offsets ? offsets[items.length]! : spacerHeight(items.length, rowHeight);
  const frame = unbounded ? localFrame({ scrollTop, viewHeight: height, offsetTop, total }) : { scrollTop, viewHeight: height };
  const { first, last } = offsets ? windowAt({ offsets, ...frame, overscan }) : windowOf({ ...frame, rowHeight, count: items.length, overscan });
  const slice = items.slice(first, last);

  // The window reported after paint and only when it moved, so a sparse list
  // can ask for exactly the window on screen without an effect per scroll pixel.
  const range = useRef<[number, number]>([-1, -1]);
  useEffect(() => {
    if (!onRange) return;
    if (range.current[0] === first && range.current[1] === last) return;
    range.current = [first, last];
    onRange(first, last);
  });

  // The total shrank under a deep scroll (a folder collapsed, a relayout):
  // the scroll is pulled back into it in the same commit.
  useLayoutEffect(() => {
    if (unbounded || !node) return;
    const clamped = clampScroll(node.scrollTop, total, height);
    if (clamped !== node.scrollTop) node.scrollTop = clamped;
    setScrollTop((s) => (s === clamped ? s : clamped));
  }, [node, total, height, unbounded]);

  // The rows as this render lays them out, for the anchor's two readers.
  rowsBetween.current = (from, to) => {
    const rows: AnchorRow[] = [];
    for (let i = Math.max(0, from); i < Math.min(items.length, to); i++) {
      const top = offsets ? (offsets[i] ?? 0) : i * rowHeight;
      rows.push({ key: keyOf(items[i]!, i), top, height: offsets ? (offsets[i + 1] ?? top) - top : rowHeight });
    }
    return rows;
  };
  drawn.current = [first, last];

  // The anchor is put back whenever the layout moved while a restore is on:
  // the rows arrive a page at a time (`items`) and are measured as they are
  // drawn (`offsets`), and each moves where the anchor's row stands. A render
  // the scroll alone caused lays nothing out anew and puts nothing back.
  // Once the restore is over this does nothing.
  useLayoutEffect(() => {
    const hands = anchorHands.current;
    if (!node || !hands || !anchored) return;
    if (wanted.current === undefined) wanted.current = hands.read();
    const anchor = wanted.current;
    if (!anchor || items.length === 0) return;
    const top = scrollFor(anchor, rowsBetween.current(0, items.length));
    if (top === null) {
      // The grace is the list's to arrive in: it starts over while rows still come.
      if (arrived.current !== items.length || !grace.current) {
        arrived.current = items.length;
        startGrace();
      }
      return;
    }
    // Found: the grace starts over, for the rows around it to be measured in.
    if (!found.current || !grace.current) startGrace();
    found.current = true;
    if (Math.abs(node.scrollTop - top) < 1) return;
    node.scrollTop = top;
    setScrollTop(node.scrollTop);
    // `rowsBetween` is a ref this render wrote from `items`, `offsets`,
    // `rowHeight` and `keyOf`: they are the layout, and the dependencies.
  }, [node, anchored, items, offsets, rowHeight, keyOf, startGrace]);

  // The hand ends a restore; an unmount ends it too, and writes the last
  // scroll that was still waiting for its frame.
  useEffect(() => {
    if (!node || !anchored) return;
    for (const hand of HANDS) node.addEventListener(hand, endRestore, true);
    return () => {
      for (const hand of HANDS) node.removeEventListener(hand, endRestore, true);
      window.clearTimeout(grace.current);
      grace.current = 0;
      if (anchorFrame.current) cancelAnimationFrame(anchorFrame.current);
      flushAnchor();
    };
  }, [node, anchored, endRestore, flushAnchor]);

  const showEmpty = items.length === 0 && empty !== undefined && empty !== null;

  return (
    <div ref={setNode} onScroll={unbounded ? undefined : onScroll} data-scroll-keep={unbounded ? undefined : keepScroll} className={`${unbounded ? "overflow-visible" : "overflow-y-auto"} ${className}`}>
      {showEmpty ? (
        empty
      ) : (
      <div style={{ height: total, position: "relative" }}>
        {slice.map((item, i) => {
          const index = first + i;
          const key = keyOf(item, index);
          return (
            <div
              key={key}
              ref={offsets ? attach(key) : undefined}
              style={{
                position: "absolute",
                top: offsets ? offsets[index] : index * rowHeight,
                // Measured rows take the height of their content; forcing one
                // here is exactly what would make an expanded row unreadable.
                height: offsets ? undefined : rowHeight,
                left: 0,
                right: 0,
              }}
            >
              {render(item, index)}
            </div>
          );
        })}
        {overlay?.({
          topOf: (index) => (offsets ? (offsets[index] ?? 0) : index * rowHeight),
          heightOf: (index) => (offsets ? (offsets[index + 1] ?? 0) - (offsets[index] ?? 0) : rowHeight),
        })}
      </div>
      )}
    </div>
  );
}
