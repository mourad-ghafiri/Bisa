/**
 * One commit as a document in the centre (ide/05): the header — its short
 * id (copyable), its refs as chips, who and when, *merge of n* — and the
 * actions the toolbar draws (cherry-pick, revert, checkout, branch, tag,
 * attach), with the **View** control every patch shares; then the subject
 * and the body; the files it changed against its first parent with `+n −m`
 * as a **selector**; and what is read under them (`commitFocusModel.mjs`):
 * on *Hunks* the whole patch, read-only, until a file is chosen and that
 * file's patch alone after; on *Side by side* and *Inline* one file's two
 * versions — the first parent's, at the file's old path for a rename,
 * against the commit's — compared in the code editor (`Comparison`). The
 * History view *points* at a commit; this is where it is read. The view is
 * one choice for every patch, remembered on this machine
 * (`usePanelView("patch")`), so a commit's file reads the way a changed
 * file does.
 *
 * The actions run through the same hook the graph's rows use
 * (`useCommitActions`) against the checkout's session, with the same
 * dialogs, so a cherry-pick from here and one from the row are one act. The
 * action context is the commit's own: it is HEAD when it wears the `head`
 * ref, and the branch HEAD is on is then one of its refs.
 */

import { useState } from "react";
import { api } from "../../api";
import type { CommitFileDiff, CommitFileSides, WorkstreamCommitView } from "../../types";
import { EmptyState, ErrorNote, ICON, RelativeTime, SegmentedControl, Skeleton, SkeletonRows, Tooltip, absolute, cn } from "../../ui";
import { setPanelView, usePanelView } from "../_workbench/rightPanelStore";
import { rootKey } from "../_workbench/workbenchModel.mjs";
import { ActionButton, RefChip } from "./CommitActionControls";
import { CommitActionDialogs } from "./CommitActionDialogs";
import { commitActions, headOf } from "./commitActionsModel.mjs";
import { chooseFile, cutWords, emptyPatchWords, focusOf, isFocused, listHint } from "./commitFocusModel.mjs";
import type { CommitFocus } from "./commitFocusModel.mjs";
import { Comparison } from "./Comparison";
import { useGitSession } from "./gitPanelStore";
import { PATCH_VIEWS, PATCH_VIEW_LABEL, needsSides, patchViewGlyph, patchViewHint } from "./patchViewModel.mjs";
import type { PatchView } from "./patchViewModel.mjs";
import { ReadOnlyDiff } from "./ReadOnlyDiff";
import { useAsync } from "./useAsync";
import { useCommitActions } from "./useCommitActions";
import type { ActionTarget } from "./useCommitActions";
import { t } from "../../i18n/l10n.mjs";

export function CommitDocument({ wid, sha }: { wid: string; sha: string }) {
  const scope = rootKey("workstream", wid);
  const session = useGitSession(scope);
  // Re-read when a write landed: a cherry-pick or a checkout moves the refs this commit wears.
  const detail = useAsync<WorkstreamCommitView>((s) => api.gitShowCommit(wid, sha, s), [wid, sha, session.stale]);
  const status = useAsync((s) => api.workstreamGitStatus(wid, s), [wid, session.stale]);
  const inProgress = status.data?.status.in_progress ?? null;
  const commit = detail.data?.commit ?? null;
  const headId = commit ? headOf([commit]) : null;
  const currentBranch = headId && commit ? (commit.refs.find((r) => r.kind === "branch")?.name ?? null) : null;
  const actions = useCommitActions(wid, { current: currentBranch });

  // What is read under the file list: the view is one choice for every patch; the file is this document's.
  const view = usePanelView("patch");
  const [chosen, setChosen] = useState<string | null>(null);
  const focus = focusOf(commit?.files ?? [], chosen, view);
  const focused = focus?.kind === "file" ? focus.path : null;
  const comparing = needsSides(view);
  const fileDiff = useAsync<CommitFileDiff | null>((s) => (focused !== null && !comparing ? api.gitCommitFileDiff(wid, sha, focused, s) : Promise.resolve(null)), [wid, sha, focused, comparing, session.stale]);
  const sides = useAsync<CommitFileSides | null>((s) => (focused !== null && comparing ? api.gitCommitFileSides(wid, sha, focused, s) : Promise.resolve(null)), [wid, sha, focused, comparing, session.stale]);

  if (detail.error && !detail.data) {
    return (
      <div className="p-4">
        <ErrorNote error={detail.error} retry={detail.reload} />
      </div>
    );
  }
  if (!detail.data || !commit) {
    return (
      <div className="flex flex-col gap-3 p-4" aria-busy>
        <Skeleton className="h-5 w-72" />
        <Skeleton className="h-4 w-1/2" />
        <Skeleton className="h-40 w-full" />
      </div>
    );
  }
  const d = detail.data;
  const target: ActionTarget = { id: commit.id, short: commit.short, subject: commit.subject, parents: commit.parents.length };
  const ctx = { busy: actions.busy, headId, inProgress, currentBranch };
  // The toolbar draws the actions a toolbar is for — not *inspect* (this is
  // it) and not the clipboard ones, which the short id below is.
  const toolbar = commitActions(target, ctx).filter((a) => !["inspect", "copy_sha", "copy_short"].includes(a.id));

  return (
    <div className="flex h-full min-h-0 flex-col">
      <header className="flex shrink-0 flex-wrap items-center gap-2 border-b border-hairline px-4 py-2 text-2xs">
        <ICON.checkout size={14} aria-hidden className="shrink-0 text-text-dim" />
        <Tooltip label={t("work-commit-document-copy-full-id")}>
          <span className="inline-flex">
            <button type="button" className="anim font-mono font-medium text-text hover:underline" onClick={() => actions.start("copy_sha", target)}>
              {commit.short}
            </button>
          </span>
        </Tooltip>
        {commit.refs.map((r) => (
          <RefChip key={`${r.kind}:${r.name}`} commit={target} refName={r} actions={actions} ctx={ctx} />
        ))}
        <span className="text-text-dim" title={commit.email}>
          {commit.author}
        </span>
        <span className="text-text-dim" title={absolute(commit.timestamp)}>
          <RelativeTime at={commit.timestamp} />
        </span>
        {commit.parents.length > 1 && <span className="text-text-dim">{t("work-commit-document-merge", { parents: commit.parents.length })}</span>}
        <span className="flex-1" />
        <SegmentedControl
          label={t("work-commit-document-view")}
          size="sm"
          iconOnly
          value={view}
          onChange={(v) => setPanelView("patch", v as PatchView)}
          options={PATCH_VIEWS.map((v) => ({ id: v, label: PATCH_VIEW_LABEL[v], icon: ICON[patchViewGlyph(v)], hint: patchViewHint(v, { readOnly: true }) }))}
        />
        {toolbar.map((a) => (
          <ActionButton key={a.id} action={a} target={target} actions={actions} withLabel />
        ))}
      </header>
      <div className="flex min-h-0 flex-1 flex-col overflow-auto">
        <div className="flex min-h-0 flex-1 flex-col gap-3 p-4">
          <p className="text-lg font-semibold tracking-tight text-text">{commit.subject}</p>
          {commit.body && <pre className="whitespace-pre-wrap font-sans text-xs leading-relaxed text-text-dim">{commit.body}</pre>}
          {commit.files.length > 0 && (
            <div className="flex flex-col gap-1">
              <p className="text-2xs text-text-dim">
                {t("work-commit-document-files-changed-count-hint", { n: commit.files.length, hint: listHint(view) })}
              </p>
              <ul className="flex flex-col rounded-control border border-border text-2xs" role="listbox" aria-label={t("work-commit-document-files-changed")}>
                {commit.files.map((f) => {
                  const selected = isFocused(focus, f.path);
                  return (
                    <li key={f.path} role="option" aria-selected={selected} className="border-b border-hairline last:border-b-0">
                      <button
                        type="button"
                        className={cn("anim flex w-full items-baseline gap-2 px-2 py-1 text-left font-mono hover:bg-surface-2", selected && "bg-selected")}
                        onClick={() => setChosen(chooseFile(chosen, f.path, view))}
                      >
                        <span className="w-3 shrink-0 text-text-dim" title={f.kind}>
                          {f.kind.charAt(0).toUpperCase()}
                        </span>
                        <span className="min-w-0 flex-1 truncate text-text">{f.old_path ? `${f.old_path} → ${f.path}` : f.path}</span>
                        {f.binary ? (
                          <span className="text-text-dim">{t("work-commit-document-binary")}</span>
                        ) : (
                          <span className="tnum shrink-0">
                            <span className="text-ok">+{f.insertions}</span> <span className="text-danger">−{f.deletions}</span>
                          </span>
                        )}
                      </button>
                    </li>
                  );
                })}
              </ul>
            </div>
          )}
          {focus === null ? (
            <EmptyState title={t("work-commit-document-nothing-compare")} hint={t("work-commit-document-commit-changed-no-file")} className="py-6" action={null} />
          ) : focus.kind === "all" ? (
            <WholePatch view={d} focus={focus} />
          ) : comparing ? (
            sides.loading && !sides.data ? (
              <SkeletonRows rows={6} />
            ) : sides.error ? (
              <ErrorNote error={sides.error} retry={sides.reload} />
            ) : sides.data ? (
              <Comparison path={focus.path} sides={sides.data} inline={view === "inline"} />
            ) : null
          ) : fileDiff.loading && !fileDiff.data ? (
            <SkeletonRows rows={6} />
          ) : fileDiff.error ? (
            <ErrorNote error={fileDiff.error} retry={fileDiff.reload} />
          ) : fileDiff.data ? (
            <FilePatch patch={fileDiff.data} focus={focus} />
          ) : null}
        </div>
      </div>
      <CommitActionDialogs actions={actions} />
    </div>
  );
}

/** The whole commit's patch, read-only, under the cut's sentence when it was cut. */
function WholePatch({ view, focus }: { view: WorkstreamCommitView; focus: CommitFocus }) {
  return (
    <>
      {view.truncated && <p className="text-2xs text-warn">{cutWords()}</p>}
      {view.diff.trim() ? <ReadOnlyDiff diff={view.diff} /> : <p className="text-2xs text-text-dim">{emptyPatchWords(focus)}</p>}
    </>
  );
}

/** One file's patch in the commit, read-only — a moved file without changes says so. */
function FilePatch({ patch, focus }: { patch: CommitFileDiff; focus: CommitFocus }) {
  return (
    <>
      {patch.truncated && <p className="text-2xs text-warn">{t("work-commit-document-patch-cut-2-mib")}</p>}
      {patch.diff.trim() ? <ReadOnlyDiff diff={patch.diff} /> : <p className="text-2xs text-text-dim">{emptyPatchWords(focus)}</p>}
    </>
  );
}
