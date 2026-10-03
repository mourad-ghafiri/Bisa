/**
 * The canvas half of the designer, shared by the library's editor and the
 * goal's Workflow tab.
 *
 * It takes a definition and gives back new ones. A step's position is its
 * own (`Step.position`): a drop from the palette lands where it was released,
 * a drag puts a card where it was dropped — snapped to the grid and nudged
 * beside a card it would cover — and connecting two steps moves nothing. A
 * step that has no position yet is drawn where the layered layout puts it
 * (`workflowLayout.mjs`), the same way on every open, and **the first edit
 * the canvas makes writes every derived position down** (`commit` →
 * `withPositions`), so the picture is the person's from then on. *Tidy* lays
 * everything out again, one undoable edit. A step just added — a drop, a
 * duplicate — is reported (`onReveal`) for the caller to bring into view; the
 * caller owns the one reveal, so a drop here and a click on its palette never
 * race. Every flow leaves and arrives at the handle its kind owns — a branch
 * at the bottom (`branch:<name>`, so a branch may be named like a shared
 * handle), a divert's path from its boundary chip on the lower edge (the
 * same `branch:<name>`, so drawing from the chip labels the flow), an
 * `on_fail` on the left, a loop on the right; a start has no `in` and an
 * end no `out`. In run mode every node wears its state, every edge its tone,
 * and a boundary that fired lights its chip. Every edit — move, connect,
 * disconnect, delete, duplicate, drop, tidy — passes the one gate, `mayEdit`:
 * nothing when the canvas is read-only, and in an amendment only the steps
 * the run has not started.
 *
 * Each node's `data` is kept by reference while its facts are the same
 * (`reuse`), so the canvas mirror's fast path hits on the renders that
 * change nothing — a goal page refetches on every frame.
 *
 * The canvas opens where the caller says (`startViewport` — the library's
 * designer remembers it per workflow, `designerMemoryStore.ts`) and fitted
 * when it says nothing; every settled pan and zoom is reported
 * (`onViewport`).
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { NewWorkflowBody, Problem, Workflow, WorkflowRun } from "../../types";
import { Button, ICON, Tooltip } from "../../ui";
import { FlowCanvas, type FlowEdge, type FlowNode, type FlowSize, type FlowViewport } from "../../ui/flow";
import { reuse, sameShallow } from "../../ui/flowMirrorModel.mjs";
import { edgeTone, firedBoundaries, stepActions, stepLabel, stepTone } from "./runView.mjs";
import { STEP_MIME, branchHandle, branchOfHandle, type StepKindName } from "./stepKinds.mjs";
import { HANDLE, NODE_TYPES, type StepNodeData } from "./StepNode";
import {
  addStep,
  connect,
  disconnect,
  duplicateStep,
  mayEdit,
  removeStep,
  setOnFail,
  setPositions,
  toGraph,
  uniqueId,
  type Definition,
} from "./workflowGraph.mjs";
import { layout, nudgeFree, snapTo, tidy, withPositions } from "./workflowLayout.mjs";
import { t } from "../../i18n/l10n.mjs";

export interface DesignerSettings {
  snap: boolean;
  grid: number;
  minimap: boolean;
}

/**
 * Which handles a flow uses, from its kind and whether it loops back: a
 * divert's path leaves from its boundary chip's own handle, a branch from
 * the branch's.
 */
function handlesFor(e: { kind: string; branch: string | null; loop: boolean }): { source: string; target: string } {
  if (e.loop) return { source: HANDLE.loopOut, target: HANDLE.loopIn };
  if (e.kind === "on_fail") return { source: HANDLE.fail, target: HANDLE.in };
  return { source: e.branch === null ? HANDLE.out : branchHandle(e.branch), target: HANDLE.in };
}

/** The same card: the same step object, count, run words, fired boundaries, gate and whose move it is. */
const sameNodeData = (a: StepNodeData, b: StepNodeData) =>
  sameShallow(a, b, ["step", "problems", "tone", "label", "fired", "editable", "yours"]) && sameShallow(a.state, b.state);

/** The acts that make a waiting step the person's move — an answer, a decision, a release — as `stepActions` names them. */
const YOUR_ACTS = new Set(["answer", "decide", "release"]);

/** Where a run is: the states a step is in while the run stands on it. */
const LIVE_STATES = new Set(["running", "waiting"]);

export function Designer<D extends NewWorkflowBody | Workflow>({
  value,
  problems,
  run,
  selected,
  onSelect,
  onChange,
  onUndo,
  onRedo,
  canUndo,
  canRedo,
  readOnly,
  editable,
  settings,
  onRefuse,
  reveal,
  onReveal,
  startViewport,
  onViewport,
}: {
  value: D;
  problems: readonly Problem[];
  /** In run mode: the run whose states the nodes wear. */
  run?: WorkflowRun | null;
  selected: string | null;
  onSelect: (id: string | null) => void;
  onChange: (next: D) => void;
  onUndo?: () => void;
  onRedo?: () => void;
  canUndo?: boolean;
  canRedo?: boolean;
  readOnly?: boolean;
  /** In an amendment: the steps that may still change. Null means every step. */
  editable?: Set<string> | null;
  settings: DesignerSettings;
  /** A connection or an edit the graph refused, with the reason. */
  onRefuse?: (reason: string) => void;
  /** A step to bring into view; change `nonce` to ask again. The caller owns it. */
  reveal?: { id: string; nonce: number } | null;
  /** A step this canvas just added — a drop, a duplicate — for the caller to reveal. */
  onReveal?: (id: string) => void;
  /** The view the canvas opens on — where the person left it; none fits the picture. Read once. */
  startViewport?: FlowViewport | null;
  /** A pan or a zoom settled: the caller may keep it. */
  onViewport?: (viewport: FlowViewport) => void;
}) {
  const [fitKey, setFitKey] = useState(0);
  // The `data` handed to each node last time, so an unchanged step keeps its object.
  const dataRef = useRef<Map<string, StepNodeData>>(new Map());
  // What each card measured at; the layout lays out at those sizes so a card
  // with badges keeps its gap. A size that did not change is not a change.
  const [sizes, setSizes] = useState<Map<string, FlowSize>>(() => new Map());
  const onMeasured = useCallback((id: string, size: FlowSize) => {
    setSizes((m) => {
      const cur = m.get(id);
      if (cur && cur.width === size.width && cur.height === size.height) return m;
      const next = new Map(m);
      next.set(id, size);
      return next;
    });
  }, []);
  const wf = value as Definition;
  const laid = useMemo(() => layout(wf), [wf]);
  const { positions } = laid;
  const graph = useMemo(() => toGraph(wf), [wf]);
  const countBy = useMemo(() => {
    const m = new Map<string, number>();
    for (const p of problems) if (p.step) m.set(p.step, (m.get(p.step) ?? 0) + 1);
    return m;
  }, [problems]);
  const gate = useMemo(() => ({ readOnly: !!readOnly, editable: editable ?? null }), [readOnly, editable]);
  // The one door every canvas edit leaves by: while any step has no position
  // of its own, the derived picture is written for all of them, so a connect
  // made first can never reflow the graph under the person.
  const commit = useCallback((next: D) => onChange(withPositions(next, positions)), [onChange, positions]);
  // A tidy moves every card; in an amendment where some may not move, it is
  // refused rather than half done.
  const mayTidy = mayEdit(gate, null) && (editable === null || editable === undefined || graph.nodes.every((n) => editable.has(n.id)));

  // Forget the size of a step that left the canvas.
  useEffect(() => {
    setSizes((m) => {
      const ids = new Set(graph.nodes.map((n) => n.id));
      if ([...m.keys()].every((k) => ids.has(k))) return m;
      return new Map([...m].filter(([k]) => ids.has(k)));
    });
  }, [graph]);

  const nodes = useMemo<FlowNode<StepNodeData>[]>(() => {
    const kept = new Map<string, StepNodeData>();
    const out = graph.nodes.map((n) => {
      // A step waiting on the person wears the accent's ring — the summons —
      // where one waiting on the world keeps its state's own.
      const yours = run ? stepActions(n.step, run.steps[n.id]).some((a) => YOUR_ACTS.has(a)) : false;
      const fresh: StepNodeData = {
        step: n.step,
        problems: countBy.get(n.id) ?? 0,
        state: run ? (run.steps[n.id]?.state ?? { state: "pending" }) : null,
        tone: run ? (yours ? "accent" : stepTone(run, n.id)) : null,
        yours,
        label: run ? stepLabel(run, n.id) : null,
        fired: run ? [...firedBoundaries(run, n.id)].join(",") || null : null,
        editable: mayEdit(gate, n.id) || !!readOnly,
      };
      const data = reuse(dataRef.current.get(n.id), fresh, sameNodeData);
      kept.set(n.id, data);
      return {
        id: n.id,
        type: "step",
        position: positions.get(n.id) ?? { x: 0, y: 0 },
        selected: selected === n.id,
        draggable: mayEdit(gate, n.id),
        data,
      };
    });
    dataRef.current = kept;
    return out;
  }, [graph, positions, selected, countBy, run, gate, readOnly]);
  const edges = useMemo<FlowEdge[]>(
    () =>
      graph.edges.map((e) => {
        const h = handlesFor(e);
        return {
          id: e.id,
          source: e.from,
          target: e.to,
          sourceHandle: h.source,
          targetHandle: h.target,
          label: e.branch ?? (e.kind === "on_fail" ? t("workflow-designer-fail") : undefined),
          kind: e.kind,
          loop: e.loop,
          tone: run ? edgeTone(run, e) : "default",
          // The flow the run came along to the step it stands on: drawn moving toward it (`theme/flow.css`).
          live: run ? edgeTone(run, e) === "taken" && LIVE_STATES.has(run.steps[e.to]?.state?.state ?? "") : false,
        };
      }),
    [graph, run],
  );

  return (
    <div className="relative h-full w-full">
      <FlowCanvas<StepNodeData>
        label={t("workflow-designer-canvas")}
        nodes={nodes}
        edges={edges}
        nodeTypes={NODE_TYPES}
        readOnly={readOnly}
        snap={settings.snap}
        grid={settings.grid}
        minimap={settings.minimap}
        fitKey={fitKey}
        startViewport={startViewport ?? null}
        onViewport={onViewport}
        acceptMimes={[STEP_MIME]}
        onMeasured={onMeasured}
        reveal={reveal ?? null}
        onSelect={onSelect}
        onMoved={(moves) => {
          const allowed = moves.filter((m) => mayEdit(gate, m.id));
          if (allowed.length < moves.length) onRefuse?.(t("workflow-designer-step-has-already-run-stays-where"));
          if (allowed.length === 0) return;
          // Each card lands on the grid, beside — never under — a card it
          // would cover; the cards of one gesture are placed in turn against
          // everything already placed, so two dropped together do not stack.
          const placed = new Map(positions);
          const next = new Map<string, { x: number; y: number }>();
          for (const m of allowed) {
            const at = nudgeFree(placed, sizes, m.id, snapTo(m.position, settings.grid));
            placed.set(m.id, at);
            next.set(m.id, at);
          }
          commit(setPositions(value, next));
        }}
        onConnect={(source, handle, target) => {
          if (!mayEdit(gate, source)) return onRefuse?.(t("workflow-designer-step-has-already-run"));
          // A side handle draws no flow: an on-fail target is set in the
          // inspector, and a loop is a flow drawn to a step that runs earlier.
          if (handle === HANDLE.fail || handle === HANDLE.loopOut) {
            return onRefuse?.(t("workflow-designer-draw-flows-from-bottom-handle-fail"));
          }
          const r = connect(value, source, target, branchOfHandle(handle));
          if (r.ok) commit(r.wf);
          else onRefuse?.(r.reason);
        }}
        onDelete={(nodeIds, edgeIds) => {
          let next = value;
          let refused = 0;
          for (const id of nodeIds) {
            if (mayEdit(gate, id)) next = removeStep(next, id);
            else refused += 1;
          }
          for (const eid of edgeIds) {
            const e = graph.edges.find((x) => x.id === eid);
            if (!e) continue;
            if (!mayEdit(gate, e.from)) {
              refused += 1;
              continue;
            }
            // A `then` flow is cut; an on-fail route is put back to `fail`,
            // which is what deleting the edge that draws it means.
            next = e.kind === "then" ? disconnect(next, e.from, e.to, e.branch) : setOnFail(next, e.from, { on_fail: "fail" });
          }
          if (refused > 0) onRefuse?.(t("workflow-designer-step-has-already-run-stays-ran"));
          if (next !== value) {
            commit(next);
            onSelect(null);
          }
        }}
        onDuplicate={(id) => {
          if (!mayEdit(gate, null)) return;
          // The copy of a placeless card takes the place the picture draws
          // its source at, then the duplicate's offset from it.
          const source = value.steps.find((s) => s.id === id);
          const base = source && !source.position ? setPositions(value, new Map([[id, positions.get(id) ?? { x: 0, y: 0 }]])) : value;
          const { wf: next, id: copy } = duplicateStep(base, id);
          if (copy) {
            commit(next);
            onSelect(copy);
            onReveal?.(copy);
          }
        }}
        onUndo={onUndo}
        onRedo={onRedo}
        onDropAt={(_mime, kind, point) => {
          if (!mayEdit(gate, null)) return;
          const id = uniqueId(value, kind);
          const at = nudgeFree(positions, sizes, id, snapTo(point, settings.grid));
          const { wf: added } = addStep(value, kind as StepKindName, id, at);
          commit(added);
          onSelect(id);
          onReveal?.(id);
        }}
      />
      <div className="absolute top-2 right-2 flex items-center gap-1 rounded-control border border-border bg-surface/90 p-1">
        {!readOnly && (
          <>
            <Tooltip label={t("workflow-designer-undo-mod-z")}>
              <Button size="sm" variant="ghost" disabled={!canUndo} onClick={onUndo} aria-label={t("workflow-designer-undo")}>
                <ICON.undo size={13} aria-hidden />
              </Button>
            </Tooltip>
            <Tooltip label={t("workflow-designer-redo-mod-shift-z")}>
              <Button size="sm" variant="ghost" disabled={!canRedo} onClick={onRedo} aria-label={t("workflow-designer-redo")}>
                <ICON.redo size={13} aria-hidden />
              </Button>
            </Tooltip>
          </>
        )}
        {!readOnly && (
          // While it may not tidy, the button says why itself (`disabledReason`): a disabled button takes no hover.
          <Tooltip label={mayTidy ? t("workflow-designer-tidy-lay-every-step-out-again") : undefined}>
            <Button
              size="sm"
              variant="ghost"
              disabled={!mayTidy}
              disabledReason={t("workflow-designer-step-has-already-run-stays-where")}
              aria-label={t("workflow-designer-tidy")}
              onClick={() => {
                onChange(tidy(value, { sizes }));
                setFitKey((k) => k + 1);
              }}
            >
              <ICON.layout size={13} aria-hidden />
            </Button>
          </Tooltip>
        )}
        <Tooltip label={t("workflow-designer-fit-whole-workflow")}>
          <Button size="sm" variant="ghost" onClick={() => setFitKey((k) => k + 1)} aria-label={t("workflow-designer-fit")}>
            <ICON.fit size={13} aria-hidden />
          </Button>
        </Tooltip>
      </div>
    </div>
  );
}
