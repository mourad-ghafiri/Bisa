/**
 * Attach the checkout's changes to the agent pane's tray — the one function
 * behind the three doors (ide/09): the *Attach* on the *files changed* line
 * under the composer, *The changes in this checkout* in the attach menu, and
 * *Attach to the agent* on a row in Git › Changes. Nothing calls it on its
 * own: the changes reach the agent only when a person asks.
 *
 * Reads the changed files, fetches the patch of each side a file has (an
 * untracked file has none), builds the chips (`gitContextModel.mjs`),
 * attaches what fits the budget and says what happened through the module
 * `toaster`, so a caller that is not a component can use it too.
 */

import { api } from "../../api";
import { toaster } from "../../ui";
import { standingOf } from "../_work/gitFiles.mjs";
import { contextFor, setContext } from "./agentPaneStore";
import { attachWithin, attachedWords, changeChips, type FilePatches } from "./gitContextModel.mjs";
import { showRightPanel } from "./rightPanelStore";

/**
 * Attach the changes of `paths` — every changed file when absent — as chips
 * on the thread `scope` (`workstream:<wid>`). Resolves once the tray is set.
 */
export async function attachGitChanges(wid: string, scope: string, paths?: readonly string[]): Promise<void> {
  try {
    const listing = await api.gitFiles(wid);
    const wanted = new Set(paths ?? []);
    const rows = listing.files.filter((f) => !paths || wanted.has(f.path));
    const patches = new Map<string, FilePatches>();
    await Promise.all(
      rows.map(async (row) => {
        const standing = standingOf(row);
        if (standing.untracked) return;
        const [staged, unstaged] = await Promise.all([
          standing.staged ? api.gitDiff(wid, row.path, true).then((d) => d.diff) : Promise.resolve(null),
          standing.unstaged || standing.conflicted ? api.gitDiff(wid, row.path, false).then((d) => d.diff) : Promise.resolve(null),
        ]);
        patches.set(row.path, { staged, unstaged });
      }),
    );
    const chips = changeChips(
      rows.map((r) => ({ path: r.path, untracked: standingOf(r).untracked })),
      patches,
    );
    const { tray, attached, leftOut } = attachWithin(contextFor(scope), chips);
    if (attached.length > 0) setContext(scope, tray);
    showRightPanel("agents", scope);
    const words = attachedWords(attached, leftOut);
    if (leftOut > 0) toaster.info(words);
    else toaster.ok(words);
  } catch (e) {
    toaster.error(e instanceof Error ? e.message : String(e));
  }
}
