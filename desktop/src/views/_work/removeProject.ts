/**
 * The one door every removal of a project goes through — the rail's menu
 * and the project's own page — in the `closeWorkstream.ts` shape. In order:
 * **name** the roots the project stands on from the workspace's list
 * (`removeProjectModel.projectRoots`) and **the home the IDE leaves for**
 * once the project is gone (`ideHomeModel.homeAfterLeaving`, from the facts
 * in hand — the lists are not read again before the person is moved);
 * **ask the node**, which stops every session in the project first and then
 * archives it, forgets it, or forgets it and takes its folder; then the
 * after-effects, each on its own: **forget the remembered root** when it
 * stood on the project — a home is never a root that was put away or
 * removed — and, for an act that takes the roots away (`rootsGo` — forget and
 * delete, never archive), **close the tabs** rooted at them — a shell left
 * standing in a folder that went to the Trash is a tab nobody can reach from
 * the rail — the browser tabs at home there, and **forget** each root's
 * documents and what was kept of how it stood; then **refresh** the
 * workspace.
 *
 * Nothing after the node's answer can read as "the removal failed": the
 * project is gone or put away by then, so an after-effect that throws is a
 * line in the log. When the node refuses, nothing here runs and nobody is
 * moved: the caller moves to `home` only on the answer, and only when the
 * person stands on what was removed (`standsOn`).
 */

import { api } from "../../api";
import { errorFields, log } from "../../log";
import { closeBrowsersRootedAt } from "../../shell/useBrowsers";
import { closeTerminalsRootedAt } from "../../shell/useTerminals";
import { refreshWorkspace } from "../../shell/useWorkspaceData";
import type { ProjectRow, WorkstreamRef } from "../../types";
import { homeAfterLeaving } from "../_workbench/ideHomeModel.mjs";
import type { Home } from "../_workbench/ideHomeModel.mjs";
import { forgetLastRoot, lastRoot } from "../_workbench/lastRootStore";
import { forgetRootMemory, forgetWorkbenchRoot } from "../_workbench/workbenchStore";
import { rootKey } from "../_workbench/workbenchModel.mjs";
import { projectRoots, removedSaid, rootsGo, standsOn } from "./removeProjectModel.mjs";
import type { RemoveAct } from "./removeProjectModel.mjs";

export interface RemovedProject {
  /** What is said once it is done — the node's own word on a folder that stayed. */
  said: ReturnType<typeof removedSaid>;
  /** The roots the project stood on, whether or not they went. */
  roots: string[];
  /** Where the IDE goes when the person stood on one of them: the next home, or none — the landing. */
  home: Home | null;
}

/** One after-effect, on its own: a throw is logged and the next one runs. */
function after(what: string, run: () => void): void {
  try {
    run();
  } catch (e) {
    log.warn("projects", "an after-effect of removing a project failed", { what, ...errorFields(e) });
  }
}

/**
 * Archive a project, forget it, or forget it and take its folder — ending what this app holds on its roots when they go.
 * @param workstreams the workspace's list, every project's checkouts
 * @param projects the workspace's live list — what the next home is chosen among
 */
export async function removeProject(
  pid: string,
  act: RemoveAct,
  trash: boolean,
  workstreams: readonly WorkstreamRef[],
  projects: readonly ProjectRow[],
): Promise<RemovedProject> {
  const roots = projectRoots(workstreams, pid);
  const home = homeAfterLeaving(pid, lastRoot(), projects, workstreams);
  const answer = act === "archive" ? await api.archiveProject(pid, true) : await api.deleteProject(pid, act === "delete" ? { tree: true } : undefined);
  after("remembered", () => {
    const remembered = lastRoot();
    if (remembered !== null && standsOn({ scope: "workstream", id: remembered }, roots)) forgetLastRoot();
  });
  if (rootsGo(act)) {
    after("terminals", () => closeTerminalsRootedAt(roots));
    after("browsers", () => closeBrowsersRootedAt(roots));
    after("roots", () => {
      for (const wid of roots) {
        forgetWorkbenchRoot(rootKey("workstream", wid));
        forgetRootMemory(rootKey("workstream", wid));
      }
    });
  }
  after("workspace", () => refreshWorkspace());
  return { said: removedSaid(act, trash, act === "delete" ? (answer as { removed_tree?: boolean; kept?: string | null; path?: string }) : null), roots, home };
}
