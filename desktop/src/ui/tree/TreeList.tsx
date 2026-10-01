/**
 * The tree every tree in the app is drawn with (ide/03): a virtualised
 * `role="tree"` over flat rows, one cursor and — for a caller that keeps one
 * — a selection set, the keyboard from `treeListModel.mjs`, indent guides,
 * and drag-and-drop with a **projected indicator** — one bar per tree that
 * slides to the gap at the depth the row would take for a reorder, a ring
 * easing in on the row for a drop into it, the rows beside the gap parting
 * by a hair, the cursor and the ghost saying when a drop is refused
 * (`treeListModel`: `indicatorStyle`, `neighbourShift`, `dragCue`) — and,
 * from the keyboard, Space lifting the cursor row and the arrows stepping
 * it through the tree's own slots (`dropSlots`, `stepSlot`).
 *
 * The cursor and the selection are the caller's facts. The tree reports a
 * cursor move with whether Shift was held (`onCursor(id, { extend })`), marks
 * the rows in `selected` for assistive technology, and drags every row the
 * payload names (`drag.actives`) — how a click or a key changes the set is
 * decided where the rows come from.
 *
 * It knows nothing about files or projects. A caller hands it rows that say
 * their depth and whether they open, paints each row's content, and answers
 * three questions: what the cursor did (`onCursor`, `onAction`), what a row
 * carries when dragged (`drag.data`), and what to do with a drop
 * (`drag.onDrop`, given the model's plan). The file explorer, the project
 * rail and Git › Changes are the three callers; anything they would otherwise
 * each implement — the arrows, type-ahead, the scroll-to, the drop
 * arithmetic — lives here or in the model so it is written once.
 *
 * The scroll-to is a **request**, resolved when it is asked and never again:
 * a reveal jumps to its row once, and rows moving under it afterwards — a
 * folder opened above it — leave the tree where the person is. Resolving
 * the row's index on every rows change is how a reveal became a standing
 * order that dragged the explorer back to the open file on every expand.
 */

import { useDndMonitor, useDraggable, useDroppable } from "@dnd-kit/core";
import React, { useCallback, useEffect, useId, useMemo, useRef, useState, type CSSProperties, type FocusEvent, type KeyboardEvent, type MutableRefObject, type ReactNode } from "react";
import { cn } from "../cn";
import { pointerOf, useDragCue, useDropHandler, type DragCue, type DragData } from "../dnd";
import { VirtualList } from "../VirtualList";
import { dragCue, dropPlan, dropSlots, indicatorStyle, keyAction, neighbourShift, rowIndexOf, stepSlot, typeAhead, type DropPlan, type TreeKeyAction, type TreeRowLike } from "./treeListModel.mjs";

/** How long a type-ahead run stays open between letters. */
const TYPE_AHEAD_MS = 700;
/** How long a row a drop just put here wears its wash — `--motion-slow`'s order. */
const LANDED_MS = 700;

/** A row's keyboard lifter: the sensor's `onKeyDown`, or null for a row that stays put. */
type Lifter = ((e: React.KeyboardEvent) => void) | null;
/** The keys a keyboard drag steps its slot with. */
const STEP_KEYS = new Set(["ArrowDown", "ArrowUp", "ArrowLeft", "ArrowRight"]);
/** Whether the event that started a drag came from the keyboard. */
const fromKeyboard = (activator: Event | null | undefined) => !!activator && !("clientX" in activator);

export interface TreeRowState {
  cursor: boolean;
  /** In the caller's selection set. */
  selected: boolean;
  dragging: boolean;
  /** An acceptable drag is over this row and would land inside it. */
  dropInside: boolean;
  /** A drop just put this row here — worn for a beat. */
  landed: boolean;
  /**
   * The row's left inset in pixels, for its depth. The row's own element
   * applies it as a margin (`style={{ marginLeft: indent }}`), so its hover
   * and selection wash begins at the row's depth and hugs the row — never the
   * full width of the list. The guides and the reorder bar are placed from the
   * same number independently.
   */
  indent: number;
}

export interface TreeDrag<T extends TreeRowLike> {
  /** What a row carries when dragged; null for a row that stays put. */
  data: (row: T) => DragData | null;
  /** Whether a payload may nest into this row. Absent: any expandable row. */
  canNest?: (data: DragData, target: T) => boolean;
  /** Whether a payload may land among this parent's children (`null` for the top level). Absent: yes. */
  canReorder?: (data: DragData, parent: T | null) => boolean;
  /** Whether a payload from outside this tree is considered at all. Absent: only this tree's own rows. */
  accepts?: (data: DragData) => boolean;
  /** Every row of this tree the payload moves — a selection dragged together. Absent: the dragged row alone. */
  actives?: (data: DragData) => readonly string[];
  /**
   * What lights up: a `bar` in the gap the row would take (a list whose order
   * is a fact — the rail), or the `parent` the row would join (a list sorted
   * by name — the explorer, where only the folder matters).
   */
  indicator?: "bar" | "parent";
  /** A drop landed. `plan.noop` is the caller's to honour. */
  onDrop: (data: DragData, plan: DropPlan) => void;
}

function TreeRowShell<T extends TreeRowLike>({
  zone,
  row,
  indent,
  base,
  guides: drawGuides,
  guideInset,
  guideClassName,
  data,
  state,
  shift,
  lifters,
  enabled,
  children,
}: {
  zone: string;
  row: T;
  indent: number;
  base: number;
  guides: boolean;
  /** Where a level's guide sits inside its column. */
  guideInset: number;
  guideClassName: string;
  data: DragData | null;
  state: TreeRowState;
  /** How far the row parts for the gap a drop would take, in pixels. */
  shift: number;
  /** Where the tree finds each row's keyboard lifter. */
  lifters: MutableRefObject<Map<string, Lifter>>;
  /** The tree takes drops at all. */
  enabled: boolean;
  children: ReactNode;
}) {
  const { setNodeRef: setDrop } = useDroppable({ id: `${zone}/drop/${row.id}`, data: { zone, row: row.id }, disabled: !enabled });
  const {
    setNodeRef: setDrag,
    listeners,
    isDragging,
  } = useDraggable({ id: `${zone}/row/${row.id}`, data: data ? { ...data, zone, treeRow: row.id } : undefined, disabled: data === null });
  // The sensor's keyboard lifter, kept where the tree's own key handler can
  // hand it Space on the cursor row: focus stays on the tree, as
  // `aria-activedescendant` wants, and the row still lifts.
  const lift = data ? ((listeners?.onKeyDown as Lifter | undefined) ?? null) : null;
  useEffect(() => {
    lifters.current.set(row.id, lift);
  });
  useEffect(() => () => void lifters.current.delete(row.id), [lifters, row.id]);
  const guides: ReactNode[] = [];
  if (drawGuides) {
    for (let d = 1; d <= row.depth; d++) {
      guides.push(<span key={d} aria-hidden className={cn("pointer-events-none absolute bottom-0 top-0 w-px", guideClassName)} style={{ left: base + (d - 1) * indent + guideInset }} />);
    }
  }
  // The shift rides the droppable itself: `dnd-kit` measures a droppable
  // net of its own transform, so the hit rect stays put while the row parts.
  return (
    <div ref={setDrop} className="tree-row-shift relative h-full" style={{ "--drop-shift": `${shift}px` } as CSSProperties}>
      <div ref={setDrag} onPointerDown={data ? (listeners?.onPointerDown as ((e: React.PointerEvent) => void) | undefined) : undefined} className={cn("anim h-full", (isDragging || state.dragging) && "opacity-40")}>
        {children}
      </div>
      {guides}
    </div>
  );
}

export function TreeList<T extends TreeRowLike>({
  rows,
  rowHeight,
  measure = false,
  indent = 14,
  indentBase = 6,
  guides = true,
  guideInset = 5,
  guideClassName = "bg-border/60",
  cursor,
  selected,
  onCursor,
  onAction,
  onKeyDown,
  onFocus,
  onBlur,
  render,
  drag,
  label,
  busy = false,
  scrollTo = null,
  beforeRows,
  empty,
  className,
  listClassName,
  fill = false,
  unbounded = false,
  tabIndex = 0,
  keepScroll,
}: {
  rows: readonly T[];
  rowHeight: number;
  /** Let rows size themselves; see `VirtualList`. */
  measure?: boolean;
  /** Pixels per nesting level. */
  indent?: number;
  /** Where a top-level row's content starts. */
  indentBase?: number;
  /** Draw a guide line per nesting level. Off for a list whose rows carry their own structure. */
  guides?: boolean;
  /** Where a level's guide sits inside its column — under the caller's chevron, say. */
  guideInset?: number;
  /** The guide's ink. */
  guideClassName?: string;
  cursor: string | null;
  /** The rows in the caller's selection, for a tree that keeps one; the cursor alone reads as selected without it. */
  selected?: ReadonlySet<string>;
  /** The cursor moved — by a click the caller handled, or by a key; `extend` when Shift was held. */
  onCursor: (id: string, mods: { extend: boolean }) => void;
  /** An open, close or open-all the keyboard asked for. */
  onAction?: (action: Exclude<TreeKeyAction, { kind: "cursor" }>) => void;
  /** A key the tree had no answer for — Enter, a chord. */
  onKeyDown?: (e: KeyboardEvent<HTMLDivElement>) => void;
  onFocus?: () => void;
  /** Focus left the tree altogether (not to a field inside it). */
  onBlur?: () => void;
  render: (row: T, state: TreeRowState) => ReactNode;
  drag?: TreeDrag<T>;
  /** Names the tree for assistive technology. */
  label: string;
  busy?: boolean;
  /** Bring this row into view, once: a new object asks again — a new `id`, or the same one with a bumped `nonce`. */
  scrollTo?: { id: string | null; nonce: number } | null;
  /** Above the rows, inside the tree's frame — a notice about the root. */
  beforeRows?: ReactNode;
  /** In place of the rows when there are none. */
  empty?: ReactNode;
  className?: string;
  listClassName?: string;
  /** Fill the height given rather than capping at a preview's worth. */
  fill?: boolean;
  /** As tall as its rows and never a scrollport of its own — `overflow-visible`, windowed against the panel's `[data-scrollport]` — for a tree inside a panel that scrolls as one (Git › Changes under its sticky composer). */
  unbounded?: boolean;
  tabIndex?: number;
  /** Name the tree's scrollport so a screen keeps its place (`useKeptScroll`); nothing for an unbounded tree, which scrolls nothing. */
  keepScroll?: string;
}) {
  const zone = useId();
  const viewport = useRef<HTMLDivElement>(null);
  const [plan, setPlan] = useState<DropPlan | null>(null);
  const planRef = useRef<DropPlan | null>(null);
  const [activeRows, setActiveRows] = useState<ReadonlySet<string>>(() => new Set());
  /** The payload in flight, for the keyboard's own projection. */
  const activeRaw = useRef<(DragData & { zone?: string; treeRow?: string }) | null>(null);
  /** The drag in flight started from the keyboard: the tree steps its slot itself. */
  const keyboardDrag = useRef(false);
  /** Each row's keyboard lifter, by id. */
  const lifters = useRef(new Map<string, Lifter>());
  /** What the drag is being told over this tree — the cursor's and the ghost's word. */
  const [cue, setCue] = useState<DragCue>("none");
  const cueBoard = useDragCue();
  /** The rows a drop just put here, for a beat. */
  const [landed, setLanded] = useState<ReadonlySet<string>>(() => new Set());
  const landedTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  /** Where the bar last stood, so it fades out in place rather than sliding home. */
  const lastBar = useRef<{ x: number; y: number } | null>(null);
  /** The type-ahead run and when its last letter landed. */
  const typed = useRef({ text: "", at: 0 });
  /** The cursor moved by a key, so the next render scrolls it into view. */
  const byKey = useRef(false);

  const { setNodeRef: setRootRef } = useDroppable({ id: `${zone}/root`, data: { zone, root: true }, disabled: !drag });

  const domId = (id: string) => `${zone}-${id}`;
  const dragOf = useCallback((row: T) => (drag ? drag.data(row) : null), [drag]);

  const settle = (next: DropPlan | null) => {
    const prev = planRef.current;
    const same =
      prev === next ||
      (prev !== null && next !== null && prev.over === next.over && prev.mode === next.mode && prev.parent === next.parent && prev.index === next.index && prev.noop === next.noop);
    if (same) return;
    planRef.current = next;
    setPlan(next);
  };

  /** The tree's rules for a payload, as `dropPlan` asks them. */
  const planOpts = (raw: DragData) => ({
    canNest: (t: T) => (drag?.canNest ? drag.canNest(raw, t) : Boolean(t.expandable)),
    canReorder: (p: T | null) => drag?.canReorder?.(raw, p) ?? true,
  });

  /** A plan, and the word for it — settled together so the cue never lags the bar. */
  const settleWith = (next: DropPlan | null, overTree: boolean) => {
    settle(next);
    setCue(dragCue(next, overTree));
  };

  /** The plan for a drag over one of this tree's droppables. */
  const project = (event: { active: { data: { current?: unknown } }; over: { data: { current?: unknown }; rect: { top: number; height: number } } | null; activatorEvent: Event; delta: { x: number; y: number } }) => {
    if (!drag) return;
    // A keyboard drag has no pointer: its slot is the tree's own (`onKey`).
    if (keyboardDrag.current) return;
    const over = event.over?.data.current as { zone?: string; row?: string; root?: boolean } | undefined;
    if (!over || over.zone !== zone) {
      settleWith(null, false);
      return;
    }
    const raw = event.active.data.current as (DragData & { zone?: string; treeRow?: string }) | undefined;
    if (!raw || typeof raw.type !== "string") return;
    const mine = activesOf(raw);
    if (mine === null && !(drag.accepts?.(raw) ?? false)) {
      settleWith(null, false);
      return;
    }
    if (over.root || typeof over.row !== "string") {
      // The frame itself, below the last row: the top level's end.
      const top = rows.filter((r) => r.depth === 0 && !mine?.includes(r.id)).length;
      settleWith(drag.canReorder?.(raw, null) ?? true ? { parent: null, index: top, mode: "inside", over: "", depth: 0, noop: false } : null, true);
      return;
    }
    const rect = event.over?.rect;
    const ratio = rect && rect.height > 0 ? (pointerOf(event).y - rect.top) / rect.height : 0.5;
    settleWith(dropPlan(rows, mine, over.row, ratio, planOpts(raw)), true);
  };

  /** The rows of this tree a payload moves: the dragged row and, for a selection, its company; null for a foreign payload. */
  const activesOf = (raw: (DragData & { zone?: string; treeRow?: string }) | undefined): string[] | null => {
    if (!raw || raw.zone !== zone || typeof raw.treeRow !== "string") return null;
    const company = drag?.actives?.(raw) ?? [];
    return [...new Set([raw.treeRow, ...company])];
  };

  const endDrag = () => {
    settleWith(null, false);
    setActiveRows(new Set());
    activeRaw.current = null;
    keyboardDrag.current = false;
  };
  useDndMonitor({
    onDragStart: (e) => {
      const raw = e.active.data.current as (DragData & { zone?: string; treeRow?: string }) | undefined;
      const mine = activesOf(raw);
      setActiveRows(new Set(mine ?? []));
      if (mine === null || !raw) return;
      activeRaw.current = raw;
      keyboardDrag.current = fromKeyboard(e.activatorEvent);
      // Lifted by the keyboard: the row starts from where it stands, so the
      // first arrow moves it one slot.
      if (keyboardDrag.current) settleWith(dropSlots(rows, mine, planOpts(raw)).find((s) => s.noop) ?? null, true);
    },
    onDragMove: project,
    onDragOver: project,
    onDragEnd: endDrag,
    onDragCancel: endDrag,
  });

  useDropHandler(zone, (e) => {
    const p = planRef.current;
    if (!drag || !p) return;
    drag.onDrop(e.data, p);
    // The rows that just landed wear their wash for a beat.
    const mine = activesOf(e.data as DragData & { zone?: string; treeRow?: string });
    if (!mine || p.noop) return;
    setLanded(new Set(mine));
    if (landedTimer.current) clearTimeout(landedTimer.current);
    landedTimer.current = setTimeout(() => setLanded(new Set()), LANDED_MS);
  });
  useEffect(() => () => void (landedTimer.current && clearTimeout(landedTimer.current)), []);

  // The cue goes to the ghost too, under this tree's name — and leaves with
  // the tree, so a list unmounted mid-drag never dresses the next drag.
  useEffect(() => {
    cueBoard.report(zone, cue);
    return () => cueBoard.report(zone, "none");
  }, [cueBoard, zone, cue]);

  /** A keyboard drag of this tree's row is in flight: the arrows step its slot. */
  const stepByKey = (e: KeyboardEvent<HTMLDivElement>) => {
    const raw = activeRaw.current;
    if (!raw || !STEP_KEYS.has(e.key)) return;
    e.preventDefault();
    const mine = activesOf(raw) ?? [];
    const next = stepSlot(rows, dropSlots(rows, mine, planOpts(raw)), planRef.current, e.key);
    if (!next) return;
    settleWith(next, true);
    if (next.over) document.getElementById(domId(next.over))?.scrollIntoView({ block: "nearest" });
  };

  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.target !== e.currentTarget) return;
    // A drag of this tree's row is in flight: the arrows are its, and Space
    // or Escape reach the sensor on the document — nothing here swallows them.
    if (activeRows.size > 0) {
      if (keyboardDrag.current) stepByKey(e);
      return;
    }
    // Space on the cursor row lifts it — handed to the sensor unprevented,
    // since it refuses an event something else already took.
    if (e.key === " " && cursor && drag) {
      const lift = lifters.current.get(cursor);
      if (lift) {
        lift(e);
        return;
      }
    }
    const action = keyAction(rows, cursor, e.key);
    if (action) {
      e.preventDefault();
      if (action.kind === "cursor") {
        byKey.current = true;
        onCursor(action.id, { extend: e.shiftKey });
      } else onAction?.(action);
      return;
    }
    // A letter: type-ahead, when the key is a plain printable character.
    if (e.key.length === 1 && !e.metaKey && !e.ctrlKey && !e.altKey && e.key !== " ") {
      const now = Date.now();
      const text = now - typed.current.at < TYPE_AHEAD_MS ? typed.current.text + e.key : e.key;
      typed.current = { text, at: now };
      const hit = typeAhead(rows, cursor, text);
      if (hit) {
        e.preventDefault();
        byKey.current = true;
        onCursor(hit, { extend: false });
        return;
      }
    }
    onKeyDown?.(e);
  };

  const onBlurInner = (e: FocusEvent) => {
    // Focus moving within the tree (to a rename field, say) keeps it ours.
    if (viewport.current?.contains(e.relatedTarget as Node | null)) return;
    onBlur?.();
  };

  // Keep the cursor row on screen after a *keyboard* move. Arrow keys move
  // one row at a time, so the new cursor is inside the overscan already
  // rendered; a reveal is the one jump, and it goes through `scrollToIndex`.
  // A pointer never scrolls: it set the cursor on mousedown, and moving the
  // row under it before mouseup would break the click it is in the middle of.
  useEffect(() => {
    if (!cursor || !byKey.current) return;
    byKey.current = false;
    viewport.current?.querySelector('[data-cursor="true"]')?.scrollIntoView({ block: "nearest" });
  }, [cursor]);

  // Resolved against the rows of the moment the request was made — `rows`
  // is deliberately not a dependency, or a folder opened above the revealed
  // row would re-resolve its index and jump the tree back to it.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const scrollToIndex = useMemo(() => (scrollTo?.id ? rowIndexOf(rows, scrollTo.id) : -1), [scrollTo]);
  const keyOf = useCallback((r: T) => r.id, []);

  const parentIndicator = drag?.indicator === "parent";
  const renderRow = (row: T, index: number) => {
    const cursorHere = cursor === row.id;
    const selectedHere = selected ? selected.has(row.id) : cursorHere;
    const state: TreeRowState = {
      cursor: cursorHere,
      selected: selectedHere,
      dragging: activeRows.has(row.id),
      dropInside: plan !== null && (parentIndicator ? plan.parent === row.id : plan.mode === "inside" && plan.over === row.id),
      landed: landed.has(row.id),
      indent: indentBase + row.depth * indent,
    };
    const shift = parentIndicator ? 0 : neighbourShift(plan, row.id, rows[index - 1]?.id ?? null, rows[index + 1]?.id ?? null);
    return (
      <div
        id={domId(row.id)}
        role="treeitem"
        aria-level={row.depth + 1}
        aria-expanded={row.expandable ? Boolean(row.expanded) : undefined}
        aria-selected={selectedHere}
        data-cursor={cursorHere || undefined}
        data-drop-inside={state.dropInside || undefined}
        className="h-full"
      >
        <TreeRowShell zone={zone} row={row} indent={indent} base={indentBase} guides={guides} guideInset={guideInset} guideClassName={guideClassName} data={dragOf(row)} state={state} shift={shift} lifters={lifters} enabled={drag !== undefined}>
          {render(row, state)}
        </TreeRowShell>
      </div>
    );
  };

  /**
   * The one bar, over the rows: placed on the gap the plan marks and slid
   * there by a transition, faded out where it last stood when the plan has no
   * gap. Only a tree whose order is a fact draws it (`indicator: "bar"`).
   */
  const bar = (geom: { topOf: (i: number) => number; heightOf: (i: number) => number }) => {
    if (!drag || parentIndicator) return null;
    const i = plan ? rowIndexOf(rows, plan.over) : -1;
    const pos = i === -1 ? null : indicatorStyle(plan, { indent, base: indentBase, rowTop: geom.topOf(i), rowHeight: geom.heightOf(i) });
    if (pos) lastBar.current = pos;
    const at = pos ?? lastBar.current;
    return (
      <span
        aria-hidden
        className="tree-drop-bar"
        data-show={pos ? "true" : "false"}
        style={at ? { transform: `translate(${at.x}px, ${at.y - 1}px)`, width: `calc(100% - ${at.x + 4}px)` } : undefined}
      />
    );
  };

  const rootTarget = plan !== null && (parentIndicator ? plan.parent === null : plan.over === "");

  return (
    <div
      ref={(el) => {
        viewport.current = el;
        setRootRef(el);
      }}
      role="tree"
      aria-label={label}
      aria-busy={busy || undefined}
      aria-multiselectable={selected ? true : undefined}
      aria-activedescendant={cursor ? domId(cursor) : undefined}
      tabIndex={tabIndex}
      onKeyDown={onKey}
      onFocus={onFocus}
      onBlur={onBlurInner}
      data-drop-cue={cue === "none" ? undefined : cue}
      // `select-none`: a Shift-click extends the selection, never the text.
      className={cn("flex min-w-0 select-none flex-col outline-none", fill && "min-h-0 flex-1", rootTarget && "ring-1 ring-inset ring-accent", className)}
    >
      {beforeRows}
      <VirtualList items={rows as T[]} rowHeight={rowHeight} measure={measure} keyOf={keyOf} render={renderRow} overlay={bar} empty={empty} scrollToIndex={scrollToIndex} scrollNonce={scrollTo?.nonce ?? 0} unbounded={unbounded} keepScroll={keepScroll} className={cn(fill ? "min-h-0 flex-1" : unbounded ? undefined : "max-h-80", listClassName)} />
    </div>
  );
}
