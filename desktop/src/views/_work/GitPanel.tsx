/**
 * A checkout's working tree — the Git tab's Changes view (ide/04): what
 * changed, what is staged, one file's patch, the notes on it, and the commit.
 *
 * The order is the order of the work, top to bottom: where the branch stands
 * (the sync bar) → how much changed and who will sign it (one line) → the
 * changes, one tree in the project's own shape — every changed file once,
 * in its folder, wearing its standing — as folders or as a flat list
 * (`ChangesTree`, with the layout switch, the filter — one standing of the
 * six, narrowing the rows and never the verbs — and *Stage all* by scope
 * above it, `ChangesToolbar`) → the **commit composer**, pinned at the bottom while
 * the files scroll → and, last, the review notes. The selected file's patch
 * is not here: the list *points* at a file, and its patch opens in the
 * centre as a document (`PatchDocument`, a preview on a glance, kept on an
 * act), where a patch has the width it wants. Nothing has to be scrolled
 * past to reach the message once the staging is done.
 *
 * Three facts from the API shape everything here.
 *
 * **The two letters are kept apart.** `index` and `worktree` are separate
 * because a file can be staged *and* modified since, so a row has two sides
 * — that is the state this surface exists to show, and collapsing it to one
 * badge throws away the thing a person needs before committing. A row wears
 * a chip per side, each the word for that side (`standingChips`) and the
 * door to that side's patch; the letters are the tooltip.
 *
 * **A read must not stage.** `GET /git/diff` answers an untracked file with
 * `diff: ""` and `untracked: true` rather than staging it to manufacture a
 * patch. So selecting a row and staging a row are two different controls: a
 * single click that did both would mean you cannot look at a file without
 * changing the index, which is the exact rule the route is following. The
 * tree is one tab stop: the arrows move and fold, Enter selects a file or
 * folds a folder, Space stages or unstages what the cursor holds — a file,
 * or every file under a folder the verb applies to — Delete discards or
 * deletes it (asked first), Shift+F10 opens the row's menu.
 *
 * **Suggest never commits.** `POST /git/message` always answers 200, and
 * `suggested: false` with an `error` leaves an empty box and a sentence. The
 * session behind it runs read-only with no MCP servers, so the thing that
 * drafts a commit message cannot make a commit; the last word — and the name
 * on the commit — stays the user's, and the composer says whose.
 *
 * Nothing on the engine bus announces a project commit, and it does not need
 * to: every write route answers with the fresh file list, so the panel
 * refreshes from its own response. What it cannot see on its own is an
 * *agent* writing into the project root, so it also listens to the same
 * "something moved on disk" events the file tree does, and offers a Refresh.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { api, inDesktopShell } from "../../api";
import type { Disposal, GitStatusInfo } from "../../types";
import { Button, ConfirmDialog, EmptyState, ErrorNote, ICON, KeyHint, Skeleton, SkeletonRows, Switch, TextArea, Tooltip, WorkingDot, requestReveal, useToast } from "../../ui";
import { copyText } from "../../ui";
import type { MenuItem } from "../../ui";
import { chordHint } from "../../ui/keymapHints";
import { joinPath } from "../../ui/fileTreeModel.mjs";
import { attachGitChanges } from "../_workbench/gitContext";
import { setPanelView, showRightPanel, usePanelView } from "../_workbench/rightPanelStore";
import { rootKey } from "../_workbench/workbenchModel.mjs";
import { amendDraft, amendPlaceholder, amendWords, headMessage } from "./amendModel.mjs";
import { ChangesToolbar } from "./ChangesToolbar";
import { ChangesTree } from "./ChangesTree";
import { deleteCopy, discardCopy } from "./gitDiscardModel.mjs";
import * as ops from "./gitOps";
import type { GitFailure, GitPending } from "./gitPanelModel.mjs";
import { refreshGitFiles, useGitFiles } from "./gitFilesStore";
import { patchSession, setMessage, useGitDraft, useGitSession } from "./gitPanelStore";
import type { DirRow, FileRow } from "./gitTreeModel.mjs";
import { changesTreeRows, fileId, foldableIds, gitFolderMenu, nextCursor, toggledFold } from "./gitTreeModel.mjs";
import { emptyWords, filterCounts, filterGitFiles } from "./changesFilterModel.mjs";
import { ReasonLine } from "./ReasonLine";
import { ReviewNotes } from "./ReviewNotes";
import { SafetyNote } from "./SafetyNote";
import { commitBlockedReason, groupGitFiles, gitFileMenu, isClean, noCommitsYet, selectionOf, stageScopes, type GitSelection } from "./gitFiles.mjs";
import { identityBlockedReason, identityState } from "./gitIdentityModel.mjs";
import { PLACE, VERB, fileSummary } from "./gitWords.mjs";
import { SyncBar } from "./SyncBar";
import { useAsync } from "./useAsync";
import { t as tr } from "../../i18n/l10n.mjs";

/**
 * A throw-away waiting on the person's word — a discard, a delete — is
 * `GitPending` in the checkout's session; one dialog, one act, and a tab
 * switch does not close the question.
 */
type Pending = GitPending;


export function GitPanel({
  pid,
  wid,
  goal,
  status,
  defaultBranch,
  onChanged,
  onOpenAbout,
  onOpenFile,
  onOpenPatch,
  rootPath = null,
}: {
  /** The project — review notes are filed under it. */
  pid: string;
  /** The checkout every git call runs in. */
  wid: string;
  /**
   * Scopes the engine subscription to one goal this project is attached to.
   * A project attached to none listens to every frame — the cost is a few
   * spare refreshes, not a missed one.
   */
  goal: string | null;
  /** Already loaded by the caller for `GitFacts` — never fetched twice here. */
  status: GitStatusInfo;
  /** The project's default branch — the sync bar never force-pushes it. */
  defaultBranch: string | null;
  /** The tree changed: the caller's `/status` and its dot are now stale. */
  onChanged?: () => void;
  /** Open About — Checkout, where who commits here is set; Settings, where the publishing policy is. */
  onOpenAbout?: (view: "checkout" | "settings") => void;
  /** Open a changed file as a document — a row's menu; absent where no editor is near. */
  onOpenFile?: (path: string) => void;
  /** Show a file's patch on one side in the centre — what selecting a row does. */
  onOpenPatch: (path: string, staged: boolean) => void;
  /** The checkout's absolute path, for *Copy absolute path*. */
  rootPath?: string | null;
}) {
  // Who would author the commit. Refetched with the status, which is what a
  // save under About › Checkout reloads.
  const identity = useAsync((s) => api.gitIdentity(wid, s), [wid, status]);
  /**
   * What this panel is doing and has typed lives in the checkout's session
   * and draft (`gitPanelStore`), not here: the panel is unmounted by every
   * tab switch, and a Suggest that landed after the switch must still be
   * there when the person comes back. The operations are `gitOps`'s.
   */
  const scope = rootKey("workstream", wid);
  const session = useGitSession(scope);
  const { message } = useGitDraft(scope);
  const { busy, pending, note, failure, selection, amend } = session;
  // HEAD in full, read only while the Amend switch is on: its message fills
  // the box, its subject names the confirmation.
  const headView = useAsync((s) => (amend && status.head ? api.gitShowCommit(wid, status.head, s) : Promise.resolve(null)), [amend, status.head, wid]);
  const headText = headView.data ? headMessage(headView.data.commit.subject, headView.data.commit.body) : null;
  const headShort = headView.data?.commit.short ?? status.head?.slice(0, 7) ?? "";
  const setAmend = (on: boolean) => {
    patchSession(scope, { amend: on });
    const next = amendDraft(on, message, headText);
    if (next !== message) setMessage(scope, next);
  };
  // The read lands after the switch: an empty box then takes HEAD's message.
  useEffect(() => {
    if (amend && headText !== null && message.trim() === "") setMessage(scope, headText);
    // The draft is read, never a dependency: typing must not refill the box.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [amend, headText, scope]);
  const setPending = (p: Pending | null) => patchSession(scope, { pending: p });
  /** The row pressed, and its patch in the centre — one act, two places that agree. */
  const select = (sel: GitSelection) => {
    patchSession(scope, { selection: sel });
    onOpenPatch(sel.path, sel.staged);
  };
  /** How this root disposes of a deleted file — read before a delete is asked about, so the dialog never changes under the person. */
  const [disposal, setDisposal] = useState<Disposal | null>(null);
  /** A delete whose disposal is still being read: the rows wait, the dialog has not opened. */
  const [askingDelete, setAskingDelete] = useState(false);
  /** The row the keyboard is on, by its tree id; the tree is one tab stop. */
  const [cursor, setCursor] = useState<string | null>(null);
  /** Bumped to bring the cursor row into view — the sync bar pointing at a conflicted file. */
  const [revealNonce, setRevealNonce] = useState(0);
  /** Folders or a flat list — one choice for every checkout. */
  const layout = usePanelView("changes");
  /** Which changed files the tree draws — every one, or one standing; remembered beside the layout. */
  const filter = usePanelView("changesFilter");

  // The checkout's changed files: one read shared with the explorer's standings.
  const listing = useGitFiles(wid);

  const toast = useToast();
  const copy = (text: string) => void copyText(text).then((ok) => (ok ? toast.ok(tr("work-git-panel-copied")) : toast.error(tr("work-git-panel-clipboard-refused"))));
  const reveal = (path: string) => {
    showRightPanel("files", `workstream:${wid}`);
    requestReveal(`workstream:${wid}`, path);
  };

  /** A changed file's menu (`gitFileMenu`): a deleted file has no document to open or reveal. */
  const menuFor = (t: FileRow): MenuItem[] => {
    const row = t.row;
    const gone = row.index === "D" || row.worktree === "D";
    const act: Record<string, () => void> = {
      open: () => onOpenFile?.(row.path),
      "attach-agent": () => void attachGitChanges(wid, `workstream:${wid}`, [row.path]),
      stage: () => void ops.stage(scope, wid, [row.path]),
      unstage: () => void ops.unstage(scope, wid, [row.path]),
      discard: () => setPending({ kind: "discard", paths: [row.path] }),
      delete: () => void askDelete([row.path], null),
      "copy-path": () => copy(row.path),
      "copy-absolute": () => copy(rootPath ? joinPath(rootPath, row.path) : row.path),
      "reveal-files": () => reveal(row.path),
    };
    return gitFileMenu({ standing: t.standing, desktop: inDesktopShell() && !!rootPath, canOpen: !!onOpenFile && !gone }).map((item) => ({
      label: item.label,
      disabled: item.disabled || busy !== null,
      danger: item.danger,
      separatorBefore: item.separatorBefore,
      shortcut: chordHint(item.command),
      onSelect: act[item.id] ?? (() => undefined),
    }));
  };

  /** A folder's menu (`gitFolderMenu`): each verb over the files under it that it applies to. */
  const folderMenuFor = (t: DirRow): MenuItem[] => {
    const act: Record<string, () => void> = {
      stage: () => void ops.stage(scope, wid, t.stageable),
      unstage: () => void ops.unstage(scope, wid, t.unstageable),
      discard: () => setPending({ kind: "discard", paths: t.discardable, under: t.path }),
      delete: () => void askDelete(t.deletable, t.path),
      "copy-path": () => copy(t.path),
      "copy-absolute": () => copy(rootPath ? joinPath(rootPath, t.path) : t.path),
      "reveal-files": () => reveal(t.path),
    };
    return gitFolderMenu(t, { desktop: inDesktopShell() && !!rootPath }).map((item) => ({
      label: item.label,
      disabled: busy !== null,
      danger: item.danger,
      separatorBefore: item.separatorBefore,
      shortcut: chordHint(item.command),
      onSelect: act[item.id] ?? (() => undefined),
    }));
  };

  /** A delete asks how this root disposes first, then asks the person — so the dialog says Trash or gone from its first frame; `all` is the toolbar's, every untracked file. */
  const askDelete = async (paths: string[], under: string | null, all = false) => {
    if (askingDelete || paths.length === 0) return;
    setAskingDelete(true);
    try {
      const r = await api.ideDisposal("workstream", wid);
      setDisposal(r.disposal);
    } catch {
      setDisposal(null);
    } finally {
      setAskingDelete(false);
    }
    setPending({ kind: "delete", paths, under, all });
  };
  const listingData = listing.data;
  // A genuine read replaces the rows the last write answered with.
  useEffect(() => patchSession(scope, { files: null }), [listingData, scope]);
  // A write landed — here or while this panel was away: the reads are stale.
  const seenStale = useRef(session.stale);
  useEffect(() => {
    if (seenStale.current === session.stale) return;
    seenStale.current = session.stale;
    refreshGitFiles(scope);
    onChanged?.();
  }, [session.stale, scope, onChanged]);

  const files = useMemo(() => session.files ?? listingData?.files ?? [], [session.files, listingData?.files]);
  const groups = useMemo(() => groupGitFiles(files), [files]);
  const unborn = noCommitsYet(status);

  // The tree's rows: every changed file once, in the project's folders or
  // as a list; what is folded is the checkout's session's, so a tab switch
  // keeps it. The cursor follows the rows when a write moves them.
  const folds = session.changesFolds;
  // The filter narrows what is drawn — the rows, the folds, the cursor — and
  // never the verbs: the scopes and the counts read the whole listing.
  const shown = useMemo(() => filterGitFiles(files, filter), [files, filter]);
  const counts = useMemo(() => filterCounts(files), [files]);
  const rows = useMemo(() => changesTreeRows(shown, layout, new Set(folds)), [shown, layout, folds]);
  const scopes = useMemo(() => stageScopes(groups), [groups]);
  const prevRows = useRef(rows);
  useEffect(() => {
    const next = nextCursor(prevRows.current, cursor, rows);
    prevRows.current = rows;
    if (next !== cursor) setCursor(next);
  }, [rows, cursor]);
  const fold = (id: string, open: boolean) => patchSession(scope, (s) => ({ ...s, changesFolds: toggledFold(s.changesFolds, id, open) }));
  const foldable = useMemo(() => foldableIds(shown), [shown]);
  const collapseAll = () => patchSession(scope, (s) => ({ ...s, changesFolds: [...new Set([...s.changesFolds, ...foldable])] }));

  const run = (kind: "stage" | "unstage", paths: string[]) => (kind === "stage" ? ops.stage(scope, wid, paths) : ops.unstage(scope, wid, paths));
  const discard = (paths: string[]) => ops.discard(scope, wid, paths);
  const remove = (paths: string[]) => ops.remove(scope, wid, paths, disposal);
  const commit = () => ops.commit(scope, wid, message);
  const suggest = () => ops.suggest(scope, wid);
  /** The primary verb: a commit, or — with the switch on — the amend, asked first. */
  const record = () => (amend ? setPending({ kind: "amend" }) : void commit());

  /** The person said yes to what was pending. */
  const confirmPending = () => {
    const p = pending;
    setPending(null);
    if (!p) return;
    switch (p.kind) {
      case "discard":
        return void discard(p.paths);
      case "delete":
        return void remove(p.paths);
      case "amend":
        return void ops.amend(scope, wid, message);
      default:
        // A stash entry's question is the Stashes view's to answer.
        return;
    }
  };

  /**
   * The last failure stays on screen until a write lands — a toast is gone in
   * seconds — with a retry rebuilt from what it remembers, since the panel
   * that met it may not be the one showing it.
   */
  const retryOf = (f: GitFailure): (() => void) => {
    switch (f.kind) {
      case "stage":
      case "unstage":
        return () => void run(f.kind === "stage" ? "stage" : "unstage", [...f.paths]);
      case "discard":
        return () => setPending({ kind: "discard", paths: [...f.paths] });
      case "delete":
        return () => void askDelete([...f.paths], null);
      case "commit":
        return () => void commit();
      case "amend":
        return () => setPending({ kind: "amend" });
      case "suggest":
        return () => void suggest();
      default:
        return () => listing.reload();
    }
  };

  const blocked = commitBlockedReason({ message, groups, exists: status.exists, git: status.git, identity: identity.data ?? null, head: status.head ?? null, amend });
  const nobody = identityBlockedReason(identity.data ?? null) !== null;
  const staged = groups.staged.length;

  if (listing.loading && !listingData) {
    return (
      <div className="mt-2 flex flex-col gap-2" aria-busy>
        <Skeleton className="h-4 w-40" />
        <SkeletonRows rows={4} />
      </div>
    );
  }
  if (listing.error && !listingData) {
    return <ErrorNote error={listing.error} retry={listing.reload} />;
  }

  const summary = fileSummary(groups);
  const who = identity.data;
  const whoWords = who && identityState(who) !== "missing" ? tr("work-git-panel-as-who-with-profile", { name: who.name ?? "?", email: who.email ?? "?", profile: who.profile, flag: who.profile ? "yes" : "no" }) : null;
  const amendCopy = pending?.kind === "amend" ? amendWords({ short: headShort, subject: headView.data?.commit.subject ?? null, staged: groups.staged.length, upstream: status.upstream ?? null, ahead: status.ahead }) : null;
  const pendingCopy =
    pending === null ? null : pending.kind === "discard" ? discardCopy(pending.paths, { under: pending.under }) : pending.kind === "delete" ? deleteCopy(pending.paths, disposal, { under: pending.under, all: pending.all }) : amendCopy;

  return (
    <div className="mt-2 flex min-w-0 flex-col gap-3">
      {/* Where the branch stands against origin, and the verbs that move it. A
          pull that stops on a conflict says so here and points at the list. */}
      <SyncBar
        pid={pid}
        wid={wid}
        goal={goal}
        status={status}
        defaultBranch={defaultBranch}
        onChanged={() => {
          listing.reload();
          onChanged?.();
        }}
        onRefresh={listing.reload}
        onOpenAbout={onOpenAbout}
        onSelectPath={(path) => {
          select({ path, staged: false });
          setCursor(fileId(path));
          setRevealNonce((n) => n + 1);
        }}
      />

      {/* One line: how much changed, and who signs the commit — the one place
          the author is said, with its door. */}
      <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-2xs text-text-dim">
        <span>{summary ?? tr("work-git-panel-nothing-changed")}</span>
        {whoWords && (
          <>
            <span aria-hidden>·</span>
            <span className="min-w-0 truncate">{whoWords}</span>
            {onOpenAbout && (
              <Button size="sm" variant="ghost" className="h-5 px-1 text-2xs" onClick={() => onOpenAbout("checkout")}>{tr("work-git-panel-change")}</Button>
            )}
          </>
        )}
      </div>
      {nobody && (
        <ReasonLine tone="warn" door={onOpenAbout ? { label: tr("work-git-panel-set-who-commits-here"), onClick: () => onOpenAbout("checkout") } : null}>
          {identityBlockedReason(identity.data ?? null, tr("work-git-panel-under-place", { place: PLACE.checkout }))}
        </ReasonLine>
      )}

      {failure && <ErrorNote error={failure.error} retry={retryOf(failure)} />}

      {isClean(groups) ? (
        <EmptyState
          title={tr("work-git-panel-nothing-commit")}
          hint={tr("work-git-panel-working-tree-matches-empty-history-head", { flag: (unborn) ? "yes" : "no" })}
          className="py-4"
          action={
            <Button size="sm" variant="ghost" onClick={listing.reload}>
              <ICON.refresh size={12} aria-hidden />{tr("work-commit-graph-refresh")}</Button>
          }
        />
      ) : (
        <div className="flex flex-col gap-1">
          <ChangesToolbar
            layout={layout}
            onLayout={(l) => setPanelView("changes", l)}
            filter={filter}
            counts={counts}
            onFilter={(f) => setPanelView("changesFilter", f)}
            canCollapse={foldable.some((id) => !folds.includes(id))}
            onCollapseAll={collapseAll}
            scopes={scopes}
            busy={busy !== null}
            onStage={(paths) => void run("stage", paths)}
            onUnstage={(paths) => void run("unstage", paths)}
            onDiscard={(paths) => setPending({ kind: "discard", paths })}
            onDeleteUntracked={(paths) => void askDelete(paths, null, true)}
          />
          {rows.length === 0 && shown.length === 0 && (
            <EmptyState
              title={emptyWords(filter)}
              hint={tr("work-git-panel-changed-file-files-hidden-filter", { files: files.length })}
              className="py-3"
              action={
                <Button size="sm" variant="ghost" onClick={() => setPanelView("changesFilter", "all")}>
                  <ICON.filter size={12} aria-hidden />{tr("work-git-panel-show-all")}</Button>
              }
            />
          )}
          {rows.length > 0 && (
          <ChangesTree
            rows={rows}
            layout={layout}
            cursor={cursor}
            onCursor={setCursor}
            selection={selection}
            busy={busy !== null}
            askingDelete={askingDelete}
            onFold={fold}
            onSelect={(row, side) => {
              const sel = selectionOf(row.row, side);
              if (sel) select(sel);
            }}
            onStage={(paths) => void run("stage", paths)}
            onUnstage={(paths) => void run("unstage", paths)}
            onDiscard={(paths, under) => setPending({ kind: "discard", paths, under })}
            onDelete={(paths, under) => void askDelete(paths, under)}
            menuFor={menuFor}
            folderMenuFor={folderMenuFor}
            revealNonce={revealNonce}
          />
          )}
        </div>
      )}

      {/* The composer, pinned at the bottom while the files and the patch
          scroll: the message, and the one primary verb — a commit, or the
          amend when the switch beside it is on (ide/04 §Amend) — with Suggest
          beside it, and the one reason it is off under it. Stashing is the
          Stashes view's (ide/04 §Stash). The review notes come after it, the
          view's last section. */}
      <div className="sticky bottom-0 -mx-2 flex flex-col gap-1.5 border-t border-border bg-bg px-2 pt-2 pb-1">
        <TextArea
          value={message}
          rows={2}
          placeholder={amend ? amendPlaceholder(headShort, headView.data?.commit.subject ?? null) : tr("work-git-panel-commit-message-file-files-staged", { staged })}
          aria-label={amend ? tr("work-git-panel-amended-commit-message") : tr("work-git-panel-commit-message")}
          onChange={(e) => setMessage(scope, e.target.value)}
          onKeyDown={(e) => {
            if ((e.metaKey || e.ctrlKey) && e.key === "Enter" && blocked === null && busy === null) record();
          }}
        />
        {note && (
          <p className="text-2xs text-text-dim" role="status">
            {note}
          </p>
        )}
        <div className="flex items-center gap-1.5">
          <Tooltip label={tr("work-git-panel-draft-message-from-what-staged-read")}>
            <span className="inline-flex">
              <Button size="sm" variant="ghost" disabled={busy !== null || staged === 0} onClick={() => void suggest()}>
                <ICON.agent size={12} aria-hidden />
                {busy === "suggest" ? tr("work-agent-review-request-asking") : tr("work-git-panel-suggest")}
              </Button>
            </span>
          </Tooltip>
          {(busy === "suggest" || busy === "commit" || busy === "amend") && <WorkingDot title={busy === "commit" ? tr("work-git-panel-committing") : busy === "amend" ? tr("work-git-panel-amending") : tr("work-git-panel-asking")} />}
          <span className="flex-1" />
          {/* The switch beside the verb: on, the box holds the last commit's
              message and the button rewrites that commit — asked first, and
              pinned in Safety by the node. */}
          <Tooltip label={status.head ? tr("work-git-panel-fold-what-staged-into-last-commit") : tr("work-git-panel-no-commit-amend-yet")}>
            <span className="inline-flex">
              <Switch checked={amend} onChange={setAmend} label={VERB.amend} disabled={busy !== null || !status.head} />
            </span>
          </Tooltip>
          <KeyHint combo="Mod+Enter" />
          <Button size="sm" variant="primary" disabled={blocked !== null || busy !== null} onClick={record}>
            {busy === "commit" ? tr("work-git-panel-committing-2") : busy === "amend" ? tr("work-git-panel-amending-2") : amend ? VERB.amend : VERB.commit}
          </Button>
        </div>
        {blocked && !nobody && <ReasonLine>{blocked}</ReasonLine>}
        {unborn && <ReasonLine>{tr("work-git-panel-no-commits-yet-first-one-what")}</ReasonLine>}
      </div>

      <ReviewNotes pid={pid} wid={wid} refreshKey={session.stale} onOpen={(path, isStaged) => select({ path, staged: isStaged })} />

      {/* One dialog for the throw-aways that ask first; the words are
          `gitDiscardModel.mjs`'s, the promise is the one `SafetyNote`. */}
      <ConfirmDialog
        open={pendingCopy !== null}
        onClose={() => setPending(null)}
        onConfirm={confirmPending}
        title={pendingCopy?.title ?? ""}
        body={
          <div className="flex flex-col gap-2">
            <p>{pendingCopy?.body ?? ""}</p>
            {amendCopy?.warning && (
              <p className="text-danger" role="alert">
                {amendCopy.warning}
              </p>
            )}
            {pending?.kind === "discard" && <SafetyNote kind="tree" />}
            {pending?.kind === "amend" && <SafetyNote kind="commit" />}
          </div>
        }
        confirmLabel={pendingCopy?.confirm ?? tr("work-git-panel-confirm")}
        danger={pendingCopy?.danger ?? true}
      />
    </div>
  );
}
