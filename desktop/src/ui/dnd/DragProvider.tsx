/**
 * The app's one drag world (ide/03).
 *
 * Drag and drop here is **not** the browser's: `dnd-kit` drives it from
 * pointer and keyboard events, so a drag starts in the desktop webview (the
 * HTML5 protocol needs a data store WebKit refuses to start without), shows a
 * ghost the app draws — the payload's glyph and label, dimmed while the
 * target under it refuses (`useDragCue`) — auto-scrolls the list under the
 * pointer, and is reachable from the keyboard — Space lifts, arrows move,
 * Space drops. A surface may draw its own payload's ghost (`useDragGhost`)
 * and have it settle into place when the drag ends.
 *
 * One provider, mounted once in `App.tsx`, because a drag crosses surfaces:
 * a file leaves the explorer and lands on the agent pane, a tab leaves one
 * pane's strip and lands in another's. Every drop target registers a
 * **handler under a zone id** and stamps its droppable data with that id;
 * when a drag ends over anything, the provider looks the zone up and hands it
 * the payload. `DropZone`, `SortableList` and `TreeList` are the three kinds
 * of target; views compose those and never touch the library.
 */

import { smallestFirst } from "./collisionModel.mjs";
import {
  DndContext,
  DragOverlay,
  KeyboardSensor,
  MeasuringStrategy,
  PointerSensor,
  closestCenter,
  defaultDropAnimationSideEffects,
  pointerWithin,
  useDndContext,
  useSensor,
  useSensors,
  type CollisionDetection,
  type DragEndEvent,
  type DragStartEvent,
  type DropAnimation,
} from "@dnd-kit/core";
import { restrictToWindowEdges } from "@dnd-kit/modifiers";
import { sortableKeyboardCoordinates } from "@dnd-kit/sortable";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { ICON } from "../icons";
import { EASE, durations } from "../motion";
import { dragCount, dragGlyph, dragType, type DragData } from "./dragData.mjs";

/** What every droppable's `data` carries: the zone whose handler takes the drop. */
export interface DroppableData {
  zone: string;
  [key: string]: unknown;
}

/** A finished drag, as a zone's handler sees it. */
export interface DropEvent {
  /** What was dragged. */
  data: DragData;
  /** The droppable the drag ended over. */
  over: DroppableData;
  /** Where the pointer was; the activator's position for a keyboard drag. */
  pointer: { x: number; y: number };
}

type Handler = (event: DropEvent) => void;

interface Registry {
  register: (zone: string, handler: Handler) => () => void;
}

/**
 * How a payload of one type is drawn under the pointer, and whether it
 * **settles** — travels to the element that carries its id when the drag
 * ends. A surface registers one for its own payload (the Board draws the
 * card itself); a type nobody registered is the chip.
 */
interface Ghost {
  render: (data: DragData) => ReactNode;
  settle: boolean;
}

interface Ghosts {
  register: (type: DragData["type"], ghost: Ghost) => () => void;
}

const GhostContext = createContext<Ghosts | null>(null);

/** dnd-kit's easing string for the motion tokens' curve. */
const EASING = `cubic-bezier(${EASE.join(", ")})`;

/** The settle: the overlay travels to the item's place over the slow token, the item's own element faded meanwhile. */
function settleAnimation(): DropAnimation {
  return {
    duration: durations().slow * 1000,
    easing: EASING,
    sideEffects: defaultDropAnimationSideEffects({ styles: { active: { opacity: "0.35" } } }),
  };
}

const RegistryContext = createContext<Registry | null>(null);

/** What the target under the drag is telling it — the ghost wears it. */
export type DragCue = "none" | "refused" | "stay" | "nest" | "move";

interface CueBoard {
  /** The cue over one target, by its zone; `none` when the drag left it. */
  report: (zone: string, cue: DragCue) => void;
}

const CueContext = createContext<CueBoard>({ report: () => {} });

/** A pixel budget a click never crosses, so a click is never a drag. */
const ACTIVATION_DISTANCE = 4;

/**
 * The pointer's position during a drag: where it went down, plus how far it
 * has moved. `dnd-kit` reports the *rect* of the moving item, not the
 * pointer, and a tree needs the pointer to know whether a drop is above,
 * on or below a row.
 */
export function pointerOf(event: { activatorEvent: Event; delta: { x: number; y: number } }): { x: number; y: number } {
  const e = event.activatorEvent as Partial<PointerEvent>;
  const x = typeof e.clientX === "number" ? e.clientX : 0;
  const y = typeof e.clientY === "number" ? e.clientY : 0;
  return { x: x + event.delta.x, y: y + event.delta.y };
}

/**
 * The droppable under the pointer, **innermost first**: a row inside a tree
 * inside a pane is three containers under one point, and the one that means
 * something is the smallest. Without a pointer (a keyboard drag) the nearest
 * centre stands in.
 */
const innermost: CollisionDetection = (args) => {
  const within = pointerWithin(args);
  if (within.length === 0) return closestCenter(args);
  // The smallest box containing the pointer is the one meant (`collisionModel`).
  const byId = new Map(within.map((c) => [c.id, c]));
  return smallestFirst([...byId.keys()], (id) => args.droppableRects.get(id)).map((id) => byId.get(id)!);
};

export function DragProvider({ children }: { children: ReactNode }) {
  const handlers = useRef(new Map<string, Handler>());
  const [active, setActive] = useState<DragData | null>(null);
  // Every target reports its own cue under its zone; the ghost wears the one
  // that is not `none` — at most one target is under the pointer.
  const cues = useRef(new Map<string, DragCue>());
  const [cue, setCue] = useState<DragCue>("none");
  const board = useMemo<CueBoard>(
    () => ({
      report: (zone, next) => {
        // Re-set, so the latest report is last: the target under the
        // pointer reported most recently, and the ghost wears its cue.
        cues.current.delete(zone);
        if (next !== "none") cues.current.set(zone, next);
        setCue([...cues.current.values()].at(-1) ?? "none");
      },
    }),
    [],
  );

  const registry = useMemo<Registry>(
    () => ({
      register: (zone, handler) => {
        handlers.current.set(zone, handler);
        return () => {
          if (handlers.current.get(zone) === handler) handlers.current.delete(zone);
        };
      },
    }),
    [],
  );
  // The ghosts surfaces registered for their own payloads, by type; a
  // change re-renders so the overlay reads the latest.
  const ghostMap = useRef(new Map<DragData["type"], Ghost>());
  const [ghostEpoch, setGhostEpoch] = useState(0);
  const ghosts = useMemo<Ghosts>(
    () => ({
      register: (type, ghost) => {
        ghostMap.current.set(type, ghost);
        setGhostEpoch((n) => n + 1);
        return () => {
          if (ghostMap.current.get(type) === ghost) {
            ghostMap.current.delete(type);
            setGhostEpoch((n) => n + 1);
          }
        };
      },
    }),
    [],
  );

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: ACTIVATION_DISTANCE } }),
    // Space lifts and drops; Enter is left to the element it is on — a tab
    // still selects, a row still opens.
    useSensor(KeyboardSensor, {
      // A tree's row steps through the tree's own slots (`TreeList`), so the
      // sensor moves nothing for it; a sortable's item takes the next slot.
      coordinateGetter: (event, args) => ((args.context.active?.data.current as { treeRow?: string } | undefined)?.treeRow ? undefined : sortableKeyboardCoordinates(event, args)),
      keyboardCodes: { start: ["Space"], cancel: ["Escape"], end: ["Space"] },
    }),
  );

  const onDragStart = useCallback((e: DragStartEvent) => {
    const data = e.active.data.current;
    setActive(dragType(data) ? (data as DragData) : null);
  }, []);
  const onDragEnd = useCallback((e: DragEndEvent) => {
    setActive(null);
    cues.current.clear();
    setCue("none");
    const data = e.active.data.current;
    const over = e.over?.data.current as DroppableData | undefined;
    if (!dragType(data) || !over || typeof over.zone !== "string") return;
    handlers.current.get(over.zone)?.({ data: data as DragData, over, pointer: pointerOf(e) });
  }, []);
  const onDragCancel = useCallback(() => {
    setActive(null);
    cues.current.clear();
    setCue("none");
  }, []);
  const glyph = active ? dragGlyph(active) : null;
  const Glyph = glyph ? (ICON as Record<string, typeof ICON.file>)[glyph] : null;
  const count = active ? dragCount(active) : 1;
  // `ghostEpoch` is the change signal for the ghost map, which is a ref.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const ghost = useMemo(() => (active ? (ghostMap.current.get(active.type) ?? null) : null), [active, ghostEpoch]);

  return (
    <RegistryContext.Provider value={registry}>
      <GhostContext.Provider value={ghosts}>
      <CueContext.Provider value={board}>
      <DndContext
        sensors={sensors}
        collisionDetection={innermost}
        autoScroll
        // Rows come and go while a virtual list scrolls under a drag; measuring
        // once at lift would leave the new rows without a rect to hit.
        measuring={{ droppable: { strategy: MeasuringStrategy.WhileDragging } }}
        onDragStart={onDragStart}
        onDragEnd={onDragEnd}
        onDragCancel={onDragCancel}
      >
        {children}
        {/* The ghost: what the surface registered for the payload's type —
            the Board's card, say — else the payload's glyph and label in a
            chip; either lifts off its row and follows the pointer, dimmed
            while a target refuses it. A registered ghost that settles
            travels to the item's place when the drag ends; the chip has no
            drop animation — the list has already moved the item. */}
        <DragOverlay dropAnimation={ghost?.settle ? settleAnimation() : null} modifiers={[restrictToWindowEdges]} zIndex={60}>
          {active &&
            (ghost ? (
              <div data-cue={cue} className="drag-ghost motion-lift pointer-events-none">
                {ghost.render(active)}
              </div>
            ) : (
              <span data-cue={cue} className="drag-ghost motion-lift anim pointer-events-none inline-flex max-w-64 items-center gap-1.5 rounded-control border border-accent/60 bg-surface px-2 py-1 text-2xs text-text shadow-lg">
                {Glyph && <Glyph size={12} aria-hidden className="shrink-0 text-text-dim" />}
                <span className="min-w-0 truncate">{active.label}</span>
                {count > 1 && <span className="rounded-full bg-accent-soft px-1 text-accent-ink">{count}</span>}
              </span>
            ))}
        </DragOverlay>
      </DndContext>
      </CueContext.Provider>
      </GhostContext.Provider>
    </RegistryContext.Provider>
  );
}

/**
 * Take the drops that end over droppables stamped with `zone`. The latest
 * handler is called, so a caller may pass a fresh closure every render.
 */
export function useDropHandler(zone: string, handler: Handler): void {
  const registry = useContext(RegistryContext);
  const latest = useRef(handler);
  latest.current = handler;
  useEffect(() => {
    if (!registry) return;
    return registry.register(zone, (e) => latest.current(e));
  }, [registry, zone]);
}

/** The board a target reports its cue to — what the ghost then wears. */
export function useDragCue(): CueBoard {
  return useContext(CueContext);
}

/**
 * Draw a payload type's ghost yourself — the Board's card as the card — and
 * say whether it settles into place when the drag ends. The latest
 * `render` is read at draw time, so a surface may pass a fresh closure.
 */
export function useDragGhost(type: DragData["type"], render: (data: DragData) => ReactNode, opts: { settle?: boolean } = {}): void {
  const ghosts = useContext(GhostContext);
  const latest = useRef(render);
  latest.current = render;
  const settle = opts.settle ?? false;
  useEffect(() => {
    if (!ghosts) return;
    return ghosts.register(type, { render: (data) => latest.current(data), settle });
  }, [ghosts, type, settle]);
}

/** The payload of the drag in flight, or null. */
export function useActiveDrag(): DragData | null {
  const { active } = useDndContext();
  const data = active?.data.current;
  return dragType(data) ? (data as DragData) : null;
}
