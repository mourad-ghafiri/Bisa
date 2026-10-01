import type { ProjectRow, WorkstreamRef } from "../../types";

export interface Home {
  scope: "workstream";
  id: string;
}

/** Where `#/projects` lands: the remembered root while it is an open workstream of a listed project, else the first project's primary, else nowhere. */
export declare function homeRoot(
  lastRoot: string | null | undefined,
  projects: readonly ProjectRow[] | null | undefined,
  workstreams: readonly WorkstreamRef[] | null | undefined,
): Home | null;

/** The home once `pid` is archived or removed, from the facts in hand with the project taken out. */
export declare function homeAfterLeaving(
  pid: string,
  lastRoot: string | null | undefined,
  projects: readonly ProjectRow[] | null | undefined,
  workstreams: readonly WorkstreamRef[] | null | undefined,
): Home | null;

/** The project a `project_deleted` or `project_archived {archived: true}` fact takes from under the root, or null. */
export declare function leavesRoot(
  payload: { type: string; project?: string | null; archived?: boolean },
  root: { scope: string; id: string } | null | undefined,
  workstreams: readonly WorkstreamRef[] | null | undefined,
): string | null;

/** The route a home is: the workbench on it, or `#/projects` when there is none. */
export declare function homeRoute(home: Home | null): { name: "workbench"; scope: "workstream"; id: string } | { name: "projects" };
