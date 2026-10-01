/**
 * Where each step is drawn.
 *
 * A step's position is **its own**: `Step.position` on the definition, in
 * canvas pixels snapped to the grid, written the moment a person moves a card,
 * drops one or edits the canvas at all (`withPositions`). A step that has none
 * yet — a template just installed, a proposal the Workflow Agent made, a
 * step a CLI added — is placed by the layered layout (dagre) **at the kinds'
 * estimated sizes**, so the derived picture never moves under a name being
 * typed, and the same definition draws the same picture on every machine
 * until somebody makes it theirs. *Tidy* lays every step out again, at the
 * cards' measured sizes, and puts `steps` in reading order.
 *
 * Nothing here routes a flow: a flow is a smooth step from handle to handle,
 * drawn by the canvas; a loop leaves by the side handles. Loop edges — the
 * ones the run machine treats as loops, from `backEdges` — are kept out of
 * the layered ranking and listed, so the canvas knows which ones they are.
 * A side path — a divert's, leaving a card's lower right, or an on-fail
 * route — pulls its target less than a step's own flow, and a step reached
 * only that way stands to the right of its rank, so the main path reads
 * straight down and the escalation sits off to the side.
 */

import dagre from "@dagrejs/dagre";
import { toGraph } from "./workflowGraph.mjs";

/** The canvas's smallest step: every stored position is a multiple of it. */
export const GRID = 8;
/** Every kind's card is one width — an event's pill, a gateway's diamond glyph and a task alike. */
export const NODE_WIDTH = 220;
/** The height of a card with nothing but its two title rows. */
export const NODE_HEIGHT = 72;
/** The room a card's boundary chips take below its lower edge. */
export const BOUNDARY_ROW = 16;
/** Horizontal room between two nodes of one rank. */
const GAP_X = 56;
/** Vertical room between two ranks — where a flow's label and arrow live. */
export const GAP_Y = 72;
/** How far a nudge searches before it gives up and stacks below. */
const NUDGE_TRIES = 64;

const ESTIMATE = Object.freeze({ width: NODE_WIDTH, height: NODE_HEIGHT });
const WITH_CHIPS = Object.freeze({ width: NODE_WIDTH, height: NODE_HEIGHT + BOUNDARY_ROW });

/**
 * A card's estimated size, before the canvas has measured it: one size for
 * every kind — the three event and gateway kinds, `start`, `emit` and
 * `parallel`, included — and a row taller for a step whose boundary chips
 * sit on its lower edge.
 * @param {{boundaries?: readonly unknown[]} | null | undefined} step
 */
export function estimateOf(step) {
  return (step?.boundaries?.length ?? 0) > 0 ? WITH_CHIPS : ESTIMATE;
}

/**
 * A point on the grid, whole pixels.
 * @param {{x: number, y: number}} point
 * @param {number} [grid]
 */
export function snapTo(point, grid = GRID) {
  const g = Math.max(1, Math.trunc(grid) || GRID);
  // `|| 0` turns the `-0` a rounded negative fraction leaves into a plain zero.
  return { x: Math.round(point.x / g) * g || 0, y: Math.round(point.y / g) * g || 0 };
}

/** A card's box: its position and its measured size, else the estimate. */
function boxOf(positions, sizes, id) {
  const p = positions.get(id);
  if (!p) return null;
  const s = sizes?.get(id) ?? ESTIMATE;
  return { x: p.x, y: p.y, width: s.width, height: s.height };
}

/**
 * Whether two boxes share any pixel.
 * @param {{x: number, y: number, width: number, height: number}} a
 * @param {{x: number, y: number, width: number, height: number}} b
 */
export function overlaps(a, b) {
  return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
}

/**
 * The nearest free place for `id` dropped at `point`: the point itself when
 * no other card is under it; else pushed right past the card it covers, on
 * the grid, and — should the row stay full — down under the lowest card it
 * met. Deterministic, so a drop onto a card lands beside it and never hides
 * it. The point is taken as given: the caller snapped it when it wanted to.
 * @param {Map<string, {x: number, y: number}>} positions every card's place, `id` included or not
 * @param {Map<string, {width: number, height: number}> | null | undefined} sizes measured sizes, by id
 * @param {string} id the card being placed
 * @param {{x: number, y: number}} point where it was dropped, snapped
 * @param {number} [grid]
 */
export function nudgeFree(positions, sizes, id, point, grid = GRID) {
  const mine = sizes?.get(id) ?? ESTIMATE;
  const others = [...positions.keys()].filter((k) => k !== id).map((k) => boxOf(positions, sizes, k)).filter(Boolean);
  // A free drop is left exactly where it was put; only a push snaps.
  let at = { x: point.x, y: point.y };
  const covered = () => others.filter((b) => overlaps({ ...at, ...mine }, b));
  let lowest = at.y;
  for (let i = 0; i < NUDGE_TRIES; i += 1) {
    const hits = covered();
    if (hits.length === 0) return at;
    const rightmost = Math.max(...hits.map((b) => b.x + b.width));
    lowest = Math.max(lowest, ...hits.map((b) => b.y + b.height));
    at = snapTo({ x: rightmost + GAP_X / 2, y: at.y }, grid);
  }
  at = snapTo({ x: point.x, y: lowest + GAP_Y / 2 }, grid);
  return covered().length === 0 ? at : snapTo({ x: point.x, y: lowest + GAP_Y }, grid);
}

/**
 * The layered layout of the whole graph — dagre's Sugiyama with dummy nodes
 * for long edges and crossing reduction — as top-left corners on the grid,
 * loops excluded from the ranking, rank-mates in `steps` order.
 * @returns {Map<string, {x: number, y: number}>}
 */
function layered(wf, sizes) {
  const { nodes, edges } = toGraph(wf);
  const ids = new Set(nodes.map((n) => n.id));
  const g = new dagre.graphlib.Graph({ multigraph: true });
  g.setGraph({ rankdir: "TB", nodesep: GAP_X, ranksep: GAP_Y, edgesep: GAP_X / 2, marginx: 0, marginy: 0 });
  g.setDefaultEdgeLabel(() => ({}));
  for (const n of nodes) {
    const s = sizes?.get(n.id) ?? estimateOf(n.step);
    g.setNode(n.id, { width: s.width, height: s.height, order: n.order });
  }
  for (const e of edges) {
    // A flow to a step that does not exist is a problem the inspector shows,
    // not a phantom row; a loop is drawn, not ranked; a side path pulls less.
    if (e.loop || !ids.has(e.to) || !ids.has(e.from)) continue;
    g.setEdge(e.from, e.to, { weight: e.kind === "then" ? 2 : 1 }, e.id);
  }
  dagre.layout(g);
  stepsOrderWithinRanks(g, nodes, sideOnly(edges));
  const positions = new Map();
  for (const n of nodes) {
    const p = g.node(n.id);
    positions.set(n.id, snapTo({ x: p.x - p.width / 2, y: p.y - p.height / 2 }));
  }
  return positions;
}

/**
 * The steps a side path alone reaches — a divert's or an on-fail route's
 * target that no step's own flow leads to: they stand to the right of their
 * rank.
 * @param {readonly {kind: string, to: string, loop: boolean}[]} edges
 */
export function sideOnly(edges) {
  const main = new Set(edges.filter((e) => e.kind === "then" && !e.loop).map((e) => e.to));
  return new Set(edges.filter((e) => e.kind !== "then" && !e.loop && !main.has(e.to)).map((e) => e.to));
}

/**
 * Within one rank, the designer's order — the order of `steps` — wins over
 * dagre's, save that a step only a side path reaches goes right of the rest.
 * dagre settles rank-mates by its crossing heuristic, which can put the
 * second arm of a diamond first; the x coordinates it chose are kept as a
 * set and handed out left to right in that order.
 */
function stepsOrderWithinRanks(g, nodes, side = new Set()) {
  const ranks = new Map();
  for (const n of nodes) {
    const y = g.node(n.id).y;
    if (!ranks.has(y)) ranks.set(y, []);
    ranks.get(y).push(n);
  }
  for (const members of ranks.values()) {
    if (members.length < 2) continue;
    const xs = members.map((n) => g.node(n.id).x).sort((a, b) => a - b);
    [...members].sort((a, b) => Number(side.has(a.id)) - Number(side.has(b.id)) || a.order - b.order).forEach((n, i) => {
      g.node(n.id).x = xs[i];
    });
  }
}

/** The box around every card, at the estimated size. */
function extent(positions) {
  let width = 0;
  let height = 0;
  for (const p of positions.values()) {
    width = Math.max(width, p.x + NODE_WIDTH);
    height = Math.max(height, p.y + NODE_HEIGHT);
  }
  return { width, height };
}

/**
 * Where each step is drawn: its own position when it has one; else the
 * layered layout's, at the estimated sizes, moved as one block to start one
 * `GAP_Y` below the placed cards and nudged free of them. Deterministic.
 * @param {object} wf the definition
 * @returns {{positions: Map<string, {x: number, y: number}>, loops: Set<string>, size: {width: number, height: number}}}
 */
export function layout(wf) {
  const { nodes, edges } = toGraph(wf);
  const loops = new Set(edges.filter((e) => e.loop).map((e) => e.id));
  if (nodes.length === 0) return { positions: new Map(), loops, size: { width: 0, height: 0 } };
  const positions = new Map();
  const missing = [];
  for (const n of nodes) {
    // A stored position is drawn as written: the canvas snapped it when it
    // was placed, and one a template wrote by hand is the author's.
    if (n.step.position) positions.set(n.id, { x: n.step.position.x, y: n.step.position.y });
    else missing.push(n.id);
  }
  if (missing.length > 0) {
    const derived = layered(wf, null);
    let dx = 0;
    let dy = 0;
    if (positions.size > 0) {
      const left = Math.min(...[...positions.values()].map((p) => p.x));
      const bottom = Math.max(...[...positions.values()].map((p) => p.y + NODE_HEIGHT));
      const block = missing.map((id) => derived.get(id));
      dx = left - Math.min(...block.map((p) => p.x));
      dy = bottom + GAP_Y - Math.min(...block.map((p) => p.y));
    }
    for (const id of missing) {
      const p = derived.get(id);
      positions.set(id, { x: p.x + dx, y: p.y + dy });
    }
    for (const id of missing) positions.set(id, nudgeFree(positions, null, id, positions.get(id)));
  }
  return { positions, loops, size: extent(positions) };
}

/**
 * The definition with every position-less step given the place the picture
 * draws it at — what the first canvas edit writes, so the picture is the
 * person's from then on. A step that has a position keeps it; a definition
 * with nothing missing comes back as it is.
 * @template D
 * @param {D} wf
 * @param {Map<string, {x: number, y: number}>} positions
 * @returns {D}
 */
export function withPositions(wf, positions) {
  const steps = wf.steps ?? [];
  if (steps.every((s) => s.position || !positions.has(s.id))) return wf;
  return { ...wf, steps: steps.map((s) => (s.position || !positions.has(s.id) ? s : { ...s, position: positions.get(s.id) })) };
}

/**
 * Every step laid out again by the layered layout — at the cards' measured
 * sizes when the canvas has them — and `steps` put in reading order, top to
 * bottom then left to right, so the list the CLI prints matches the picture.
 * One edit; undo puts every card back.
 * @template D
 * @param {D} wf
 * @param {{sizes?: Map<string, {width: number, height: number}>}} [options]
 * @returns {D}
 */
export function tidy(wf, options = {}) {
  const steps = wf.steps ?? [];
  if (steps.length === 0) return wf;
  const positions = layered(wf, options.sizes ?? null);
  const placed = steps.map((s) => ({ ...s, position: positions.get(s.id) ?? s.position ?? null }));
  const at = (s) => s.position ?? { x: 0, y: 0 };
  const sorted = placed
    .map((s, i) => ({ s, i }))
    .sort((a, b) => at(a.s).y - at(b.s).y || at(a.s).x - at(b.s).x || a.i - b.i)
    .map(({ s }) => (s.position === null ? (({ position: _p, ...rest }) => rest)(s) : s));
  return { ...wf, steps: sorted };
}
