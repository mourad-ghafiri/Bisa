/**
 * The footer's terminals and harnesses, as the rail draws them: the two
 * rosters reconciled through the one rule every surface reads
 * (`claimedSessions` / `isDrawn`, `workstreamSessionsModel.mjs`) — a harness
 * a person opened in a terminal that has reported to the engine is **one**
 * row, the harness's, remembering its tab; a tab nothing claims is a
 * terminal row; a terminal session no tab claims is nowhere, and a
 * conversation's turn is its conversation's, never a harness row here.
 *
 * A terminal is **open** while its tab has not exited — live, or
 * unverifiable (contact lost; never evidence of death, ide/06). An exited
 * tab is listed, dim, and not counted.
 *
 * Every row says **where it stands**, compactly — *Bisa › feat/a*, the
 * project and the workstream — from one index over the workspace
 * ({@link placeIndex}); the workstream's word is the cards' rule
 * (`cardTitle`), so the footer, a card and the Workstreams panel name a
 * checkout alike.
 */

import { harnessOf, livenessWord } from "./terminalsModel.mjs";
import { isLive } from "../ui/sessionState.mjs";
import { cardTitle } from "../views/_work/workstreamCardModel.mjs";
import { claimedSessions, isDrawn } from "../views/_workbench/workstreamSessionsModel.mjs";
import { t as tr } from "../i18n/l10n.mjs";

/** Whether a tab still stands: anything but a reported exit. */
export function isOpen(liveness) {
  return !!liveness && liveness.status !== "exited";
}

const ORDER = Object.freeze({ live: 0, unverifiable: 1, exited: 2 });

/** The join between a project and its workstream in a place's words. */
const IN = " › ";

const tail = (id) => String(id ?? "").slice(-6);

/**
 * One lookup for where a session stands, from the workspace index: every
 * workstream's project (its name and its id) and word, every project's name
 * (by id, and by slug for what the disk is filed under), every goal's label
 * and the workflow its run follows.
 * @param {{workstreams?: readonly object[], projects?: readonly object[], goals?: readonly {id: string, label: string, workflow?: string | null, workflowName?: string | null}[]}} ws
 *   `workstreams` the `WorkstreamRef`s, `projects` the `ProjectRow`s, `goals` already labelled
 * @returns {{workstreams: Map<string, {project: string | null, projectId: string | null, workstream: string}>, projects: Map<string, string>, projectSlugs: Map<string, {id: string, name: string}>, goals: Map<string, {label: string, workflow: string | null, workflowName: string | null}>}}
 */
export function placeIndex(ws) {
  const projects = new Map((ws?.projects ?? []).map((r) => [r.project.id, r.project.name]));
  const projectSlugs = new Map((ws?.projects ?? []).map((r) => [r.project.slug, { id: r.project.id, name: r.project.name }]));
  const goals = new Map((ws?.goals ?? []).map((g) => [g.id, { label: g.label, workflow: g.workflow ?? null, workflowName: g.workflowName ?? null }]));
  const workstreams = new Map(
    (ws?.workstreams ?? []).map((r) => [
      r.workstream.id,
      { project: r.project_name ?? projects.get(r.workstream.project) ?? null, projectId: r.workstream.project ?? null, workstream: cardTitle(r.workstream, null) },
    ]),
  );
  return { workstreams, projects, projectSlugs, goals };
}

/** The empty index: every place falls back to its kind and tail. */
export function emptyPlaceIndex() {
  return { workstreams: new Map(), projects: new Map(), projectSlugs: new Map(), goals: new Map() };
}

/**
 * Where a session stands, in a few words: a workstream as its project and
 * itself — *Bisa › feat/a*, *Bisa › primary* — a project by name,
 * a goal by its label, a run of the workspace, a work item and an unknown id
 * by kind and tail, the machine as itself.
 * @param {string} scope @param {string} id
 * @param {ReturnType<typeof placeIndex>} index
 */
export function placeWords(scope, id, index) {
  switch (scope) {
    case "workstream": {
      const w = index.workstreams.get(id);
      if (!w) return tr("shell-footer-sessions-workstream-tail", { tail: tail(id) });
      return w.project ? `${w.project}${IN}${w.workstream}` : w.workstream;
    }
    case "project":
      return index.projects.get(id) ?? tr("shell-footer-sessions-project-tail", { tail: tail(id) });
    case "goal":
      return index.goals.get(id)?.label ?? tr("shell-footer-sessions-goal-tail", { tail: tail(id) });
    case "run":
      return tr("shell-footer-sessions-run-tail", { tail: tail(id) });
    case "work_item":
      return tr("shell-footer-sessions-work-item-tail", { tail: tail(id) });
    case "machine":
      return tr("shell-footer-sessions-machine");
    default:
      return `${scope} ·${tail(id)}`;
  }
}

/** Where a harness stands: its workstream's place, else its project's name, else nowhere. */
function harnessPlace(s, index) {
  if (s.workstream) return placeWords("workstream", s.workstream, index);
  if (s.project) return placeWords("project", s.project, index);
  return tr("shell-footer-sessions-checkout");
}

/**
 * @typedef {{key: string, scope: string, id: string, harness: string | null, liveness: object, open: boolean, word: string, place: string}} TerminalRow
 * @typedef {{id: string, agent: string | null, harness: string | null, workstream: string | null, state: unknown, terminalKey: string | null, place: string}} HarnessRow
 */

/**
 * The footer's two lists and its counts, every row with where it stands.
 * @param {readonly object[]} terminals the terminal store's sessions
 * @param {readonly object[]} sessions the roster (`SessionRow[]`)
 * @param {ReturnType<typeof placeIndex>} [index] the workspace's places
 * @returns {{terminals: TerminalRow[], harnesses: HarnessRow[], openTerminals: number}}
 */
export function footerSessions(terminals, sessions, index = emptyPlaceIndex()) {
  const claimed = claimedSessions(sessions ?? [], terminals ?? []);
  const claimedKeys = new Set(claimed.values());
  const terminalRows = (terminals ?? [])
    .filter((t) => !claimedKeys.has(t.key))
    .map((t) => ({ key: t.key, scope: t.scope, id: t.id, harness: harnessOf(t), liveness: t.liveness, open: isOpen(t.liveness), word: livenessWord(t.liveness), place: placeWords(t.scope, t.id, index) }))
    .sort((a, b) => (ORDER[a.liveness?.status] ?? 3) - (ORDER[b.liveness?.status] ?? 3));
  const harnesses = (sessions ?? [])
    .filter((s) => isDrawn(s, claimed) && isLive(s.state))
    .map((s) => ({ id: s.id, agent: s.agent ?? null, harness: s.harness ?? null, workstream: s.workstream ?? null, state: s.state, terminalKey: claimed.get(s.id) ?? null, place: harnessPlace(s, index) }));
  return { terminals: terminalRows, harnesses, openTerminals: terminalRows.filter((t) => t.open).length };
}
