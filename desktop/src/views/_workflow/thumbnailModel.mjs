/**
 * A workflow's thumbnail, as shapes (03-workflows §The library): the picture
 * the designer opens on, read off the definition without the canvas. The
 * cards stand where `layout` puts them — a step's own position, the layered
 * layout for a step that has none — at the kinds' estimated sizes, so a
 * thumbnail and the canvas draw one picture; each card says its family, so
 * an event is drawn as the canvas's pill. The flows are the graph's
 * (`toGraph`): a `then` flow leaves the source's bottom, at the branch's own
 * handle when the step branches, and arrives at the target's top; a
 * divert's path leaves the bottom at its boundary's handle, off the card's
 * lower right (`boundaryModel.divertOffsets`); an on-fail route leaves the
 * source's left; a loop leaves the source's right, goes out and comes back
 * to the target's right — the same sides the canvas's handles own. Pure
 * data in, pure shapes out; `WorkflowThumbnail.tsx` paints them as one SVG.
 * Plain `.mjs`, so `node --test` reads it.
 */

import { familyOf, kindBranchesOf } from "./stepKinds.mjs";
import { OUT_WITH_DIVERTS, divertOffsets } from "./forms/boundaryModel.mjs";
import { toGraph } from "./workflowGraph.mjs";
import { NODE_HEIGHT, NODE_WIDTH, layout } from "./workflowLayout.mjs";

/** The margin around the picture, in canvas pixels. */
export const PAD = 24;
/** How far a loop leaves the cards' column before it turns back. */
export const LOOP_OUT = 40;
/** Where a loop leaves a card's right edge, and where it arrives — the canvas's handle offsets. */
const LOOP_OUT_AT = 0.65;
const LOOP_IN_AT = 0.35;
/** Where an on-fail route leaves a card's left edge. */
const FAIL_AT = 0.65;

/**
 * @typedef {{id: string, kind: string, family: "event" | "gateway" | "loop" | "task", name: string, x: number, y: number, w: number, h: number}} ThumbNode
 * @typedef {{id: string, kind: "then" | "on_fail" | "loop" | "boundary", points: {x: number, y: number}[]}} ThumbEdge
 * @typedef {{viewBox: {x: number, y: number, w: number, h: number}, nodes: ThumbNode[], edges: ThumbEdge[]}} Thumbnail
 */

/**
 * Where a step's own flow leaves its bottom edge: a gateway's or a loop's
 * branch at its handle, spread across the edge as the canvas spreads them;
 * a plain step's one flow at the centre — or at the left once a divert
 * shares the edge.
 */
function leaveAt(node, step, branch) {
  const branches = kindBranchesOf(step);
  if (branches.length > 0) {
    const i = Math.max(0, branches.indexOf(branch));
    return node.x + (node.w * (i + 1)) / (branches.length + 1);
  }
  return divertOffsets(step).size > 0 ? node.x + (node.w * OUT_WITH_DIVERTS) / 100 : node.x + node.w / 2;
}

/**
 * The shapes a definition previews as, and the box around them.
 * @param {{steps?: readonly Record<string, any>[]}} wf
 * @returns {Thumbnail}
 */
export function thumbnail(wf) {
  const { positions } = layout(wf);
  const { nodes: graphNodes, edges: graphEdges } = toGraph(wf);
  if (graphNodes.length === 0) return { viewBox: { x: 0, y: 0, w: 1, h: 1 }, nodes: [], edges: [] };

  /** @type {Map<string, ThumbNode>} */
  const byId = new Map();
  /** @type {ThumbNode[]} */
  const nodes = graphNodes.map((n) => {
    const p = positions.get(n.id) ?? { x: 0, y: 0 };
    const node = { id: n.id, kind: n.step.kind, family: familyOf(n.step.kind), name: n.step.name || n.id, x: p.x, y: p.y, w: NODE_WIDTH, h: NODE_HEIGHT };
    byId.set(n.id, node);
    return node;
  });
  const stepOf = (id) => graphNodes.find((n) => n.id === id)?.step;

  /** @type {{x: number, y: number}[]} */
  const points = nodes.flatMap((n) => [
    { x: n.x, y: n.y },
    { x: n.x + n.w, y: n.y + n.h },
  ]);
  /** @type {ThumbEdge[]} */
  const edges = [];
  for (const e of graphEdges) {
    const from = byId.get(e.from);
    const to = byId.get(e.to);
    // A flow to a step that is not there is the validator's problem, not a line.
    if (!from || !to) continue;
    if (e.loop) {
      const out = Math.max(from.x + from.w, to.x + to.w) + LOOP_OUT;
      const a = { x: from.x + from.w, y: from.y + from.h * LOOP_OUT_AT };
      const b = { x: to.x + to.w, y: to.y + to.h * LOOP_IN_AT };
      edges.push({ id: e.id, kind: "loop", points: [a, { x: out, y: a.y }, { x: out, y: b.y }, b] });
      points.push({ x: out, y: a.y });
      continue;
    }
    const arrive = { x: to.x + to.w / 2, y: to.y };
    if (e.kind === "on_fail") {
      const a = { x: from.x, y: from.y + from.h * FAIL_AT };
      edges.push({ id: e.id, kind: "on_fail", points: [a, arrive] });
      continue;
    }
    const step = stepOf(e.from);
    if (e.kind === "boundary") {
      const at = divertOffsets(step).get(e.branch) ?? 75;
      edges.push({ id: e.id, kind: "boundary", points: [{ x: from.x + (from.w * at) / 100, y: from.y + from.h }, arrive] });
      continue;
    }
    edges.push({ id: e.id, kind: "then", points: [{ x: leaveAt(from, step, e.branch), y: from.y + from.h }, arrive] });
  }

  const minX = Math.min(...points.map((p) => p.x)) - PAD;
  const minY = Math.min(...points.map((p) => p.y)) - PAD;
  const maxX = Math.max(...points.map((p) => p.x)) + PAD;
  const maxY = Math.max(...points.map((p) => p.y)) + PAD;
  return { viewBox: { x: minX, y: minY, w: Math.max(1, maxX - minX), h: Math.max(1, maxY - minY) }, nodes, edges };
}
