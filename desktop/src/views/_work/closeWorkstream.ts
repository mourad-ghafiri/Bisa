/**
 * The one door every close of a workstream goes through (ide/07): the
 * Workstreams panel's *Close* and *Delete the checkout*, the Clean up step
 * after a merge, the project rail's and the Board's *Close*.
 *
 * In order: **count** what stands in the checkout from this app's stores —
 * the consent line the dialog showed was read from the same counts; **close**
 * the record through the node, which stops every engine session and every
 * harness's roster row first and says how many (`stopped_sessions`); then
 * **close the tabs** rooted at the workstream (`closeTerminalsRootedAt`) —
 * a harness is terminated by closing its tab, a shell is closed; and
 * **forget** the workbench root's document tabs, with what was kept of how
 * the root stood (`forgetRootMemory`). Terminals are the desktop's
 * alone: the node never knows a PTY, so this half cannot live server-side.
 *
 * The tabs close **without the terminal guard asking again**: the close
 * dialog was the one consent, and a second "Close this terminal?" per tab
 * would be the same question asked N+1 times. A harness tab may already be
 * gone by the time the response lands — the roster's `aborted` frame closes
 * it (`watchAborts`) — and closing a gone key is a no-op, so the order does
 * not matter. When the node refuses (a clean script that failed), nothing
 * here runs: the tree is still there, and its shells still stand in it.
 */

import { api } from "../../api";
import { sessionRows } from "../../shell/sessionsStore";
import { closeBrowsersRootedAt } from "../../shell/useBrowsers";
import { closeTerminalsRootedAt, terminalSessions } from "../../shell/useTerminals";
import { forgetRootMemory, forgetWorkbenchRoot } from "../_workbench/workbenchStore";
import { rootKey } from "../_workbench/workbenchModel.mjs";
import { terminationCounts } from "./closeWorkstreamModel.mjs";
import type { TerminationCounts } from "./closeWorkstreamModel.mjs";

export type Closed = Awaited<ReturnType<typeof api.closeWorkstream>> & {
  /** What this app terminated with the close — counted before the call, so it is what the dialog promised. */
  terminated: TerminationCounts;
};

/** Close a workstream's record — and, with `tree`, its checkout — ending what stands in it. */
export async function closeWorkstream(wid: string, opts: { tree: boolean }): Promise<Closed> {
  const terminated = terminationCounts(sessionRows(), terminalSessions(), wid);
  const response = await api.closeWorkstream(wid, opts.tree ? { tree: true } : undefined);
  closeTerminalsRootedAt([wid]);
  closeBrowsersRootedAt([wid]);
  forgetWorkbenchRoot(rootKey("workstream", wid));
  forgetRootMemory(rootKey("workstream", wid));
  return { ...response, terminated };
}
