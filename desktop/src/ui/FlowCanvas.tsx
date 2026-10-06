/**
 * The one wrapper over `@xyflow/react`.
 *
 * Every view draws a graph through this component and never imports the
 * library itself — `ui/imports.test.mjs` bans `@xyflow/` outside the kit the
 * way it bans `lucide-react`. What the wrapper buys is the same thing the
 * rest of the kit buys: the canvas is themed from the role contract
 * (`theme/flow.css`), its keyboard is the kit's, and swapping the library
 * would be a change to one file. It is reached through `ui/flow.ts`, the
 * kit's second entry, so the library stays out of the main chunk.
 *
 * The wrapper is deliberately small. It draws nodes and edges it is given,
 * reports moves, connections, selection, deletion and drops, and applies
 * the three canvas switches it is handed (snap, grid, minimap — the
 * designer's settings own them). Layout is not its business: positions
 * arrive from the caller (`views/_workflow/workflowLayout.mjs` — a step's
 * own, or a derived one until it has one), and a drag reports where every
 * moved node was dropped for the caller to write down. A flow is a smooth
 * step from handle to handle; a loop leaves by the side handles.
 *
 * **Controlled, with a live mirror.** xyflow is a controlled component: a
 * drag, a click that selects, a box-select are *changes* it reports and does
 * not apply. The wrapper keeps a mirror of the caller's nodes and edges,
 * applies every change to the mirror so a dragged node follows the pointer
 * and a clicked edge is really selected, and resyncs the mirror from the
 * props whenever the caller changes the picture. A drop the caller writes
 * down comes back as the new props and stays; one it refuses leaves the
 * props as they were, and the same resync puts the node back — one path for
 * both. The resync is **content-addressed** (`flowMirrorModel.mjs`): a caller
 * that re-renders with the same picture commits nothing and never costs
 * xyflow a re-measure.
 *
 * Keyboard, on the focused canvas only: Delete/Backspace removes the
 * selection, `Mod+D` duplicates it, `Mod+Z` / `Mod+Shift+Z` undo and redo,
 * `Escape` clears the selection. None of these are keymap commands — they are
 * the editor's own, the way a text field owns its arrows.
 *
 * **Where it looks.** A canvas opens on the viewport its caller hands in
 * (`startViewport` — where the person left it), and fitted to the picture
 * when there is none; every pan and zoom that settles is reported
 * (`onViewport`) for the caller to keep. A `fitKey` change fits again either
 * way — *Fit*, *Tidy*.
 */

import {
  Background,
  BackgroundVariant,
  Controls,
  Handle,
  MarkerType,
  MiniMap,
  Position,
  ReactFlow,
  ReactFlowProvider,
  applyEdgeChanges,
  applyNodeChanges,
  useReactFlow,
  type AriaLabelConfig,
  type Connection,
  type Edge,
  type Node,
  type NodeProps,
  type NodeTypes,
  type OnConnect,
  type OnEdgesChange,
  type OnNodeDrag,
  type OnNodesChange,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useCallback, useEffect, useMemo, useRef, useState, type DragEvent, type KeyboardEvent, type ReactNode } from "react";
import { cn } from "./cn";
import { reconcileEdges, reconcileNodes } from "./flowMirrorModel.mjs";
import { t } from "../i18n/l10n.mjs";

export type FlowPosition = { x: number; y: number };

/** Where a canvas looks: the pan and the zoom. */
export type FlowViewport = { x: number; y: number; zoom: number };

/** A node the caller hands in: an id, a position, and whatever its renderer needs. */
export interface FlowNode<D extends Record<string, unknown> = Record<string, unknown>> {
  id: string;
  position: FlowPosition;
  data: D;
  /** Which renderer from `nodeTypes` draws it. */
  type: string;
  selected?: boolean;
  draggable?: boolean;
}

/**
 * An edge, with a tone the theme paints, a kind the theme dashes, and the
 * handles it leaves from and arrives at. A `loop` is drawn around the side;
 * a `boundary` edge — a divert's path — leaves from its boundary's chip.
 */
export interface FlowEdge {
  id: string;
  source: string;
  target: string;
  /** The source handle — a gateway's branch or a divert's (`branch:<name>`), `out`, `fail` or `loop-out`. */
  sourceHandle?: string | null;
  /** The target handle — `in`, or `loop-in` for a loop. */
  targetHandle?: string | null;
  label?: string;
  tone?: "default" | "taken" | "skipped";
  kind?: "then" | "on_fail" | "boundary";
  loop?: boolean;
  animated?: boolean;
  /** The flow a running process came along to where it stands now — drawn moving toward it (`theme/flow.css`, `.is-live`). */
  live?: boolean;
}

/** What a node measured at, reported once xyflow knows. */
export type FlowSize = { width: number; height: number };

/** What a node renderer receives: `NodeProps` narrowed to its own data. */
export type FlowNodeProps<D extends Record<string, unknown>> = NodeProps<Node<D>>;

export interface FlowCanvasProps<D extends Record<string, unknown>> {
  nodes: FlowNode<D>[];
  edges: FlowEdge[];
  nodeTypes: NodeTypes;
  /** One gesture dropped these nodes here — a multi-selection moves together; the caller writes them down. */
  onMoved?: (moves: { id: string; position: FlowPosition }[]) => void;
  /** A connection was drawn from a source (and its handle) to a target. */
  onConnect?: (source: string, sourceHandle: string | null, target: string) => void;
  onSelect?: (id: string | null) => void;
  /** Delete pressed on a selection — node ids and edge ids. */
  onDelete?: (nodes: string[], edges: string[]) => void;
  onDuplicate?: (id: string) => void;
  onUndo?: () => void;
  onRedo?: () => void;
  /** Something was dropped from outside — the palette. `mime` is the type matched from `acceptMimes`. */
  onDropAt?: (mime: string, payload: string, position: FlowPosition) => void;
  acceptMimes?: string[];
  readOnly?: boolean;
  snap?: boolean;
  grid?: number;
  minimap?: boolean;
  /** Change it to refit the view — after a layout, or a load. */
  fitKey?: string | number;
  /** The view to open on — where the person left it. Read once, at mount; none fits the picture. */
  startViewport?: FlowViewport | null;
  /** A pan or a zoom settled here: the caller keeps it to open on next time. */
  onViewport?: (viewport: FlowViewport) => void;
  /** A node measured: the caller lays out at real sizes. */
  onMeasured?: (id: string, size: FlowSize) => void;
  /** Bring this node into view — after a drop or a click-to-add; change `nonce` to ask again. */
  reveal?: { id: string; nonce: number } | null;
  className?: string;
  children?: ReactNode;
  /** The canvas's accessible name; the catalog's *Flow canvas* when the caller names none. */
  label?: string;
}

/**
 * xyflow's own words — its controls, the minimap, a handle, and what a
 * keyboard on a step or a flow can do — from the catalog, not the library's
 * English. "Fit view" is not among the controls: the designer has its own Fit.
 */
function ariaLabels(): Partial<AriaLabelConfig> {
  return {
    "node.a11yDescription.default": t("ui-flow-canvas-node-keys"),
    "node.a11yDescription.keyboardDisabled": t("ui-flow-canvas-node-keys-move"),
    "node.a11yDescription.ariaLiveMessage": ({ direction, x, y }) => t("ui-flow-canvas-node-moved", { direction, x, y }),
    "edge.a11yDescription.default": t("ui-flow-canvas-edge-keys"),
    "controls.ariaLabel": t("ui-flow-canvas-controls"),
    "controls.zoomIn.ariaLabel": t("ui-flow-canvas-zoom-in"),
    "controls.zoomOut.ariaLabel": t("ui-flow-canvas-zoom-out"),
    "controls.fitView.ariaLabel": t("ui-flow-canvas-fit"),
    "controls.interactive.ariaLabel": t("ui-flow-canvas-interactive"),
    "minimap.ariaLabel": t("ui-flow-canvas-minimap"),
    "handle.ariaLabel": t("ui-flow-canvas-handle"),
  };
}

/**
 * A handle on a step node. Five places, each one meaning: `in` on
 * top for the flows that arrive, `out` at the bottom for the flows that
 * leave (a `decide` step draws one per branch, spread by `offset`), `fail`
 * on the left for `on_fail: then`, and `loop-out` / `loop-in` on the right,
 * where a flow back to an earlier step leaves and arrives — so a loop is
 * routed around the side instead of across the graph. A handle may also sit
 * inside a part of the card — a divert's chip on its lower edge — and is
 * placed against that part: the path leaves from under the chip. Branch
 * names travel on the edge's label, not beside the handle.
 */
export function FlowHandle({
  kind,
  id,
  side = kind === "target" ? "top" : "bottom",
  offset,
  connectable = true,
}: {
  kind: "target" | "source";
  id?: string;
  side?: "top" | "bottom" | "left" | "right";
  /** Position along the side as a percentage, for several handles on one side. */
  offset?: number;
  /** Off: an anchor edges are drawn from, never one a flow is dragged out of. */
  connectable?: boolean;
}) {
  const position = { top: Position.Top, bottom: Position.Bottom, left: Position.Left, right: Position.Right }[side];
  const style =
    offset === undefined ? undefined : side === "top" || side === "bottom" ? { left: `${offset}%` } : { top: `${offset}%` };
  return (
    <Handle
      type={kind}
      id={id}
      position={position}
      style={style}
      isConnectable={connectable}
      className={cn("!border-border !bg-surface", !connectable && "!pointer-events-none !opacity-0")}
    />
  );
}

function toEdge(e: FlowEdge): Edge {
  return {
    id: e.id,
    source: e.source,
    target: e.target,
    sourceHandle: e.sourceHandle ?? undefined,
    targetHandle: e.targetHandle ?? undefined,
    label: e.label,
    animated: e.animated,
    className: cn(`tone-${e.tone ?? "default"}`, `kind-${e.kind ?? "then"}`, e.loop && "kind-loop", e.live && "is-live"),
    // Every flow is a smooth step from its handle to the other's; a loop and
    // an on_fail leave from a side handle and come around.
    type: "smoothstep",
    markerEnd: { type: MarkerType.ArrowClosed, width: 14, height: 14 },
  };
}

function Inner<D extends Record<string, unknown>>({
  nodes,
  edges,
  nodeTypes,
  onMoved,
  onConnect,
  onSelect,
  onDelete,
  onDuplicate,
  onUndo,
  onRedo,
  onDropAt,
  acceptMimes = [],
  readOnly = false,
  snap = true,
  grid = 16,
  minimap = false,
  fitKey,
  startViewport,
  onViewport,
  onMeasured,
  reveal,
  className,
  children,
  label,
}: FlowCanvasProps<D>) {
  const flow = useReactFlow();
  const aria = useMemo(ariaLabels, []);
  // The view it opens on, and the fit key it opened with: a canvas handed a
  // place opens there and fits only when the key moves; one handed none
  // fits at once, as it always did.
  const [opening] = useState<FlowViewport | null>(() => startViewport ?? null);
  const openingFit = useRef(fitKey);

  const rfNodes = useMemo<Node<D>[]>(
    () =>
      nodes.map((n) => ({
        id: n.id,
        position: n.position,
        data: n.data,
        type: n.type,
        selected: n.selected,
        draggable: !readOnly && n.draggable !== false,
        connectable: !readOnly,
      })),
    [nodes, readOnly],
  );
  const rfEdges = useMemo(() => edges.map(toEdge), [edges]);

  // The live mirror: what xyflow draws. Every change it reports lands here;
  // every change the caller makes replaces it — by content, so a re-render
  // that says nothing new is not a change (`reconcileNodes` hands back the
  // previous array itself) and a surviving node keeps its measurement.
  const [liveNodes, setLiveNodes] = useState<Node<D>[]>(rfNodes);
  const [liveEdges, setLiveEdges] = useState<Edge[]>(rfEdges);
  useEffect(() => setLiveNodes((prev) => reconcileNodes(prev, rfNodes)), [rfNodes]);
  useEffect(() => setLiveEdges((prev) => reconcileEdges(prev, rfEdges)), [rfEdges]);
  const onNodesChange: OnNodesChange<Node<D>> = useCallback(
    (changes) => {
      // xyflow measured a card: the layout lays it out at that size.
      for (const c of changes) {
        if (c.type === "dimensions" && c.dimensions && onMeasured) onMeasured(c.id, c.dimensions);
      }
      setLiveNodes((ns) => applyNodeChanges(changes, ns));
    },
    [onMeasured],
  );
  const onEdgesChange: OnEdgesChange<Edge> = useCallback(
    (changes) => setLiveEdges((es) => applyEdgeChanges(changes, es)),
    [],
  );

  useEffect(() => {
    // Opened on a remembered place: it stays until the caller asks for a fit.
    if (opening && fitKey === openingFit.current) return;
    // After the caller changed the picture, frame it. A timeout lets the
    // nodes measure themselves first; xyflow fits to measured bounds.
    const t = setTimeout(() => void flow.fitView({ padding: 0.2, duration: 200 }), 30);
    return () => clearTimeout(t);
  }, [fitKey, flow, opening]);

  // A step was just added: bring it into view without losing the zoom, so a
  // click-to-add on a big graph never lands off screen.
  useEffect(() => {
    if (!reveal) return;
    const t = setTimeout(() => {
      const node = flow.getNode(reveal.id);
      if (!node) return;
      const w = node.measured?.width ?? 220;
      const h = node.measured?.height ?? 72;
      void flow.setCenter(node.position.x + w / 2, node.position.y + h / 2, { zoom: flow.getZoom(), duration: 200 });
    }, 60);
    return () => clearTimeout(t);
    // Keyed on the step and the ask, not the `reveal` object: a caller may
    // hand a fresh one per render, and `flow` is the store's, stable.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reveal?.id, reveal?.nonce]);

  const handleConnect: OnConnect = useCallback(
    (c: Connection) => {
      if (c.source && c.target) onConnect?.(c.source, c.sourceHandle ?? null, c.target);
    },
    [onConnect],
  );

  const handleDragStop: OnNodeDrag<Node<D>> = useCallback(
    (_e, _node, dragged) => {
      // Report where every node of the gesture landed. What happens next is
      // the caller's: a move written down comes back as new props; one it
      // refuses leaves the props as they were, and the props effect above
      // puts the mirror back — the same path either way.
      onMoved?.(dragged.map((n) => ({ id: n.id, position: n.position })));
    },
    [onMoved],
  );

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (readOnly) return;
    const mod = e.metaKey || e.ctrlKey;
    if (e.key === "Delete" || e.key === "Backspace") {
      const ns = liveNodes.filter((n) => n.selected).map((n) => n.id);
      const es = liveEdges.filter((x) => x.selected).map((x) => x.id);
      if (ns.length + es.length > 0) {
        e.preventDefault();
        onDelete?.(ns, es);
      }
    } else if (mod && e.key.toLowerCase() === "d") {
      const one = liveNodes.find((n) => n.selected);
      if (one) {
        e.preventDefault();
        onDuplicate?.(one.id);
      }
    } else if (mod && e.key.toLowerCase() === "z") {
      e.preventDefault();
      if (e.shiftKey) onRedo?.();
      else onUndo?.();
    } else if (e.key === "Escape") {
      onSelect?.(null);
    }
  };

  const onDragOver = (e: DragEvent<HTMLDivElement>) => {
    if (readOnly) return;
    if (acceptMimes.some((m) => e.dataTransfer.types.includes(m))) {
      e.preventDefault();
      e.dataTransfer.dropEffect = "copy";
    }
  };
  const onDrop = (e: DragEvent<HTMLDivElement>) => {
    if (readOnly) return;
    const mime = acceptMimes.find((m) => e.dataTransfer.types.includes(m));
    if (!mime) return;
    e.preventDefault();
    const position = flow.screenToFlowPosition({ x: e.clientX, y: e.clientY });
    onDropAt?.(mime, e.dataTransfer.getData(mime), position);
  };

  // What the canvas draws for the eye alone — the dotted ground, the arrowhead
  // definitions — is no image to a screen reader: every `<svg>` that names
  // nothing and holds no control is hidden from the tree once xyflow has
  // drawn it. The edges' own svg holds the focusable flows and stays.
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const frame = requestAnimationFrame(() => {
      for (const svg of box.current?.querySelectorAll<SVGElement>("svg") ?? []) {
        if (svg.classList.contains("react-flow__edges") || svg.hasAttribute("aria-label") || svg.hasAttribute("role") || svg.querySelector("title, [tabindex]")) continue;
        svg.setAttribute("aria-hidden", "true");
      }
    });
    return () => cancelAnimationFrame(frame);
  }, []);

  return (
    <div
      ref={box}
      className={cn("bisa-flow h-full w-full outline-none", className)}
      role="group"
      aria-label={label ?? t("ui-flow-canvas-label")}
      tabIndex={0}
      onKeyDown={onKeyDown}
      onDragOver={onDragOver}
      onDrop={onDrop}
    >
      <ReactFlow<Node<D>, Edge>
        nodes={liveNodes}
        edges={liveEdges}
        nodeTypes={nodeTypes}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={handleConnect}
        onNodeDragStop={handleDragStop}
        onNodeClick={(_e, n) => onSelect?.(n.id)}
        onPaneClick={() => onSelect?.(null)}
        nodesDraggable={!readOnly}
        nodesConnectable={!readOnly}
        // Selecting is a read — a run's canvas selects a step to show its
        // record — so it stays on; only edges lose focus when read-only.
        elementsSelectable
        edgesFocusable={!readOnly}
        edgesReconnectable={false}
        // The wrapper owns deletion so the caller can refuse or record it.
        deleteKeyCode={null}
        snapToGrid={snap}
        snapGrid={[grid, grid]}
        fitView={!opening}
        defaultViewport={opening ?? undefined}
        onMoveEnd={onViewport ? (_e, v) => onViewport({ x: v.x, y: v.y, zoom: v.zoom }) : undefined}
        minZoom={0.25}
        maxZoom={2}
        proOptions={{ hideAttribution: true }}
        ariaLabelConfig={aria}
        defaultEdgeOptions={{ type: "smoothstep", markerEnd: { type: MarkerType.ArrowClosed, width: 14, height: 14 } }}
      >
        <Background variant={BackgroundVariant.Dots} gap={grid} size={1} />
        <Controls showInteractive={false} showFitView={false} position="bottom-left" />
        {minimap && <MiniMap pannable zoomable position="bottom-right" />}
        {children}
      </ReactFlow>
    </div>
  );
}

/**
 * The canvas. Wrapped in its own provider so two canvases on one screen — a
 * designer and a goal's run — never share a store.
 */
export function FlowCanvas<D extends Record<string, unknown>>(props: FlowCanvasProps<D>) {
  return (
    <ReactFlowProvider>
      <Inner {...props} />
    </ReactFlowProvider>
  );
}
