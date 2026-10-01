/**
 * The one switch over a checkout's Git views — Changes · Branches · History ·
 * Stashes (`GIT_VIEWS`) — so the right panel's Git tab has one rendering per
 * view and nothing can drift between doors. Every view is what a person does
 * to the tree; the repository's facts and the project's settings are About's
 * Checkout and Settings views, which `onOpenAbout` opens. Above whichever
 * view shows, the **Resolve card** whenever git has left a merge, a
 * rebase, a cherry-pick or a revert half-done (ide/04 §Conflicts, continued)
 * — the one door to continue, skip or abort it.
 */
import { useState } from "react";
import type { GitStatusInfo } from "../../types";
import type { GitView } from "../_workbench/rightPanelModel.mjs";
import { openPanelView } from "../_workbench/rightPanelStore";
import { rootKey } from "../_workbench/workbenchModel.mjs";
import { BranchesPanel } from "./BranchesPanel";
import { CommitGraph } from "./CommitGraph";
import { GitPanel } from "./GitPanel";
import { readsFor } from "./gitChangeModel.mjs";
import { patchSession } from "./gitPanelStore";
import { useGitChanges } from "./useGitChanges";
import { ResolveCard } from "./ResolveCard";
import { StashesPanel } from "./StashesPanel";

export function GitViewBody({
  pid,
  wid,
  goal,
  view,
  status,
  defaultBranch,
  onChanged,
  onOpenAbout,
  onOpenFile,
  onOpenPatch,
  onOpenCommit,
  rootPath = null,
}: {
  pid: string;
  wid: string;
  goal: string | null;
  view: GitView;
  status: GitStatusInfo;
  defaultBranch: string | null;
  onChanged: () => void;
  /** Open About › Settings — the remote, who commits here, the publishing policy. */
  /** Open About on the view that holds the thing pointed at — the checkout's connection and config, or the project's settings. */
  onOpenAbout: (view: "checkout" | "settings") => void;
  /** Open a changed file as a document — a row's menu; absent where no editor is near. */
  onOpenFile?: (path: string) => void;
  /** Show a file's patch on one side in the centre — what selecting a row in Changes does. */
  onOpenPatch: (path: string, staged: boolean) => void;
  /** Show a commit in the centre — what a row's click in History does. */
  onOpenCommit: (sha: string) => void;
  /** The checkout's absolute path, for *Copy absolute path*. */
  rootPath?: string | null;
}) {
  // A consented operation from the graph (cherry-pick, revert, checkout, a
  // new branch or tag) changes what the graph shows: bump to relayout.
  const [graphKey, setGraphKey] = useState(0);
  // A commit or a ref moved under the checkout: the graph relayouts — only
  // while History is showing, never on a worktree edit (ide/04 §Live).
  useGitChanges(wid, (kinds) => {
    if (view === "history" && readsFor(kinds).history) setGraphKey((k) => k + 1);
  });
  const card = status.in_progress ? (
    <div className="mt-2">
      <ResolveCard
        wid={wid}
        status={status}
        onOpenPath={(path) => {
          patchSession(rootKey("workstream", wid), { selection: { path, staged: false } });
          openPanelView("git", "changes");
          onOpenPatch(path, false);
        }}
      />
    </div>
  ) : null;
  switch (view) {
    case "changes":
      return (
        <>
          {card}
          <GitPanel pid={pid} wid={wid} goal={goal} status={status} defaultBranch={defaultBranch} onChanged={onChanged} onOpenAbout={onOpenAbout} onOpenFile={onOpenFile} onOpenPatch={onOpenPatch} rootPath={rootPath} />
        </>
      );
    case "branches":
      return (
        <>
          {card}
          <BranchesPanel pid={pid} wid={wid} status={status} defaultBranch={defaultBranch} onChanged={onChanged} />
        </>
      );
    case "history":
      // The graph fills the column: the box passes the tab's height down, so
      // the list is as tall as the panel and scrolls inside it (ide/05).
      return (
        <div className="flex min-h-0 flex-1 flex-col">
          {card}
          <CommitGraph
            wid={wid}
            refreshKey={graphKey}
            inProgress={status.in_progress ?? null}
            onChanged={() => {
              setGraphKey((k) => k + 1);
              onChanged();
            }}
            onOpenCommit={onOpenCommit}
          />
        </div>
      );
    case "stashes":
      return (
        <div className="flex min-h-0 flex-1 flex-col">
          {card}
          <StashesPanel wid={wid} status={status} onChanged={onChanged} />
        </div>
      );
  }
}
