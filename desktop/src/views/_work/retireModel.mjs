/**
 * Retiring a goal or a workflow, as the dialog decides it: the sections a
 * preview becomes — each a fact with its choice — the words on the one
 * button, what the plan is, what is not available, and what the plan
 * terminates counted from this app's own stores. `RetireDialog.tsx` draws;
 * `retire.ts` is the door; the node answers the preview and carries the
 * plan out.
 *
 * The rules that never bend: a refusal is said before anything stops; a
 * merely attached project is detached, never deleted; an adopted folder
 * never moves; a workflow something uses cannot be deleted, only archived;
 * a goal whose design something else uses cannot be deleted either.
 *
 * The vocabulary is the workstream close's (`closeWorkstreamModel.mjs`): a
 * harness is **terminated**, a shell **closed**, an engine session
 * **aborted** — and the run is **cancelled**: a goal's one run, or every run
 * of the workspace a workflow has going (retired before the workflow goes).
 */

import { isLive as sessionIsLive } from "../../ui/sessionState.mjs";
import { terminationWords } from "./closeWorkstreamModel.mjs";
import { t as tr } from "../../i18n/l10n.mjs";

/**
 * @typedef {{ id: string, slug: string, name: string, adopted: boolean, workstreams: number, workstream_ids: string[], sessions: number, archived: boolean }} ProjectFacts
 * @typedef {{ id: string, status: string, live_steps: number }} RunFacts
 * @typedef {{ kind: string, id: string, label: string, live: boolean }} Holder
 * @typedef {{ agents: number, harnesses: number, run: RunFacts | null, runs: RunFacts[], history: number, refusal: string | null, designs: number, used_by: Holder[], projects_born: ProjectFacts[], projects_attached: ProjectFacts[] }} Preview
 * @typedef {{ thing: "archive" | "delete", projects: "keep" | "archive" | "delete", tree: boolean }} Choices
 * @typedef {import("./closeWorkstreamModel.mjs").TerminationCounts} TerminationCounts
 */

/**
 * Whether the thing itself may be deleted: never a workflow something uses,
 * never a goal whose design something else uses — the node's refusal, read
 * before anything stops.
 */
export function deleteAvailable(preview) {
  return (preview?.used_by?.length ?? 0) === 0 && !preview?.refusal;
}

/**
 * The workstreams the plan's fate touches — the born projects' when they
 * are archived or deleted; none when they are kept.
 * @param {Preview} preview @param {Choices} choices
 * @returns {Set<string>}
 */
export function touchedWorkstreams(preview, choices) {
  if (choices.projects === "keep") return new Set();
  return new Set(preview.projects_born.flatMap((p) => p.workstream_ids ?? []));
}

/**
 * What the plan terminates, counted once: the engine sessions on the thing
 * (the preview's `agents`) and those in the touched workstreams that are
 * not on it (this app's roster); the harnesses and shells in this app's
 * live tabs rooted at the goal and at the touched workstreams. A harness the
 * node counts on the goal that has no tab here is another desktop's to
 * terminate — it is counted, never twice. A workflow has no goal scope of
 * its own, so the preview's agents and the roster's rows in the touched
 * workstreams describe the same sessions: the larger count, not the sum.
 * @param {Preview} preview
 * @param {readonly import("../../types").SessionRow[] | null | undefined} sessions the roster
 * @param {readonly import("../../shell/terminalsModel.mjs").TerminalSessionState[] | null | undefined} terminals this app's tabs
 * @param {Choices} choices
 * @param {string | null} goal the goal's id, `null` for a workflow
 * @returns {TerminationCounts}
 */
export function terminationOf(preview, sessions, terminals, choices, goal) {
  const touched = touchedWorkstreams(preview, choices);
  const rooted = (terminals ?? []).filter(
    (t) => t.liveness?.status === "live" && ((goal !== null && t.scope === "goal" && t.id === goal) || (t.scope === "workstream" && touched.has(t.id))),
  );
  const harnessTabs = rooted.filter((t) => t.harness !== null && t.harness !== undefined).length;
  const shells = rooted.length - harnessTabs;
  const elsewhere = (sessions ?? []).filter(
    (s) => s.kind !== "terminal" && sessionIsLive(s.state) && (goal === null || s.goal !== goal) && s.workstream != null && touched.has(s.workstream),
  ).length;
  const agents = goal === null ? Math.max(preview.agents, elsewhere) : preview.agents + elsewhere;
  return { harnesses: Math.max(preview.harnesses, harnessTabs), shells, agents };
}

/**
 * What becomes of the run — *Its run is cancelled: 2 steps are live.* —
 * or `null` when none is going.
 * @param {Preview} preview
 */
export function runLine(preview) {
  const run = preview.run;
  if (!run) return null;
  if (run.live_steps === 0) return tr("work-retire-run-cancelled-before-another-step-starts");
  return tr("work-retire-run-cancelled-live", { live_steps: run.live_steps });
}

/**
 * What becomes of a workflow's runs of the workspace that are going —
 * *Its 2 runs in the workspace are cancelled: 3 steps are live.* — or
 * `null` when none is going. Retiring the workflow retires them first,
 * whatever its fate.
 * @param {Preview} preview
 */
export function runsLine(preview) {
  const runs = preview.runs ?? [];
  if (runs.length === 0) return null;
  const live_steps = runs.reduce((n, r) => n + (r.live_steps ?? 0), 0);
  return tr("work-retire-runs-cancelled", { n: runs.length, live_steps });
}

/**
 * What becomes of a workflow's runs of the workspace, finished or not —
 * kept as its history under an archived workflow, gone with a deleted one
 * — or `null` when it has none.
 * @param {Preview} preview @param {Choices} choices
 */
export function historyLine(preview, choices) {
  const n = preview.history ?? 0;
  if (n === 0) return null;
  return choices.thing === "delete" ? tr("work-retire-runs-go-with-it", { n }) : tr("work-retire-runs-stay-its-history", { n });
}

/**
 * The sentence for what stops — *2 agent sessions aborted, 1 harness
 * terminated and 1 shell closed* — or `null` when nothing stands there.
 * @param {TerminationCounts} counts
 */
export function stopsLine(counts) {
  const words = terminationWords(counts);
  return words ? `${words[0].toUpperCase()}${words.slice(1)}.` : null;
}

/**
 * The choices the dialog opens with: the fate the door named, kept to what
 * is available; the projects kept; no Trash.
 * @param {"archive" | "delete"} wanted
 * @param {Preview | null | undefined} preview
 * @returns {Choices}
 */
export function defaultChoices(wanted, preview) {
  const thing = wanted === "delete" && deleteAvailable(preview) ? "delete" : "archive";
  return { thing, projects: "keep", tree: false };
}

/**
 * The sections: one per fact, in the order a person weighs them — the
 * refusal first, the run (a goal's, or a workflow's runs of the
 * workspace), what stops, then what still uses it, its runs' history, its
 * designs, the projects.
 * @param {"goal" | "workflow"} kind
 * @param {Preview} preview
 * @param {Choices} choices
 * @param {TerminationCounts} terminated what the plan ends, from `terminationOf`
 * @returns {{ id: string, title: string, lines: string[], tone: "quiet" | "warn" | "danger", holders?: Holder[] }[]}
 */
export function retireSections(kind, preview, choices, terminated) {
  const out = [];
  const deleting = choices.thing === "delete";
  if (preview.refusal) {
    out.push({ id: "refused", title: tr("work-retire-delete-refused"), lines: [tr("work-retire-refusal-can-be-archived", { refusal: preview.refusal })], tone: "warn" });
  }
  const runs = [runLine(preview), runsLine(preview)].filter((line) => line !== null);
  if (runs.length > 0) {
    out.push({ id: "run", title: tr("work-retire-what-runs"), lines: runs, tone: "warn" });
  }
  const stops = stopsLine(terminated);
  if (stops) {
    out.push({ id: "stops", title: tr("work-retire-what-stops"), lines: [stops], tone: "warn" });
  }
  if (kind === "workflow" && preview.used_by.length > 0) {
    out.push({
      id: "used",
      title: tr("work-retire-still-use"),
      lines: [tr("work-retire-used-by-archive-not-delete", { holders: preview.used_by.map((r) => tr("work-retire-holder", { kind: r.kind, label: r.label, live: r.live ? "yes" : "no" })).join(", ") })],
      tone: "warn",
      holders: preview.used_by.map((r) => ({ kind: r.kind, id: r.id, label: r.label, live: r.live })),
    });
  }
  const history = kind === "workflow" ? historyLine(preview, choices) : null;
  if (history) {
    out.push({ id: "history", title: tr("work-retire-runs-history"), lines: [history], tone: deleting ? "danger" : "quiet" });
  }
  if (kind === "goal" && preview.designs > 0) {
    out.push({
      id: "designs",
      title: tr("work-retire-designs"),
      lines: [deleting ? tr("work-retire-drawn-go", { designs: preview.designs }) : tr("work-retire-drawn-stay-under", { designs: preview.designs })],
      tone: deleting ? "danger" : "quiet",
    });
  }
  if (preview.projects_born.length > 0) {
    const born = preview.projects_born;
    const adopted = born.filter((p) => p.adopted);
    const lines = born.map((p) => tr("work-retire-project-born-line", { name: p.name, workstreams: p.workstreams, sessions: p.sessions, adopted: p.adopted ? "yes" : "no", archived: p.archived ? "yes" : "no" }));
    if (choices.projects === "delete") {
      lines.push(choices.tree ? tr("work-retire-their-records-go-each-managed-folder") : tr("work-retire-their-records-go-every-folder-stays"));
      if (choices.tree && adopted.length > 0) lines.push(tr("work-retire-adopted-folder-never-moved", { names: adopted.map((p) => p.name).join(", ") }));
    } else if (choices.projects === "archive") {
      lines.push(tr("work-retire-they-put-away-hidden-from-rail"));
    } else {
      lines.push(deleting && kind === "goal" ? tr("work-retire-they-stay-workspace-detached") : tr("work-retire-they-stay-they"));
    }
    out.push({ id: "born", title: kind === "goal" ? tr("work-retire-projects-made") : tr("work-retire-projects-steps-made"), lines, tone: choices.projects === "delete" ? "danger" : "quiet" });
  }
  if (kind === "goal" && preview.projects_attached.length > 0) {
    const names = preview.projects_attached.map((p) => p.name).join(", ");
    out.push({
      id: "attached",
      title: tr("work-retire-attached-projects"),
      lines: [tr("work-retire-attached-not-deleted", { n: preview.projects_attached.length, names, deleting: deleting ? "yes" : "no" })],
      tone: "quiet",
    });
  }
  return out;
}

/**
 * The one button's words, reading the plan back: *Archive goal and 2
 * projects*, *Delete workflow, keep projects*.
 * @param {"goal" | "workflow"} kind
 * @param {Choices} choices
 * @param {Preview} preview
 */
export function confirmWords(kind, choices, preview) {
  const verb = choices.thing === "delete" ? tr("work-retire-verb-delete") : tr("work-retire-verb-archive");
  const born = preview.projects_born.length;
  if (born === 0) return tr("work-retire-verb-kind", { verb, kind });
  const projects = tr("work-retire-projects-count", { n: born });
  if (choices.projects === "keep") return tr("work-retire-keep", { verb, kind, projects });
  const same = choices.projects === choices.thing;
  return same ? tr("work-retire-words", { verb, kind, projects }) : tr("work-retire-words-differ", { verb, kind, fate: choices.projects, projects });
}

/**
 * The dialog's title: the thing by name, under the fate chosen — the door's
 * at first, the person's once they changed their mind inside.
 * @param {"archive" | "delete"} thing
 * @param {string} name
 */
export function titleWords(thing, name) {
  return thing === "delete" ? tr("work-retire-dialog-title-delete", { name }) : tr("work-retire-dialog-title-archive", { name });
}

/**
 * What is said once it happened: the thing, its fate, and what this app
 * terminated with it — the one sentence the goal's page, the designer and
 * the library's card all say.
 * @param {"goal" | "workflow"} kind
 * @param {"archive" | "delete"} thing the fate it met
 * @param {TerminationCounts | null | undefined} terminated
 */
export function retiredWords(kind, thing, terminated) {
  const ended = terminated ? terminationWords(terminated) : null;
  if (kind === "goal") {
    if (thing === "delete") return ended ? tr("screens-goal-detail-goal-deleted-2", { ended }) : tr("screens-goal-detail-goal-deleted-3");
    return ended ? tr("screens-goal-detail-goal-archived", { ended }) : tr("screens-goal-detail-goal-archived-2");
  }
  if (thing === "delete") return ended ? tr("screens-workflow-designer-workflow-deleted-2", { ended }) : tr("screens-workflow-designer-workflow-deleted-3");
  return ended ? tr("screens-workflow-designer-workflow-archived", { ended }) : tr("screens-workflow-designer-workflow-archived-2");
}

/** Whether the plan deletes anything — the button's danger tone. */
export function destroys(choices) {
  return choices.thing === "delete" || choices.projects === "delete";
}

/** The plan as the node takes it. */
export function planOf(kind, choices) {
  return kind === "goal" ? { goal: choices.thing, projects: choices.projects, tree: choices.projects === "delete" && choices.tree } : { workflow: choices.thing, projects: choices.projects, tree: choices.projects === "delete" && choices.tree };
}
