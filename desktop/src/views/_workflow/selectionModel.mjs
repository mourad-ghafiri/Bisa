/**
 * What is picked on a workflow's canvas, as one value the canvas and the
 * panel share (03-workflows §The designer): nothing, **a step**, **a flow**
 * — one edge, named by its ends and its branch, since a flow has no id of
 * its own in the definition — or **several** of either, from a box select.
 * The panel reads the value to decide which view it draws; the canvas marks
 * its nodes and edges from it; the history stamps it on every entry so an
 * undo lands where the person was.
 *
 * A flow is addressed the way `workflowGraph.toGraph` names its edges
 * (`from->to`, `from->to#branch`, `from!>to` for a failure route), so the id
 * the canvas reports and the reference the panel holds round-trip exactly.
 * Step ids are `a–z`, `0–9`, `-` and `_` (`workflowForm.idProblem`), so the
 * arrows and the hash can never be part of one.
 *
 * What is kept across visits is a step and nothing else (`storedSelection`,
 * `readSelection`): the memory outlives the window and was written as a bare
 * step id before this model existed, and a flow or a box selection is a
 * moment's pick. Facts only, no React. Plain `.mjs`, so `node --test` reads it.
 */

import { t } from "../../i18n/l10n.mjs";
import { divertNamesOf, isBranching } from "./stepKinds.mjs";
import { toGraph } from "./workflowGraph.mjs";

/** The longest id a memory gives back as a step. */
const MAX_ID = 512;

/** The kinds whose rule, case or option names may be relabelled from the panel (`workflowGraph.relabelBranch`). */
const RELABELS = new Set(["decide", "switch", "judge"]);

/** A step picked, or nothing for no id. @param {string | null | undefined} id */
export function stepSelection(id) {
  return id ? { kind: "step", id } : null;
}

/** The step a selection is, or `null` — a flow and a many are no step. @param {unknown} sel */
export function stepOf(sel) {
  return sel && typeof sel === "object" && sel.kind === "step" ? sel.id : null;
}

/**
 * One flow by its ends: a plain flow, a branch's, or — `route: "on_fail"` —
 * a step's failure route, which has no branch.
 * @param {string} from
 * @param {string} to
 * @param {string | null} [branch]
 * @param {"then" | "on_fail"} [route]
 */
export function flowRef(from, to, branch = null, route = "then") {
  return { kind: "flow", from, to, branch: route === "on_fail" ? null : (branch ?? null), route };
}

/** The flow a graph's edge is (`workflowGraph.toGraph`). @param {{from: string, to: string, branch: string | null, kind: string}} edge */
export function flowRefOf(edge) {
  return flowRef(edge.from, edge.to, edge.branch, edge.kind === "on_fail" ? "on_fail" : "then");
}

/** The id `toGraph` gives the edge of a flow. @param {{from: string, to: string, branch: string | null, route: string}} ref */
export function edgeIdOf(ref) {
  if (ref.route === "on_fail") return `${ref.from}!>${ref.to}`;
  return `${ref.from}->${ref.to}${ref.branch ? `#${ref.branch}` : ""}`;
}

/** The flow an edge id names, or `null` for an id no edge of ours has. @param {unknown} id */
export function flowRefOfEdgeId(id) {
  if (typeof id !== "string") return null;
  const fail = id.indexOf("!>");
  if (fail > 0) {
    const to = id.slice(fail + 2);
    return to && !to.includes("#") ? flowRef(id.slice(0, fail), to, null, "on_fail") : null;
  }
  const arrow = id.indexOf("->");
  if (arrow <= 0) return null;
  const rest = id.slice(arrow + 2);
  const hash = rest.indexOf("#");
  const to = hash < 0 ? rest : rest.slice(0, hash);
  const branch = hash < 0 ? null : rest.slice(hash + 1);
  if (!to || branch === "") return null;
  return flowRef(id.slice(0, arrow), to, branch);
}

const sameFlow = (a, b) => a.from === b.from && a.to === b.to && a.branch === b.branch && a.route === b.route;

/** Whether two selections pick the same thing. @param {unknown} a @param {unknown} b */
export function sameSelection(a, b) {
  if (a === b) return true;
  if (!a || !b || a.kind !== b.kind) return false;
  if (a.kind === "step") return a.id === b.id;
  if (a.kind === "flow") return sameFlow(a, b);
  return a.steps.length === b.steps.length && a.steps.every((id, i) => id === b.steps[i]) && a.flows.length === b.flows.length && a.flows.every((f, i) => sameFlow(f, b.flows[i]));
}

/**
 * The selection of what the canvas reports picked — node ids and edge ids.
 * One step alone is a step, one flow alone a flow, nothing is nothing, and
 * anything more is a many; an edge id no flow of ours has is dropped.
 * @param {{nodes: readonly string[], edges: readonly string[]}} pick
 */
export function selectionOf(pick) {
  const steps = [...(pick?.nodes ?? [])];
  const flows = (pick?.edges ?? []).map(flowRefOfEdgeId).filter((f) => f !== null);
  return shape(steps, flows);
}

/** The one shape for a count of steps and flows: nothing, one, or many. */
function shape(steps, flows) {
  if (steps.length + flows.length === 0) return null;
  if (steps.length === 1 && flows.length === 0) return stepSelection(steps[0]);
  if (steps.length === 0 && flows.length === 1) return flows[0];
  return { kind: "many", steps, flows };
}

/** The node ids and edge ids a selection marks on the canvas. @param {unknown} sel */
export function selectedIds(sel) {
  if (!sel) return { nodes: [], edges: [] };
  if (sel.kind === "step") return { nodes: [sel.id], edges: [] };
  if (sel.kind === "flow") return { nodes: [], edges: [edgeIdOf(sel)] };
  return { nodes: [...sel.steps], edges: sel.flows.map(edgeIdOf) };
}

/**
 * The selection narrowed to what the workflow still has — a remembered step
 * an agent's save took, a flow a delete cut, are no pick — the same object
 * when nothing is lost, so a render on the same value changes nothing.
 * @param {unknown} sel
 * @param {{steps?: readonly {id: string}[]} | null | undefined} wf
 */
export function keptSelection(sel, wf) {
  if (!sel) return null;
  const graph = toGraph(wf ?? { steps: [] });
  const ids = new Set(graph.nodes.map((n) => n.id));
  const edges = new Set(graph.edges.map((e) => e.id));
  return narrow(sel, (id) => ids.has(id), (f) => edges.has(edgeIdOf(f)));
}

/**
 * The selection after a delete of these nodes and edges: what was deleted
 * leaves it, what remains stays picked — a step whose flow was cut stays
 * selected — and the same object comes back when nothing of it went.
 * @param {unknown} sel
 * @param {readonly string[]} nodeIds
 * @param {readonly string[]} edgeIds
 */
export function selectionAfterDelete(sel, nodeIds, edgeIds) {
  if (!sel) return null;
  const nodes = new Set(nodeIds);
  const edges = new Set(edgeIds);
  // A flow out of or into a deleted step goes with the step.
  return narrow(sel, (id) => !nodes.has(id), (f) => !edges.has(edgeIdOf(f)) && !nodes.has(f.from) && !nodes.has(f.to));
}

function narrow(sel, keepStep, keepFlow) {
  if (sel.kind === "step") return keepStep(sel.id) ? sel : null;
  if (sel.kind === "flow") return keepFlow(sel) ? sel : null;
  const steps = sel.steps.filter(keepStep);
  const flows = sel.flows.filter(keepFlow);
  if (steps.length === sel.steps.length && flows.length === sel.flows.length) return sel;
  return shape(steps, flows);
}

/**
 * A selection as the memory or a link gives it back, made safe to open on:
 * a bare id — how it was always written — or a step or a flow by shape; a
 * many, or anything else, is nothing.
 * @param {unknown} raw
 */
export function readSelection(raw) {
  if (typeof raw === "string") return raw.length > 0 && raw.length <= MAX_ID ? stepSelection(raw) : null;
  if (!raw || typeof raw !== "object") return null;
  if (raw.kind === "step") return readSelection(raw.id);
  if (raw.kind === "flow") {
    const { from, to, branch = null, route = "then" } = raw;
    const id = (x) => typeof x === "string" && x.length > 0 && x.length <= MAX_ID;
    if (!id(from) || !id(to) || (route !== "then" && route !== "on_fail") || (branch !== null && !id(branch))) return null;
    return flowRef(from, to, branch, route);
  }
  return null;
}

/** What is kept of a selection across visits: the step, as a bare id, or nothing. @param {unknown} sel */
export function storedSelection(sel) {
  return stepOf(sel);
}

/**
 * The name a properties pane keeps its scroll under: each step's page its
 * own — `properties:<id>`, as it was always spelt — a flow's its own too,
 * and the workflow's the bare name.
 * @param {unknown} sel
 */
export function propertiesScrollName(sel) {
  if (!sel) return "properties:";
  if (sel.kind === "step") return `properties:${sel.id}`;
  if (sel.kind === "flow") return `properties:flow:${edgeIdOf(sel)}`;
  return "properties:many";
}

/**
 * The selection once a branch of `from` was relabelled: a picked flow on the
 * old name follows it to the new one, so the panel keeps showing the flow the
 * person is editing. Anything else is left alone.
 * @param {unknown} sel
 * @param {string} from
 * @param {string} was
 * @param {string} now
 */
export function flowRefAfterRelabel(sel, from, was, now) {
  if (!sel || sel.kind !== "flow" || sel.from !== from || sel.branch !== was || sel.route !== "then") return sel;
  return flowRef(from, sel.to, now);
}

/**
 * What the panel says about one flow: its two steps, its word — a plain
 * flow, a branch's, the otherwise, a boundary's path, the failure route —
 * whether it is a loop back, and whether its branch may be renamed here.
 * `exists` is false for a flow the workflow no longer has.
 * @param {{steps?: readonly object[]} | null | undefined} wf
 * @param {{from: string, to: string, branch: string | null, route: string}} ref
 */
export function flowFacts(wf, ref) {
  const steps = wf?.steps ?? [];
  const from = steps.find((s) => s.id === ref.from) ?? null;
  const to = steps.find((s) => s.id === ref.to) ?? null;
  const edge = toGraph(wf ?? { steps: [] }).edges.find((e) => e.id === edgeIdOf(ref)) ?? null;
  const boundary = edge?.kind === "boundary" || (!!from && !isBranching(from) && ref.branch !== null && divertNamesOf(from).includes(ref.branch));
  const word =
    ref.route === "on_fail"
      ? t("workflow-inspector-flow-on-failure")
      : boundary
        ? t("workflow-inspector-flow-boundary", { branch: ref.branch })
        : ref.branch === null
          ? t("workflow-inspector-flow-then")
          : from?.otherwise === ref.branch
            ? t("workflow-inspector-flow-otherwise")
            : t("workflow-inspector-flow-branch", { branch: ref.branch });
  const relabel = !!from && RELABELS.has(from.kind) && ref.route === "then" && ref.branch !== null && from.otherwise !== ref.branch && !boundary;
  return { exists: edge !== null, from, to, branch: ref.branch, route: ref.route, loop: edge?.loop ?? false, boundary, word, relabel };
}
