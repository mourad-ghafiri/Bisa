/**
 * The rail's drag contract (ide/07): what a row carries when lifted, which
 * headings and parents take it, and what a drop the tree planned *means* —
 * the next `rail.order`, a project moving house, or a shell's place in the
 * session order. Pure, so the rules are assertable without a DOM;
 * `ProjectRail.tsx` binds them and keeps no rule of its own.
 *
 * Payloads speak **record ids**. A project row's own id is `<under>:<id>` so
 * the same project can stand under a goal and under a group, but the order
 * the setting keeps and the section a heading folds are by the record's id
 * and by `under` — the one namespace every side of a drag agrees on.
 */

import { railRowDrag } from "../../ui/dnd/dragData.mjs";
import { childrenOf, parentsFromDepth } from "../../ui/tree/treeListModel.mjs";
import { UNGROUPED } from "./projectRailModel.mjs";
import { groupOrder, projectOrder, reorderAmong, workstreamOrder } from "./railOrderModel.mjs";
import { t as tr } from "../../i18n/l10n.mjs";

/** A section a project can be moved into by a drop: a named group, or none. */
function isGroupSection(under) {
  return under === "ungrouped" || String(under ?? "").startsWith("group:");
}

function isRail(data) {
  return !!data && typeof data === "object" && data.type === "rail-row";
}

/** A rail with no heading at all — nobody made a group, every project at the top. */
export function isHeadless(rows) {
  return !rows.some((r) => r.kind === "group" || r.kind === "goal");
}

/**
 * What a row carries when dragged; `null` for one that stays put. A group
 * drags only among the named groups of the Workspace tab (*Other projects*,
 * a goal and a workflow heading are folds, not things); agent rows do not
 * move; the row being renamed is a field, not a handle.
 * @param {object} row a `RailRow`
 * @param {{renaming?: {kind: string, id: string} | null}} [opts]
 */
export function railDragOf(row, opts = {}) {
  const renaming = opts.renaming ?? null;
  switch (row.kind) {
    case "group":
      return String(row.under ?? "").startsWith("group:") ? railRowDrag("group", row.id, "", row.label) : null;
    case "project":
      if (renaming && renaming.kind === "project" && renaming.id === row.project.id) return null;
      return railRowDrag("project", row.project.id, row.under, row.project.name);
    case "workstream":
      if (renaming && renaming.kind === "workstream" && renaming.id === row.id) return null;
      return railRowDrag("workstream", row.id, row.project, row.label);
    case "terminal":
      return railRowDrag("terminal", row.id, row.workstream, row.label);
    default:
      return null;
  }
}

/** A heading takes a project that is already its own, or — both being project groups — one from another group. */
function headingTakes(data, heading) {
  if (!heading || (heading.kind !== "group" && heading.kind !== "goal")) return false;
  return heading.under === data.ctx || (isGroupSection(heading.under) && isGroupSection(data.ctx));
}

/**
 * Whether a payload may nest into a row: a project into a heading that takes
 * it, a workstream into its own project, a shell into its own workstream.
 * @param {object} data @param {object} target a `RailRow`
 */
export function railCanNest(data, target) {
  if (!isRail(data)) return false;
  switch (data.kind) {
    case "project":
      return headingTakes(data, target);
    case "workstream":
      return target.kind === "project" && target.project.id === data.ctx;
    case "terminal":
      return target.kind === "workstream" && target.id === data.ctx;
    default:
      return false;
  }
}

/**
 * Whether a payload may land among a parent's children (`null` for the top):
 * a group at the top; a project under a heading that takes it, or at the top
 * of a headless rail; a workstream under its project; a shell under its
 * workstream.
 * @param {object} data @param {object | null} parent a `RailRow` or null
 * @param {{headless?: boolean}} [opts]
 */
export function railCanReorder(data, parent, opts = {}) {
  if (!isRail(data)) return false;
  switch (data.kind) {
    case "group":
      return parent === null;
    case "project":
      return parent === null ? Boolean(opts.headless) : headingTakes(data, parent);
    case "workstream":
      return parent !== null && parent.kind === "project" && parent.project.id === data.ctx;
    case "terminal":
      return parent !== null && parent.kind === "workstream" && parent.id === data.ctx;
    default:
      return false;
  }
}

/**
 * The whole list a dimension keeps: the saved ids first (a project folded
 * away under a collapsed group is still in its place), then what is shown
 * and not yet saved, in display order — so nothing is forgotten by a drop.
 */
function wholeList(saved, shown) {
  const seen = new Set(saved);
  return [...saved, ...shown.filter((id) => !seen.has(id) && (seen.add(id), true))];
}

function orderVerdict(order, next) {
  return next === order ? null : { kind: "order", order: next };
}

/**
 * What a drop the tree planned means. `rows` are the rail's rows as drawn,
 * `treeRows` the tree's rows over them (`treeRowsOf`), `plan` the projection
 * (`dropPlan`). Answers `{kind: "order", order}` — the next `rail.order` —
 * `{kind: "regroup", project, group, order, words}` — a project into another
 * group (`group` null for *Other projects*), placed where it was dropped —
 * `{kind: "sessions", key, before}` — a shell's place among its workstream's
 * shells — or `null` when nothing would change.
 * @param {object} order the current `rail.order`
 * @param {object} data @param {object | null} plan
 * @param {readonly object[]} rows @param {readonly object[]} treeRows
 */
export function railDropVerdict(order, data, plan, rows, treeRows) {
  if (!isRail(data) || !plan || plan.noop) return null;
  const parents = parentsFromDepth(treeRows);
  const byId = new Map(treeRows.map((t) => [t.id, t.row]));
  const siblings = childrenOf(treeRows, parents, plan.parent)
    .map((id) => byId.get(id))
    .filter(Boolean);
  const parent = plan.parent ? (byId.get(plan.parent) ?? null) : null;
  switch (data.kind) {
    case "group": {
      const shown = rows.filter((r) => r.kind === "group" && String(r.under ?? "").startsWith("group:")).map((r) => r.id);
      const sibs = siblings.filter((r) => r.kind === "group").map((r) => r.id);
      return orderVerdict(order, reorderAmong(order, { kind: "groups" }, wholeList(groupOrder(order), shown), sibs, data.id, plan.index));
    }
    case "project": {
      const shown = rows.filter((r) => r.kind === "project").map((r) => r.project.id);
      const sibs = siblings.filter((r) => r.kind === "project").map((r) => r.project.id);
      const next = reorderAmong(order, { kind: "projects" }, wholeList(projectOrder(order), shown), sibs, data.id, plan.index);
      // Into another group: the project moves house, then takes its slot there.
      if (parent && parent.kind === "group" && isGroupSection(parent.under) && parent.under !== data.ctx) {
        const group = parent.id === UNGROUPED ? null : parent.id;
        return { kind: "regroup", project: data.id, group, order: next, words: group ? tr("workbench-rail-drag-moved", { group }) : tr("workbench-project-rail-removed-from-group") };
      }
      return orderVerdict(order, next);
    }
    case "workstream": {
      const shown = rows.filter((r) => r.kind === "workstream" && r.project === data.ctx).map((r) => r.id);
      const all = wholeList(workstreamOrder(order, data.ctx), shown);
      return orderVerdict(order, reorderAmong(order, { kind: "workstreams", project: data.ctx }, all, all, data.id, plan.index));
    }
    case "terminal": {
      // A workstream's children mix shells and agent rows; the session order
      // counts shells only, so the plan's index is read among those.
      const before = siblings.slice(0, plan.index).filter((r) => r.kind === "terminal" && r.id !== data.id).length;
      return { kind: "sessions", key: data.id, before };
    }
    default:
      return null;
  }
}
