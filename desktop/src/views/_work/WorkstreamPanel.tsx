/**
 * A workstream's own panel — the Workstreams occupant, on every root (ide/07).
 *
 * The checkout you stand in, **whole**, in this order: what it is called
 * (renamed in place), who is standing in it right now, then where its branch
 * stands on the way to its base — the pull-request lifecycle, on a branch
 * beside the primary (`PullRequestLifecycle`) — and how it ends — closed, or
 * deleted with its checkout. Where it is on disk is never printed (a
 * managed path says nothing a person needs); the ⋮ menu copies it. One panel,
 * because the lifecycle *is* the branch's story and a person who opened the
 * workstream came for it; the one act under the current step is the only
 * button it shows, so nobody meets a merge button before its time. On the
 * primary — the project's own root — there is no closing and no lifecycle,
 * and the panel says so with a chip rather than a greyed verb. Commit, push,
 * pull and fetch are Git › Changes'. The project's other checkouts are the
 * project rail's list; opening a new one is *New workstream…* in this menu
 * as in the rail's — the one dialog, the workbench's. So nothing here
 * duplicates another surface.
 *
 * The reading is one hook (`useWorkstream`): the record and the live status,
 * following the bus, read once and handed to the lifecycle, which asks the
 * code host on top of it (`usePullRequest`) and only for a branch with a pull
 * request.
 */

import { useState } from "react";
import { ApiError, openExternal } from "../../api";
import { errorFields, log } from "../../log";
import { NEW_WORKSTREAM, fire } from "../../shell/shortcuts";
import { scriptRefusal } from "./workstreamScripts.mjs";
import { navigate } from "../../router";
import { Button, Chip, ErrorNote, ICON, Menu, SessionMark, Skeleton, SkeletonRows, Tooltip, WorkingDot, failureText, copyText, useToast, type MenuItem } from "../../ui";
import { useHarnessLabels } from "../../shell/useHarnesses";
import { useSessions } from "../../shell/sessionsStore";
import { useTerminals } from "../../shell/useTerminals";
import { rootKey } from "../_workbench/workbenchModel.mjs";
import { workstreamActivity } from "../_workbench/workstreamActivityModel.mjs";
import { useGitSession, useSessionDraft } from "./gitPanelStore";
import { CloseWorkstreamDialogs, type Closing } from "./CloseWorkstreamDialogs";
import { closeWorkstream } from "./closeWorkstream";
import { terminationCounts, terminationWords } from "./closeWorkstreamModel.mjs";
import { InitRepositoryCard } from "./InitRepositoryCard";
import { initOffer } from "./initRepositoryModel.mjs";
import { PullRequestLifecycle } from "./PullRequestLifecycle";
import { RenameWorkstreamField } from "./RenameWorkstream";
import { useWorkstream } from "./useWorkstream";
import { cardTitle } from "./workstreamCardModel.mjs";
import { WorkstreamSessions } from "./WorkstreamSessions";
import { WorkstreamStatusChips } from "./WorkstreamStatusChips";
import { t } from "../../i18n/l10n.mjs";

export function WorkstreamPanel({
  wid,
  onBack,
  onChanged,
  onOpenAbout,
  onOpenChanges,
  onCheckoutGone,
}: {
  wid: string;
  /** Back to the project this workstream checks out. */
  onBack?: () => void;
  /** The workstream record changed — the caller's list is stale. */
  onChanged?: () => void;
  /** Open About on a view — Settings, where the project's publishing policy is set. */
  onOpenAbout?: (view: "checkout" | "settings") => void;
  /** Switch to Git › Changes — where a commit or a push is made. */
  onOpenChanges?: () => void;
  /** The clean-up after a merge deleted the checkout: this root is gone. */
  onCheckoutGone?: () => void;
}) {
  const toast = useToast();
  const data = useWorkstream(wid);
  // A close in flight and the clean script's refusal live in the checkout's
  // session (`gitPanelStore`): the refusal is the only record of why the
  // checkout stayed, and a tab switch must not lose it. The lifecycle's own
  // work — a pull request opening, a merge — is the same session's `busy`.
  const scope = rootKey("workstream", wid);
  const [busy, setBusy] = useSessionDraft(`${scope}|workstream|busy`, false);
  const session = useGitSession(scope);
  const lifecycleBusy = session.busy === "pr" || session.busy === "merge" ? session.busy : null;
  const [closing, setClosing] = useState<Closing>(null);
  /** The clean script's refusal of the last delete, shown in the delete dialog. */
  const [cleanProblem, setCleanProblem] = useSessionDraft<ReturnType<typeof scriptRefusal> | null>(`${scope}|workstream|cleanProblem`, null);
  const [renaming, setRenaming] = useState(false);

  // Who is standing here, live.
  const sessions = useSessions();
  const { sessions: terminals } = useTerminals();
  const harnessLabels = useHarnessLabels();

  const reloadAll = () => {
    data.reload();
    onChanged?.();
  };

  if (data.loading) {
    return (
      <div className="flex flex-col gap-3" aria-busy>
        <Skeleton className="h-6 w-64" />
        <SkeletonRows rows={6} />
      </div>
    );
  }
  if (data.error || !data.detail) return <ErrorNote error={data.error ?? t("work-pull-request-lifecycle-workstream-not-found")} retry={data.reload} />;

  const d = data.detail;
  const w = d.workstream;
  const s = data.status;
  const isGit = w.kind.kind !== "copy";
  const isPrimary = w.kind.kind === "primary";
  /** A branch beside the primary — the one kind of checkout with a base to go to. */
  const hasBranch = w.kind.kind === "worktree";
  const activity = workstreamActivity(sessions, terminals, w.id);
  const title = cardTitle(w, s);
  // The pull request the record knows of — its door on the code host needs no request.
  const prUrl = w.state.state === "pr_open" || w.state.state === "merged" ? w.state.url : null;

  const terminated = terminationCounts(sessions, terminals, w.id);
  const close = async (tree: boolean) => {
    setBusy(true);
    try {
      const r = await closeWorkstream(wid, { tree });
      const ended = terminationWords(r.terminated);
      toast.ok(
        [
          tree
            ? t("work-workstream-panel-workstream-closed-checkout-removed-what-held", { safety: r.recovery?.ref_name.replace(/^refs\/bisa\/safety\//, "") ?? "", flag: r.recovery ? "yes" : "no" })
            : t("work-workstream-panel-workstream-closed"),
          ...(ended ? [`${ended[0].toUpperCase()}${ended.slice(1)}.`] : []),
        ].join(" "),
      );
      onChanged?.();
      onBack?.();
    } catch (e) {
      // The clean script refused: the checkout stays, and its words belong in
      // the dialog the person is about to retry from, not in a toast.
      if (tree && e instanceof ApiError && e.code === "script_failed") {
        setCleanProblem(scriptRefusal(e));
        setClosing("tree");
      } else {
        toast.error(failureText("work", "workstream-panel-failed", e));
      }
    } finally {
      setBusy(false);
    }
  };

  const copyPath = () => void copyText(d.path).then((ok) => (ok ? toast.ok(t("work-workstream-panel-path-copied")) : toast.error(t("work-git-panel-clipboard-refused"))));

  const menu: MenuItem[] = [
    { label: t("work-commit-graph-refresh"), icon: ICON.refresh, onSelect: reloadAll },
    // A new workstream on this project — the same dialog the rail opens; the
    // workbench owns it, so it opens whether or not the rail is on screen.
    { label: t("work-workstream-panel-new-workstream"), icon: ICON.workstream, onSelect: () => fire(NEW_WORKSTREAM, { pid: w.project }) },
    { label: t("work-workstream-panel-rename"), icon: ICON.edit, separatorBefore: true, onSelect: () => setRenaming(true) },
    { label: t("work-workstream-panel-copy-checkout-path"), icon: ICON.copy, onSelect: copyPath },
    ...(prUrl ? [{ label: t("work-workstream-panel-open-pull-request-code-host"), icon: ICON.pullRequest, separatorBefore: true, onSelect: () => void openExternal(prUrl).catch((e: unknown) => log.warn("shell", "the machine's browser could not be opened", { url: prUrl, ...errorFields(e) })) }] : []),
    ...(isGit && !isPrimary ? [{ label: t("work-workstream-panel-diff-against-base"), icon: ICON.workstream, separatorBefore: !prUrl, onSelect: () => navigate({ name: "workbench", scope: "workstream", id: wid }, { doc: "diff" }) }] : []),
    ...(!isPrimary
      ? [
          { label: t("work-workstream-panel-close-workstream"), icon: ICON.close, danger: true, separatorBefore: true, disabled: busy, onSelect: () => setClosing("record") },
          { label: t("work-workstream-panel-delete-checkout"), icon: ICON.delete, danger: true, disabled: busy, onSelect: () => setClosing("tree") },
        ]
      : []),
  ];

  return (
    <div className="flex min-w-0 flex-col gap-5">
      {onBack && (
        <div>
          <Button size="sm" variant="ghost" onClick={onBack}>
            <ICON.back size={12} aria-hidden />
            {d.project?.name ?? t("work-new-project-dialog-project")}
          </Button>
        </div>
      )}

      {/* The header reads the same model the list's rows read (`cardChips`),
          so "+3" and t("work-workstream-panel-pr-open") mean one thing in both places. The title is
          the name over the branch; double-click it, or Rename, to change it. */}
      <header className="flex flex-col gap-1">
        <div className="flex items-center gap-2">
          <ICON.workstream size={14} aria-hidden className="shrink-0 text-text-dim" />
          {renaming ? (
            <RenameWorkstreamField wid={wid} name={w.name ?? null} placeholder={d.branch ?? w.id} onDone={() => setRenaming(false)} />
          ) : (
            <span className="group flex min-w-0 flex-1 items-center gap-1">
              <h2 className="min-w-0 truncate font-mono text-sm font-semibold text-text" title={d.branch ?? undefined} onDoubleClick={() => setRenaming(true)}>
                {title}
              </h2>
              <Tooltip label={t("work-workstream-panel-rename-double-click-name")}>
                <span className="row-actions anim inline-flex">
                  <Button size="icon" variant="ghost" className="h-5 w-5" aria-label={t("work-workstream-panel-rename-workstream")} onClick={() => setRenaming(true)}>
                    <ICON.edit size={11} aria-hidden />
                  </Button>
                </span>
              </Tooltip>
            </span>
          )}
          {/* The base yields too, capped, so a long base never squeezes the
              workstream's own name down to its first letter. */}
          {hasBranch && s?.base && (
            <span className="flex max-w-[45%] min-w-0 items-center gap-1 font-mono text-2xs text-text-dim" title={s.base}>
              <ICON.forward size={10} aria-hidden className="shrink-0" />
              <span className="min-w-0 truncate">{s.base}</span>
            </span>
          )}
          {(activity.counts.agents > 0 || activity.counts.live > 0) && (
            <SessionMark state={activity.state} title={t("work-workstream-panel-agents-working-shells", { agents: activity.counts.agents, working: activity.counts.working, live: activity.counts.live })} />
          )}
          {busy && <WorkingDot title={t("work-workstream-panel-closing")} />}
          {!busy && lifecycleBusy && <WorkingDot title={lifecycleBusy === "pr" ? t("work-workstream-panel-opening-pull-request") : t("work-workstream-panel-merging")} />}
          <Menu
            label={t("work-workstream-panel-workstream-actions")}
            trigger={
              <span className="anim flex h-7 w-7 items-center justify-center rounded-control border border-border text-text-dim hover:bg-surface-2 hover:text-text">
                <ICON.more size={14} aria-hidden />
              </span>
            }
            items={menu}
          />
        </div>
        <div className="flex flex-wrap items-center gap-1 pl-5">
          {isPrimary && (
            <Tooltip label={t("work-workstream-panel-project-s-own-root-remove-project")}>
              <span>
                <Chip tone="quiet">{t("work-workstream-panel-primary")}</Chip>
              </span>
            </Tooltip>
          )}
          <WorkstreamStatusChips w={w} s={s} />
          {w.name && d.branch && d.branch !== title && <span className="font-mono text-2xs text-text-dim">{d.branch}</span>}
          {!isGit && <span className="text-2xs text-text-dim">{t("work-workstream-panel-copy-no-branch")}</span>}
        </div>
        {(!d.status.exists || d.status.error) && (
          <p className="pl-5 text-2xs text-danger">{!d.status.exists ? t("work-workstream-panel-checkout-not-disk-close-workstream-restore") : d.status.error}</p>
        )}
      </header>

      {/* A plain folder's own root: the one offer to make it a repository (ide/07). */}
      {initOffer({ kind: w.kind.kind, exists: d.status.exists, git: d.status.git }).shown && <InitRepositoryCard pid={d.project.id} adopted={d.project.root.type === "external"} path={d.path} onDone={() => data.reload()} />}

      {/* Who is standing in it right now — the checkout first, before its
          branch's story. Where it is on disk is not shown: the ⋮ menu copies
          the path for a shell of your own or a file manager. */}
      <WorkstreamSessions sessions={sessions} terminals={terminals} workstream={w.id} harnessLabels={harnessLabels} />

      {/* The branch's way to its base: the one act under the current step,
          the card, the review, the merge. */}
      {hasBranch && (
        <PullRequestLifecycle base={data} onChanged={onChanged} onOpenAbout={onOpenAbout} onOpenChanges={onOpenChanges} onCheckoutGone={onCheckoutGone} />
      )}


      <CloseWorkstreamDialogs
        mode={closing}
        onModeChange={(mode) => {
          if (mode === null) setCleanProblem(null);
          setClosing(mode);
        }}
        problem={cleanProblem}
        path={d.path}
        clean={s?.clean ?? d.status.clean}
        branch={isGit ? d.branch : null}
        busy={busy}
        terminated={terminated}
        onClose={(tree) => void close(tree)}
      />
    </div>
  );
}
