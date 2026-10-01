/**
 * What a person selected in the project rail, as the Board reads it (ide/16):
 * a project group — its projects — or one project. A row's project is the
 * selection a click on it means: a heading gathers projects, a project row
 * is one, a workstream, a shell or an agent row stands in one. The Board
 * narrows to the selection and offers *All* as the door back. A selection
 * is how the rail stood, so it is kept across a restart — read back made
 * safe (`parseSelection`), and narrowed to what the workspace still lists
 * (`listedSelection`): a project that is gone selects nothing.
 */

import { t } from "../../i18n/l10n.mjs";
import { idValue, textValue, wordsValue } from "../../shell/viewValuesModel.mjs";

/**
 * The selection a rail row means, or null for a row that means none.
 * @param {object | null | undefined} row a `RailRow`
 * @returns {{kind: "group", label: string, projects: string[]} | {kind: "project", id: string, label: string} | null}
 */
export function selectionOf(row) {
  if (!row) return null;
  switch (row.kind) {
    case "group":
    case "goal":
      return { kind: "group", label: row.label, projects: [...new Set(row.projects ?? [])] };
    case "project":
      return { kind: "project", id: row.project.id, label: row.project.name };
    case "workstream":
    case "terminal":
    case "agent":
      return typeof row.project === "string" && row.project ? { kind: "project", id: row.project, label: "" } : null;
    default:
      return null;
  }
}

/**
 * A selection read back from a memory that outlives the window: a project
 * with an id, or a group with a label and projects — else nothing. A memory
 * written by hand or by another version is no selection.
 * @param {unknown} raw
 * @returns {ReturnType<typeof selectionOf>}
 */
export function parseSelection(raw) {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return null;
  if (raw.kind === "project") {
    const id = idValue(raw.id);
    return id ? { kind: "project", id, label: textValue(raw.label) ?? "" } : null;
  }
  if (raw.kind === "group") {
    const label = textValue(raw.label);
    const projects = wordsValue(raw.projects) ?? [];
    return label && projects.length > 0 ? { kind: "group", label, projects } : null;
  }
  return null;
}

/**
 * A selection narrowed to what the workspace lists: a project that is gone
 * selects nothing, a group keeps the projects that are still there and
 * selects nothing once none is. The same selection, by identity, when every
 * id is listed.
 * @param {ReturnType<typeof selectionOf>} selection
 * @param {Iterable<string>} listed the ids of the projects the workspace lists
 * @returns {ReturnType<typeof selectionOf>}
 */
export function listedSelection(selection, listed) {
  if (!selection) return null;
  const known = listed instanceof Set ? listed : new Set(listed);
  if (selection.kind === "project") return known.has(selection.id) ? selection : null;
  const projects = selection.projects.filter((p) => known.has(p));
  if (projects.length === selection.projects.length) return selection;
  return projects.length > 0 ? { ...selection, projects } : null;
}

/** Two selections that mean the same cards. */
export function sameSelection(a, b) {
  if (a === b) return true;
  if (!a || !b || a.kind !== b.kind) return false;
  if (a.kind === "project") return a.id === b.id;
  return a.label === b.label && a.projects.length === b.projects.length && a.projects.every((p, i) => p === b.projects[i]);
}

/**
 * The Board's scope for a selection: the projects it narrows to (null for
 * all) and the words the bar shows — *All workstreams*, *Bisa*,
 * *Shop · 3 projects*. A project selected from a workstream's row carries no
 * name of its own; `nameOf` supplies it from the workspace.
 * @param {ReturnType<typeof selectionOf>} selection
 * @param {(id: string) => string | null | undefined} [nameOf]
 * @returns {{projects: Set<string> | null, words: string, all: boolean}}
 */
export function boardScope(selection, nameOf = () => null) {
  if (!selection) return { projects: null, words: t("workbench-rail-selection-all-workstreams"), all: true };
  if (selection.kind === "project") {
    const name = selection.label || nameOf(selection.id) || t("workbench-rail-selection-project-tail", { tail: selection.id.slice(-6) });
    return { projects: new Set([selection.id]), words: name, all: false };
  }
  const n = selection.projects.length;
  return { projects: new Set(selection.projects), words: t("workbench-rail-selection-project-projects", { selection: selection.label, n }), all: false };
}
