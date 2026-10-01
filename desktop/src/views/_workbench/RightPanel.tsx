/**
 * The right panel's column (layout's rail): one occupant at a time — Files,
 * Git, Workstreams, Agent or About from the rail beside it
 * (`OccupantRail`), which the workbench draws whether or not this column is
 * showing. Which occupant is showing is the workbench's decision
 * (`resolveOccupant`); this file paints its body.
 */

import { refreshWorkstreamStatuses } from "../../shell/workstreamStatusStore";
import { serveAndOpen } from "../../shell/serveDoors";
import { canOpenBrowser } from "../../shell/useBrowsers";
import React, { useEffect, useMemo } from "react";
import { api } from "../../api";
import type { WorkbenchScope } from "../../router";
import type { Standing } from "../../ui";
import { useEngineEvents } from "../../bus";
import { Button, Chip, ErrorNote, FileTreeView, ICON, SectionHeader, SegmentedControl, SkeletonRows, Tooltip, closeExplorerSearch, foldStandings, openExplorerSearch, requestReveal, useExplorerSearch, useWatchLease } from "../../ui";
import { firstCaution, hasCautions } from "../_work/connectionModel.mjs";
import { useDirtyEditors } from "./editorRegistry";
import { FilesSearch } from "./FilesSearch";
import { rootKey } from "./workbenchModel.mjs";
import { idePlace } from "./idePlacesModel.mjs";
import { useViewState } from "../../shell/viewMemoryStore";
import { wordsValue } from "../../shell/viewValuesModel.mjs";
import { GitViewBody } from "../_work/GitViewBody";
import { treeStandings } from "../_work/gitFiles.mjs";
import { useGitFiles } from "../_work/gitFilesStore";
import { readsFor } from "../_work/gitChangeModel.mjs";
import { useGitChanges } from "../_work/useGitChanges";
import { useWorkstreamStatus } from "../../shell/workstreamStatusStore";
import { ProjectDetail } from "../_work/ProjectDetail";
import { CheckoutView } from "../_work/CheckoutView";
import { InitRepositoryCard } from "../_work/InitRepositoryCard";
import { ProjectSettingsView } from "../_work/ProjectSettingsView";
import { WorkItemPanel } from "../_work/WorkItemPanel";
import { WorkstreamPanel } from "../_work/WorkstreamPanel";
import { useAsync } from "../_work/useAsync";
import { AgentPane } from "./AgentPane";
import { ABOUT_VIEWS, ABOUT_VIEW_LABEL, GIT_VIEWS, GIT_VIEW_LABEL, OCCUPANT_LABEL, aboutBody, occupantScroll } from "./rightPanelModel.mjs";
import { openPanelView, setPanelView, useRightPanel } from "./rightPanelStore";
import type { AboutView, GitView, Occupant } from "./rightPanelStore";
import type { Home } from "./ideHomeModel.mjs";
import { t } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";

/** A tree nobody unfolded: one list, so a root with nothing kept starts from the same beginning every render. */
const NO_FOLDERS: string[] = [];

export function RightPanel({
  occupant,
  scope,
  id,
  project,
  isPrimary,
  rootLabel,
  goals,
  owner,
  exists,
  path,
  activeFile,
  openPath,
  onOpenFile,
  onOpenAt,
  onFileDeleted,
  onOpenWorkstream,
  onOpenDiffDocument,
  onOpenPatch,
  onOpenCommit,
  onLeft,
  onLeftWorkstream,
}: {
  /** What is showing — already resolved against the root's availability by the workbench. */
  occupant: Occupant;
  scope: WorkbenchScope;
  id: string;
  project: string | null;
  isPrimary: boolean;
  /** The root's label — the workstream's name, or the project's for the primary — the Agent pane's subject. */
  rootLabel: string;
  goals: { id: string; label: string }[];
  owner: string | null;
  exists: boolean;
  path: string | null;
  activeFile: string | null;
  openPath: string | null;
  /** Open a document — as a preview for a glance, kept otherwise. */
  onOpenFile: (path: string, opts?: { preview?: boolean }) => void;
  /** Open a document and show a line of it — a search hit's. */
  onOpenAt: (path: string, line: number | null, opts?: { preview?: boolean }) => void;
  /** Paths that were deleted: the documents open on them close, together. */
  onFileDeleted: (paths: string[]) => void;
  onOpenWorkstream: (wid: string) => void;
  /** Absent on a root that has no patch document — the primary, a goal. */
  onOpenDiffDocument?: () => void;
  /** Show one changed file's patch on one side in the centre. */
  onOpenPatch: (path: string, staged: boolean) => void;
  /** Show one commit in the centre. */
  onOpenCommit: (sha: string) => void;
  /** The root's project was archived or removed from About: the workbench leaves for the next home, or the landing. */
  onLeft: (home: Home | null) => void;
  onLeftWorkstream: () => void;
}) {
  const panel = useRightPanel();
  const tab = occupant;
  const key = rootKey(scope, id);
  const search = useExplorerSearch(key);
  // The folders the Files tree was left unfolded with are the root's view:
  // an occupant switch, leaving the IDE and a restart come back to them.
  const [openFolders, setOpenFolders] = useViewState(idePlace(key), "files.open", NO_FOLDERS, wordsValue);
  // The files with unsaved changes in this root's documents, for the tree's dots.
  const dirty = useDirtyEditors();
  const dirtyPaths = useMemo(() => {
    const out = new Set<string>();
    const prefix = `${key}|file:`;
    for (const k of dirty) if (k.startsWith(prefix)) out.add(k.slice(prefix.length));
    return out;
  }, [dirty, key]);
  // What git says about the root's paths, for the tree's colours: one read
  // shared with the Git panel, folded into the folders by the kit's model.
  // A root with no repository asks nothing.
  const standings = useTreeStandings(scope === "workstream" ? id : null);
  // The tree follows the active document (VS Code's auto-reveal): a tab
  // clicked in the strip is shown in Files, now or when the tree mounts.
  useEffect(() => {
    if (tab === "files" && openPath) requestReveal(key, openPath);
  }, [tab, key, openPath]);

  return (
    <aside data-pane className="flex min-h-0 min-w-0 flex-1 flex-col" aria-label={OCCUPANT_LABEL[tab]} role="tabpanel">
      {/* The body fills the column by flex, never by percentage (ide/03 §The
          explorer): for an occupant with its own scrollport it is a flex
          column that clips, so the tree reaches the bottom and scrolls inside
          its box; for one that scrolls as one — Git under its sticky
          composer, every PanelBody — it is the scrollport. */}
      <div data-scrollport={occupantScroll(tab) === "panel" || undefined} className={occupantScroll(tab) === "own" ? "flex min-h-0 flex-1 flex-col overflow-hidden" : "min-h-0 flex-1 overflow-auto"}>
        {tab === "files" && (
          <div className="flex min-h-0 flex-1 flex-col p-2">
            {!exists ? (
              // A directory the workspace has not laid down is not this panel's
              // to create — the checkout is the workstream's. Say whose it is
              // and offer the way there rather than a dead end.
              <div className="flex flex-col gap-2 px-1 py-2 text-2xs text-text-dim">
                <p>{rich("workbench-right-panel-nothing-at-path-yet", { path: <span className="font-mono">{path ?? t("workbench-right-panel-that-path")}</span> })}</p>
                <button type="button" onClick={() => onOpenWorkstream(id)} className="self-start underline underline-offset-2 hover:text-text">{t("workbench-right-panel-open-workstream-s-panel")}</button>
              </div>
            ) : (
              <>
                {search.open && <FilesSearch scope={scope} id={id} focusNonce={search.nonce} onOpen={onOpenAt} onClose={() => closeExplorerSearch(key)} />}
                <FileTreeView
                  fill
                  mutable
                  title={t("workbench-right-panel-files")}
                  scope={scope}
                  id={id}
                  goal={owner ?? undefined}
                  onDeleted={onFileDeleted}
                  // A folder served on this machine, opened in the embedded browser (ide/18).
                  onServeFolder={scope === "workstream" && canOpenBrowser() ? (path) => void serveAndOpen(id, path) : undefined}
                  // A new file opens as a kept document in the middle panel;
                  // the reveal effect above then selects it in the tree.
                  onCreated={(p, kind) => {
                    if (kind === "file") onOpenFile(p);
                  }}
                  onOpenFile={onOpenFile}
                  openPath={openPath}
                  dirtyPaths={dirtyPaths}
                  standings={standings}
                  onSearch={() => openExplorerSearch(key)}
                  openFolders={openFolders}
                  onOpenFolders={setOpenFolders}
                  emptyHint={t("workbench-right-panel-folder-empty")}
                />
              </>
            )}
          </div>
        )}
        {tab === "git" && project && (
          <div className="flex h-full min-h-0 flex-col">
            <GitTab pid={project} wid={id} isPrimary={isPrimary} view={panel.views.git} onOpenDiff={onOpenDiffDocument} onOpenFile={(p) => onOpenFile(p)} onOpenPatch={onOpenPatch} onOpenCommit={onOpenCommit} rootPath={path} />
          </div>
        )}
        {tab === "agents" && project && (
          <div className="flex min-h-0 flex-1 flex-col">
            <AgentPane wid={id} pid={project} title={rootLabel} activeFile={activeFile} />
          </div>
        )}
        {tab === "workstreams" && project && (
          <PanelBody>
            {/* The checkout you stand in, whole — name, the branch's way to its
                base (the lifecycle, on a branch beside the primary), sessions,
                path, closing — on every root, the primary included. A checkout's
                facts are this occupant's, never About's; the project's other
                checkouts are the rail's list; *New workstream…* is in its menu too. */}
            <WorkstreamPanel
              wid={id}
              onBack={isPrimary ? undefined : onLeftWorkstream}
              onOpenAbout={(view) => openPanelView("about", view)}
              onOpenChanges={() => openPanelView("git", "changes")}
              onCheckoutGone={onLeftWorkstream}
            />
          </PanelBody>
        )}
        {tab === "about" && (
          <PanelBody>
            {aboutBody({ scope, hasProject: project !== null }) === "project" && project ? (
              <AboutTab pid={project} wid={id} view={panel.views.about} goals={goals} onLeft={onLeft} />
            ) : (
              <SectionHeader title={OCCUPANT_LABEL.about} />
            )}
            {scope === "work_item" && <WorkItemPanel itemId={id} inWorkbench />}
            {scope === "goal" && (
              <p className="text-xs text-text-dim">
                {rich("workbench-right-panel-goal-own-folder-lives-on", { goal: <a href={`#/goals/${id}`} className="text-accent-ink underline underline-offset-2">{t("workbench-right-panel-goal")}</a> })}
              </p>
            )}
          </PanelBody>
        )}
      </div>
    </aside>
  );
}

/** One padding for every occupant's body, so the panels line up when you switch between them. */
function PanelBody({ children }: { children: React.ReactNode }) {
  return <div className="flex min-h-0 flex-col gap-3 p-2">{children}</div>;
}

/**
 * About's three views (`ABOUT_VIEWS`): the project — its identity and
 * relations; this checkout — the repository as it reaches it; the project's
 * settings — what is saved on the project itself. The header row is sticky
 * so the two settings views' Save toolbar stacks under it.
 */
function AboutTab({ pid, wid, view, goals, onLeft }: { pid: string; wid: string; view: AboutView; goals: { id: string; label: string }[]; onLeft: (home: Home | null) => void }) {
  return (
    <>
      <div className="sticky top-0 z-20 -mx-2 -mt-2 flex items-center gap-2 bg-surface px-2 pt-2">
        <SectionHeader title={OCCUPANT_LABEL.about} />
        <span className="flex-1" />
        <SegmentedControl
          label={t("workbench-right-panel-about-view")}
          size="sm"
          value={view}
          onChange={(v) => setPanelView("about", v as AboutView)}
          options={ABOUT_VIEWS.map((v) => ({ id: v, label: ABOUT_VIEW_LABEL[v] }))}
        />
      </div>
      {view === "project" && <ProjectDetail pid={pid} goals={goals} onLeft={onLeft} />}
      {view === "checkout" && <CheckoutView pid={pid} wid={wid} />}
      {view === "settings" && <ProjectSettingsView pid={pid} wid={wid} />}
    </>
  );
}

/** Changes, branches, history and stashes for one checkout, keyed by the workstream. */
/**
 * The explorer's standings for a workstream root: the changed files from the
 * one shared read (`gitFilesStore`), made into kinds by the git model and
 * folded up the folders by the kit's. `undefined` — no colouring — for a
 * root without a repository, or until the status says it has one.
 */
function useTreeStandings(wid: string | null): ReadonlyMap<string, Standing> | undefined {
  const status = useWorkstreamStatus(wid);
  const git = wid !== null && status?.git === true;
  const files = useGitFiles(git ? wid : null);
  return useMemo(() => (git && files.data ? foldStandings(treeStandings(files.data.files)) : undefined), [git, files.data]);
}

function GitTab({
  pid,
  wid,
  isPrimary,
  view,
  onOpenDiff,
  onOpenFile,
  onOpenPatch,
  onOpenCommit,
  rootPath,
}: {
  pid: string;
  wid: string;
  isPrimary: boolean;
  view: GitView;
  /** Absent where there is no patch document to open — the primary. */
  onOpenDiff?: () => void;
  /** Open a changed file as a document — a row's menu. */
  onOpenFile: (path: string) => void;
  onOpenPatch: (path: string, staged: boolean) => void;
  onOpenCommit: (sha: string) => void;
  /** The checkout's absolute path, for *Copy absolute path*. */
  rootPath: string | null;
}) {
  const status = useAsync((s) => api.workstreamGitStatus(wid, s), [wid]);
  const detail = useAsync((s) => api.workstream(wid, s), [wid]);
  // The connection's cautions — the wrong author, an unloaded key, no account
  // — wear a dot beside the views so About › Settings is one click away
  // before a push. Read once per root; re-read when the setup or a remote moves.
  const connection = useAsync((s) => api.gitConnection(wid, s).catch(() => null), [wid]);
  useEngineEvents((e) => {
    if (e.payload.type === "git_setup_changed" || e.payload.type === "committer_set") connection.reload();
    // A plain folder became a repository: the standing, the record and the
    // connection all read differently now.
    if (e.payload.type === "project_changed" && e.payload.project === detail.data?.project.id) {
      status.reload();
      detail.reload();
      connection.reload();
    }
  });
  // The tab follows the checkout (ide/04 §Live): it holds the watcher's
  // lease while it shows — Files or no Files — and re-reads where the
  // branch stands on any change the watcher reports, so a commit in a
  // terminal, an agent's git or a fetch from elsewhere reaches the sync bar
  // and the rail's marks without a click. Each view listens for its own.
  useWatchLease("workstream", wid);
  useGitChanges(wid, (kinds) => {
    if (!readsFor(kinds).status) return;
    status.reload();
    refreshWorkstreamStatuses();
  });
  if (status.error) return <ErrorNote error={status.error} retry={status.reload} />;
  if (!status.data || !detail.data) return <SkeletonRows rows={6} className="p-3" />;
  if (!status.data.status.exists) return <p className="p-3 text-xs text-danger">{t("workbench-right-panel-folder-not-disk")}</p>;
  if (!status.data.status.git) {
    // The one offer a plain folder gets (ide/04): the sentence and the button.
    const project = detail.data.project;
    return (
      <div className="p-2">
        <InitRepositoryCard
          pid={project.id}
          adopted={project.root.type === "external"}
          path={rootPath}
          onDone={() => {
            status.reload();
            detail.reload();
            connection.reload();
          }}
        />
      </div>
    );
  }
  const reload = () => {
    status.reload();
    detail.reload();
    // A stage or a discard raises no frame: the rail's mark is told here.
    refreshWorkstreamStatuses();
  };
  const goal = detail.data.workstream.goal ?? null;
  // The column's height reaches the view: History and Stashes fill it and
  // scroll inside; Changes is taller than it and scrolls the panel.
  return (
    <div className="flex h-full min-h-0 flex-col gap-2 p-2">
      <div className="flex shrink-0 items-center gap-2">
        <SegmentedControl
          label={t("workbench-right-panel-git-view")}
          size="sm"
          value={view}
          onChange={(v) => setPanelView("git", v as GitView)}
          options={GIT_VIEWS.map((v) => ({ id: v, label: GIT_VIEW_LABEL[v] }))}
        />
        {hasCautions(connection.data) && (
          <Tooltip label={firstCaution(connection.data) ?? t("workbench-right-panel-connection-has-caution-see-about-checkout")}>
            <button type="button" className="inline-flex" aria-label={t("workbench-right-panel-connection-has-caution-open-checkout-s")} onClick={() => openPanelView("about", "checkout")}>
              <Chip tone="warn" icon={ICON.warn}>
                {t("workbench-right-panel-cautions", { cautions: connection.data?.cautions.length ?? 0 })}
              </Chip>
            </button>
          </Tooltip>
        )}
        <span className="flex-1" />
        {!isPrimary && onOpenDiff && (
          <Button size="sm" variant="ghost" onClick={onOpenDiff}>
            <ICON.workstream size={12} aria-hidden />{t("workbench-right-panel-diff-against-base")}</Button>
        )}
      </div>
      <GitViewBody
        pid={pid}
        wid={wid}
        goal={goal}
        view={view}
        status={status.data.status}
        defaultBranch={status.data.default_branch}
        onChanged={reload}
        onOpenAbout={(view) => openPanelView("about", view)}
        onOpenFile={onOpenFile}
        onOpenPatch={onOpenPatch}
        onOpenCommit={onOpenCommit}
        rootPath={rootPath}
      />
    </div>
  );
}
