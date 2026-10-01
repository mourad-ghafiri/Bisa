/**
 * The project rail's rows (layout, D5/D12): projects → their
 * workstreams → the sessions standing in them, as one flat list for a
 * virtualised viewport — the whole rule, with no React in it.
 *
 * Three tabs that are **three origins**: a project sits in exactly
 * one tab, the one its `origin` names — *Workspace* holds the projects a
 * person made with no goal in hand, grouped by `Project.group`; *Goals* holds
 * the ones made from a goal, under the goal that made them; *Workflows* holds
 * the ones an agent made running a step, under the workflow whose step it
 * was. `tabOf` is the one rule; each tab folds its own projects into
 * sections, and one emitter turns sections into rows. Attachment is still the
 * one relation a project has to a goal (04-workspace-project-goal) — it is
 * shown on the project, never as a tab.
 *
 * Order is stable — sections by name, projects by name, the primary first then
 * by creation — so nothing reshuffles under the pointer. Attention shows in the
 * dot ({@link workstreamActivity}) and the row's wash, never in the order; the
 * tab carries one badge, its project count ({@link railCounts}).
 */

import { harnessOf } from "../../shell/terminalsModel.mjs";
import { attentionRank } from "../../ui/sessionState.mjs";
import { projectActivity, workstreamActivity } from "./workstreamActivityModel.mjs";
import { projectPulse, pulseOf } from "./workstreamPulseModel.mjs";
import { portsOf } from "./portsModel.mjs";
import { claimedSessions, isDrawn, visibleSessionRows, workstreamSessionRows } from "./workstreamSessionsModel.mjs";
import { groupOrder, orderBy, projectOrder, workstreamOrder } from "./railOrderModel.mjs";
import { cardTitle } from "../_work/workstreamCardModel.mjs";
import { t as tr } from "../../i18n/l10n.mjs";

export const UNGROUPED = "";

/** The tabs, in strip order. */
export const RAIL_TABS = Object.freeze(["workspace", "goals", "workflows"]);
export const RAIL_TAB_LABEL = Object.freeze({
  workspace: tr("workbench-project-rail-workspace"),
  goals: tr("workbench-project-rail-goals"),
  workflows: tr("workbench-project-rail-workflows"),
});
/** Each tab's glyph, as the key of `ui/icons.ts` it is drawn from: the workspace's members mark, the goal's and the workflow's own. */
export const RAIL_TAB_ICON = Object.freeze({ workspace: "members", goals: "goal", workflows: "workflow" });
/**
 * The turn in which each tab yields its word in a rail too narrow — the
 * strip's order reversed, so Workflows folds to its glyph first and
 * Workspace keeps its word longest; one row at every width.
 */
export const RAIL_TAB_FOLD = Object.freeze({ workspace: 3, goals: 2, workflows: 1 });

/** The one rule: which tab a project belongs to, from where it was born. */
export function tabOf(project) {
  switch (project?.origin?.origin) {
    case "goal":
      return "goals";
    case "step":
      return "workflows";
    default:
      return "workspace";
  }
}

/** Where the rail's chosen tab and its archived switch are kept on this machine. */
export const RAIL_TAB_KEY = "bisa.ide.rail.tab";
export const RAIL_ARCHIVED_KEY = "bisa.ide.rail.archived";

/** The remembered tab, else the workspace tab for any other word. @param {unknown} raw */
export function railTabOf(raw) {
  return isRailTab(raw) ? raw : "workspace";
}

/**
 * The project the workbench is rooted in — by its workstream — for the
 * keyboard and the toolbar; none for a goal or a work item root.
 * @template {{project: {id: string}}} P
 * @param {{scope: string, id: string} | null | undefined} current
 * @param {readonly {workstream: {id: string, project: string}}[]} workstreams
 * @param {readonly P[]} projects
 * @returns {P | null}
 */
export function currentProjectOf(current, workstreams, projects) {
  if (!current || current.scope !== "workstream") return null;
  const ref = workstreams.find((w) => w.workstream.id === current.id);
  return ref ? (projects.find((r) => r.project.id === ref.workstream.project) ?? null) : null;
}

export function isRailTab(v) {
  return typeof v === "string" && RAIL_TABS.includes(v);
}

/** A section header for a thing that no longer exists: the id's tail, and the fact. */
function removedLabel(noun, id) {
  return tr("workbench-project-rail-removed", { noun, slice: String(id).slice(-6) });
}

/** `bisa.collapsed.<key>` — the same store `useCollapsed` reads. */
export function collapseKey(row) {
  switch (row.kind) {
    case "group":
      return `rail.group.${row.id}`;
    case "goal":
      return `rail.goal.${row.id}`;
    case "project":
      return `rail.project.${row.project.id}.${row.under}`;
    case "workstream":
      return `rail.workstream.${row.workstream.id}`;
    case "agent":
      // Only a harness with sub-agents folds; a sub-agent row has no key.
      return row.parent === null && (row.childCount ?? 0) > 0 ? `rail.agent.${row.id}` : null;
    default:
      return null;
  }
}

function byName(a, b) {
  return a.localeCompare(b, undefined, { sensitivity: "base" });
}

function matches(needle, ...hay) {
  if (!needle) return true;
  const n = needle.toLowerCase();
  return hay.some((h) => typeof h === "string" && h.toLowerCase().includes(n));
}

/**
 * The rows.
 *
 * @param {object} input
 * @param {"workspace" | "goals" | "workflows"} input.tab
 * @param {import("../../types").ProjectRow[]} input.projects
 * @param {import("../../types").WorkstreamRef[]} input.workstreams
 * @param {import("../../types").WorkstreamStatus[]} [input.statuses]
 * @param {import("../../types").SessionRow[]} [input.sessions]
 * @param {import("../../shell/terminalsModel.mjs").TerminalSessionState[]} [input.terminals]
 * @param {{id: string, label: string, live?: boolean}[]} [input.goals]
 * @param {{id: string, label: string}[]} [input.workflows] every workflow the caller knows, library and goal designs alike
 * @param {Set<string> | ((key: string) => boolean)} [input.collapsed]
 * @param {{scope: string, id: string} | null} [input.current]
 * @param {string} [input.filter]
 * @param {number} [input.now] unix **milliseconds** — the rail's clock, for the pulse's elapsed words
 */
export function railRows(input) {
  const {
    tab = "workspace",
    projects = [],
    workstreams = [],
    statuses = [],
    sessions = [],
    terminals = [],
    ports = [],
    goals = [],
    workflows = [],
    current = null,
    filter = "",
    now = 0,
    order = {},
  } = input ?? {};
  const isCollapsed = typeof input?.collapsed === "function" ? input.collapsed : (k) => !!input?.collapsed?.has?.(k);
  // The tabs that claim a roster row — the one rule for who is standing here.
  const claimed = claimedSessions(sessions, terminals);
  const statusOf = new Map(statuses.map((s) => [s.workstream, s]));
  const wsByProject = new Map();
  for (const ref of workstreams) {
    const w = ref.workstream;
    if (w.state?.state === "closed") continue;
    const list = wsByProject.get(w.project) ?? [];
    list.push(ref);
    wsByProject.set(w.project, list);
  }
  for (const [pid, list] of wsByProject) {
    list.sort((a, b) => {
      const pa = a.workstream.kind?.kind === "primary" ? 0 : 1;
      const pb = b.workstream.kind?.kind === "primary" ? 0 : 1;
      if (pa !== pb) return pa - pb;
      return (a.workstream.created_at ?? 0) - (b.workstream.created_at ?? 0) || a.workstream.id.localeCompare(b.workstream.id);
    });
    // The person's manual order leads; anything undragged keeps primary-first.
    wsByProject.set(pid, orderBy(list, workstreamOrder(order, pid), (r) => r.workstream.id));
  }
  const goalLabel = new Map(goals.map((g) => [g.id, g.label]));
  const workflowLabel = new Map(workflows.map((w) => [w.id, w.label]));

  /** The subtree of one project, or null when the filter removes it whole. */
  const projectRows = (row, under, depth) => {
    const p = row.project;
    const refs = wsByProject.get(p.id) ?? [];
    const activities = [];
    const pulses = [];
    const children = [];
    for (const ref of refs) {
      const w = ref.workstream;
      const status = statusOf.get(w.id) ?? null;
      // The title a workstream goes by everywhere — the card model's one rule.
      const label = cardTitle(w, status);
      const activity = workstreamActivity(sessions, terminals, w.id);
      activities.push(activity);
      const shells = terminals.filter((t) => t.scope === "workstream" && t.id === w.id);
      const agents = sessions.filter((s) => s.workstream === w.id && isDrawn(s, claimed));
      const wsPorts = portsOf(ports, w.id);
      // The one live line under the row, in seconds like the roster;
      // it carries the workstream's ports.
      const pulse = pulseOf({ sessions, terminals, workstream: w.id, now: Math.floor(now / 1000), ports: wsPorts });
      pulses.push(pulse);
      // Who is here, for the row's glyphs: the distinct harnesses of the
      // sessions standing in this workstream, first seen first; and one mark
      // per open shell, dim when it is not live.
      const harnesses = [];
      for (const s of agents) if (s.harness && !harnesses.includes(s.harness)) harnesses.push(s.harness);
      // A harness that reports is drawn once, by its session's glyph above.
      const shellGlyphs = shells
        .filter((t) => !(t.sessionId && claimed.has(t.sessionId)))
        .map((t) => ({ harness: harnessOf(t), live: t.liveness?.status === "live" }));
      // The one builder for who is standing here; a folded harness
      // hides its sub-agents through the shared rule, then the rail
      // decorates what remains with its own layout: the project, and the depth
      // — a terminal or a top agent one level under the workstream, a sub-agent
      // one deeper.
      const rawSessionRows = workstreamSessionRows(sessions, terminals, w.id);
      const sessionRows = visibleSessionRows(rawSessionRows, (sessionId) => isCollapsed(`rail.agent.${sessionId}`)).map((r) => ({
        ...r,
        project: p.id,
        depth: r.parent ? depth + 3 : depth + 2,
      }));
      // The filter searches the whole tree, folded sub-agents included, so a
      // match is never hidden by a collapse.
      const wsMatches = matches(filter, label, w.name, status?.branch) || rawSessionRows.some((r) => matches(filter, r.label, r.activity));
      const projectMatches = matches(filter, p.name, p.slug, ...(p.tags ?? []));
      if (filter && !wsMatches && !projectMatches) continue;
      const wrow = {
        kind: "workstream",
        id: w.id,
        project: p.id,
        workstream: w,
        status,
        primary: w.kind?.kind === "primary",
        label,
        exists: ref.exists,
        activity,
        harnesses,
        shellGlyphs,
        current: !!current && current.scope === "workstream" && current.id === w.id,
        depth: depth + 1,
        collapsed: isCollapsed(`rail.workstream.${w.id}`),
        // Whether the workstream has any sessions at all — its own caret — is
        // the whole tree's, not what an agent-fold leaves visible.
        sessions: rawSessionRows.length,
        pulse,
      };
      children.push(wrow);
      if (!wrow.collapsed) children.push(...sessionRows);
    }
    const projectMatches = matches(filter, p.name, p.slug, ...(p.tags ?? []));
    if (filter && children.length === 0 && !projectMatches) return null;
    const prow = {
      kind: "project",
      id: `${under}:${p.id}`,
      project: p,
      path: row.path,
      exists: row.exists,
      goals: row.goals ?? [],
      under,
      depth,
      collapsed: isCollapsed(`rail.project.${p.id}.${under}`),
      activity: projectActivity(activities),
      workstreams: refs.length,
      current: !!current && refs.some((r) => current.scope === "workstream" && current.id === r.workstream.id),
      // A folded project still says what is loudest inside it; an open one
      // shows its workstreams' own lines instead.
      pulse: null,
    };
    if (prow.collapsed) return [{ ...prow, pulse: projectPulse(pulses) }];
    return [prow, ...children];
  };

  // The tab's own projects, by name, then the person's manual order on top;
  // grouping preserves this order within each section.
  const byNameSorted = projects.filter((r) => tabOf(r.project) === tab).sort((a, b) => byName(a.project.name, b.project.name));
  const mine = orderBy(byNameSorted, projectOrder(order), (r) => r.project.id);
  const view = VIEWS[tab] ?? VIEWS.workspace;
  const built = view.sections(mine, { goalLabel, workflowLabel });
  // On the workspace tab the sections are the project groups, so a manual group
  // order reorders them; the ungrouped "Other projects" stays last (unlisted).
  const sections = tab === "workspace" ? orderBy(built, groupOrder(order), (s) => s.id) : built;

  // One emitter for every tab: a header row (unless the section says it has
  // none), then the members' subtrees — honouring collapse, dropping a
  // section the filter emptied.
  const out = [];
  for (const s of sections) {
    const depth = s.header ? 1 : 0;
    const members = s.rows.map((row) => projectRows(row, s.under, depth)).filter(Boolean);
    if (members.length === 0) continue;
    if (s.header) {
      const collapsed = isCollapsed(`rail.${s.kind}.${s.id}`);
      out.push({
        kind: s.kind,
        id: s.id,
        label: s.label,
        // What the heading folds — the namespace a drop reads (`railDragModel`).
        under: s.under,
        depth: 0,
        collapsed,
        count: members.length,
        projects: members.map((m) => m[0].project.id),
      });
      if (collapsed) continue;
    }
    for (const m of members) out.push(...m);
  }
  return out;
}

/**
 * How each tab folds its projects into sections — the Strategy behind
 * {@link railRows}. A section is `{kind, id, label, under, header, rows}`:
 * `kind` picks the header row's shape and collapse key (`rail.group.<id>` /
 * `rail.goal.<id>`), `under` disambiguates a project row's id, `header`
 * false means the members sit at the top with no heading at all.
 */
const VIEWS = Object.freeze({
  /** By `Project.group`, by name; the ungrouped last — and under no header when nobody made a group. */
  workspace: {
    sections(rows) {
      const groups = new Map();
      for (const row of rows) {
        const g = row.project.group?.trim() || UNGROUPED;
        const list = groups.get(g) ?? [];
        list.push(row);
        groups.set(g, list);
      }
      const names = [...groups.keys()].filter((g) => g !== UNGROUPED).sort(byName);
      const anyGroup = names.length > 0;
      return [...names, UNGROUPED]
        .filter((g) => groups.has(g))
        .map((g) => ({
          kind: "group",
          id: g,
          label: g === UNGROUPED ? tr("workbench-project-rail-other-projects") : g,
          under: g === UNGROUPED ? "ungrouped" : `group:${g}`,
          header: anyGroup,
          rows: groups.get(g),
        }));
    },
  },
  /** Under the goal that made each project; a goal that is gone still heads its section. */
  goals: {
    sections(rows, { goalLabel }) {
      return byKey(rows, (row) => row.project.origin.goal, (id) => goalLabel.get(id) ?? removedLabel("goal", id)).map((s) => ({
        kind: "goal",
        id: s.id,
        label: s.label,
        under: `goal:${s.id}`,
        header: true,
        rows: s.rows,
      }));
    },
  },
  /** Under the workflow whose step made each project; a workflow that is gone still heads its section. */
  workflows: {
    sections(rows, { workflowLabel }) {
      return byKey(rows, (row) => row.project.origin.workflow, (id) => workflowLabel.get(id) ?? removedLabel("workflow", id)).map((s) => ({
        kind: "group",
        id: `workflow:${s.id}`,
        label: s.label,
        under: `workflow:${s.id}`,
        header: true,
        rows: s.rows,
      }));
    },
  },
});

/** Rows folded by a key, as `{id, label, rows}` sections sorted by label. */
function byKey(rows, keyOf, labelOf) {
  const by = new Map();
  for (const row of rows) {
    const k = keyOf(row);
    const list = by.get(k) ?? [];
    list.push(row);
    by.set(k, list);
  }
  return [...by.keys()]
    .map((id) => ({ id, label: labelOf(id), rows: by.get(id) }))
    .sort((a, b) => byName(a.label, b.label) || String(a.id).localeCompare(String(b.id)));
}

/**
 * The tab strip's one badge: how many projects each tab holds. The
 * strip carries no other number — attention lives in the row's dot and its
 * wash, not in a count beside the tab.
 * @param {{projects: import("../../types").ProjectRow[]}} input
 * @returns {Record<"workspace" | "goals" | "workflows", number>}
 */
export function railCounts({ projects = [] }) {
  const counts = Object.fromEntries(RAIL_TABS.map((t) => [t, 0]));
  for (const row of projects) counts[tabOf(row.project)] += 1;
  return counts;
}

/**
 * Where an import from the rail lands, in one sentence for the dialog. The
 * tab never decides: a project with no goal is the workspace's and sits in
 * the *Workspace* tab whatever tab the button was pressed on; one made from
 * a goal heading's *Import into this goal…* is attached as it is made and
 * sits under that goal in *Goals*. An imported project is a person's, never
 * a step's, so it never sits in *Workflows* — that tab's sentence says why.
 * Nothing is guessed: the dialog has no goal picker, so a guess would be a
 * choice nobody made and nobody could change.
 * @param {"workspace" | "goals" | "workflows"} tab
 * @param {string | null} goalLabel the goal the door named, labelled — `null` for none
 * @returns {string}
 */
export function importLanding(tab, goalLabel) {
  if (goalLabel) return tr("workbench-project-rail-lands-under-goal-in-goals", { goal: goalLabel });
  if (tab === "workflows") return tr("workbench-project-rail-workflow-s-own-projects-made-steps");
  return tr("workbench-project-rail-lands-workspace-tab-attach-from-about");
}

/**
 * Where a project's row sits in its tab: the section that heads it
 * and the `under` its row id carries — from the same rules `railRows` folds
 * by, so a reveal and a listing cannot disagree.
 * @param {import("../../types").Project} project
 * @returns {{tab: "workspace" | "goals" | "workflows", kind: "group" | "goal", id: string, under: string}}
 */
export function sectionOf(project) {
  const tab = tabOf(project);
  if (tab === "goals") {
    const goal = project.origin.goal;
    return { tab, kind: "goal", id: goal, under: `goal:${goal}` };
  }
  if (tab === "workflows") {
    const wf = project.origin.workflow;
    return { tab, kind: "group", id: `workflow:${wf}`, under: `workflow:${wf}` };
  }
  const g = project.group?.trim() || UNGROUPED;
  return { tab, kind: "group", id: g, under: g === UNGROUPED ? "ungrouped" : `group:${g}` };
}

/**
 * What the rail must do to show a project's row: the tab to switch to and
 * the collapse keys that must be open — its section's header and its own
 * row (so the workstream under it shows too). A key that names no header
 * (the Workspace tab with no groups) is harmless to open.
 * @param {import("../../types").Project} project
 */
export function revealPlan(project) {
  const s = sectionOf(project);
  return { tab: s.tab, expand: [`rail.${s.kind}.${s.id}`, `rail.project.${project.id}.${s.under}`] };
}

/** The index of a project's row, or -1. */
export function rowIndexOfProject(rows, pid) {
  return rows.findIndex((r) => r.kind === "project" && r.project.id === pid);
}

/** The index of a workstream's row, or -1. */
export function rowIndexOfWorkstream(rows, wid) {
  return rows.findIndex((r) => r.kind === "workstream" && r.id === wid);
}

/** The distinct group names, for a "Move to group" menu. */
export function groupNames(projects) {
  return [...new Set((projects ?? []).map((r) => r.project.group?.trim()).filter(Boolean))].sort(byName);
}

/**
 * The rail's rows as the generic tree wants them (`ui/tree/treeListModel.mjs`):
 * an id unique across kinds — a project and its primary workstream share a
 * record id — a label for type-ahead, and whether the row opens. The rail
 * row rides along as `row`, so paint reads it unchanged.
 * @param {readonly object[]} rows
 * @param {(key: string) => boolean} isCollapsed
 */
export function treeRowsOf(rows, isCollapsed) {
  return rows.map((row) => {
    const key = collapseKey(row);
    const expandable =
      row.kind === "group" || row.kind === "goal" || row.kind === "project"
        ? true
        : row.kind === "workstream"
          ? row.sessions > 0
          : row.kind === "agent"
            ? row.parent === null && (row.childCount ?? 0) > 0
            : false;
    return {
      id: `${row.kind}:${row.id}`,
      depth: row.depth,
      label: row.kind === "project" ? row.project.name : row.label,
      expandable,
      expanded: expandable && key !== null ? !isCollapsed(key) : false,
      row,
    };
  });
}

// `attentionRank` is the one order the dot vocabulary knows; re-exported so a
// rail row can compare without a second import site.
export { attentionRank };

/**
 * A project by id from the whole workspace — the ones on screen and the ones
 * put away — so a door that names a project the rail drew never resolves to
 * nothing. `null` when neither list has it.
 * @param {readonly import("../../types").ProjectRow[]} projects
 * @param {readonly import("../../types").ProjectRow[]} archivedProjects
 * @param {string | null | undefined} pid
 * @returns {import("../../types").ProjectRow | null}
 */
export function projectOf(projects, archivedProjects, pid) {
  if (!pid) return null;
  return (projects ?? []).find((r) => r.project.id === pid) ?? (archivedProjects ?? []).find((r) => r.project.id === pid) ?? null;
}

/** Why a put-away project offers no new workstream — the row's, the menu's and the engine's one sentence. */
export const ARCHIVED_NO_WORKSTREAM = tr("workbench-project-rail-put-away-unarchive-before-opening-workstream");

/**
 * Whether a project's row and menu offer *New workstream*, and the reason
 * when they do not: a project that is put away takes no new work, said on
 * the row rather than discovered as the engine's refusal.
 * @param {Pick<import("../../types").Project, "name" | "archived"> | null | undefined} project
 * @returns {{ label: string, disabled: boolean, reason: string | null }}
 */
export function newWorkstreamOffer(project) {
  const name = project?.name ?? "";
  if (project?.archived) return { label: tr("workbench-project-rail-new-workstream-3", { name }), disabled: true, reason: ARCHIVED_NO_WORKSTREAM };
  return { label: tr("workbench-project-rail-new-workstream-3", { name }), disabled: false, reason: null };
}
