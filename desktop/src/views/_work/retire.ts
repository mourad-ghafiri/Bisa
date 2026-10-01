/**
 * The one door every retirement of a goal or a workflow goes through — the
 * `closeWorkstream.ts` shape. In order: **count** what the plan terminates
 * from this app's stores (`retireModel.terminationOf` — the consent line the
 * dialog showed was read from the same counts); **retire** through the
 * node, which refuses first, stops every session, cancels the run — the
 * goal's, or every run of the workspace the workflow has going — waits for
 * the rows to end, settles the projects, then archives or deletes; then
 * the after-effects, each on its own — **close the tabs** rooted at the
 * retired workstreams and at the goal (`closeTerminalsRootedAt`), **forget**
 * their workbench roots and what was kept of how each stood, and **refresh**
 * the workspace. Terminals are the
 * desktop's alone: the node never knows a PTY.
 *
 * Nothing after the node's answer can read as "the delete failed": the
 * thing is gone or put away by then, and an after-effect that throws is a
 * line in the log, never a toast that lies and never a dialog left open on
 * a goal that no longer exists.
 */

import { api } from "../../api";
import { errorFields, log } from "../../log";
import { sessionRows } from "../../shell/sessionsStore";
import { closeTerminalsRootedAt, terminalSessions } from "../../shell/useTerminals";
import { refreshWorkspace } from "../../shell/useWorkspaceData";
import type { Retired } from "../../types";
import { forgetRootMemory, forgetWorkbenchRoot } from "../_workbench/workbenchStore";
import { rootKey } from "../_workbench/workbenchModel.mjs";
import type { TerminationCounts } from "./closeWorkstreamModel.mjs";
import { planOf, terminationOf } from "./retireModel.mjs";
import type { RetireChoices, RetireKind, RetirePreview } from "./retireModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export interface Retirement {
  /** What the node did. */
  retired: Retired;
  /** What this app terminated with it — counted before the call, so it is what the dialog promised. */
  terminated: TerminationCounts;
}

/** One after-effect, on its own: a throw is logged and the next one runs. */
function after(what: string, run: () => void): void {
  try {
    run();
  } catch (e) {
    log.warn("retire", `an after-effect of a retirement failed: ${what}`, errorFields(e));
  }
}

/** Retire a goal or a workflow on the person's choices, ending what stands on it. */
export async function retire(kind: RetireKind, id: string, choices: RetireChoices, preview: RetirePreview): Promise<Retirement> {
  const goal = kind === "goal" ? id : null;
  const terminated = terminationOf(preview, sessionRows(), terminalSessions(), choices, goal);
  const answer = kind === "goal" ? await api.retireGoal(id, planOf("goal", choices)) : await api.retireWorkflow(id, planOf("workflow", choices));
  const retired = answer.retired;
  after(t("work-retire-closing-terminals"), () => closeTerminalsRootedAt(retired.workstreams_retired, goal));
  after(t("work-retire-forgetting-workbench-roots"), () => {
    for (const wid of retired.workstreams_retired) {
      forgetWorkbenchRoot(rootKey("workstream", wid));
      forgetRootMemory(rootKey("workstream", wid));
    }
  });
  after(t("work-retire-refreshing-workspace"), () => refreshWorkspace());
  return { retired, terminated };
}
