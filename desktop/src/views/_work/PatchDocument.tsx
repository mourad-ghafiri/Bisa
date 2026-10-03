/**
 * One changed file's patch, on one side, as a document in the centre
 * (ide/04 §The Changes view): the Changes list *points* at a file, this is
 * where its patch is read and acted on — in one of three views
 * (`patchViewModel.mjs`): **Hunks**, hunk by hunk with the three acts
 * (`HunkDiff`: stage or unstage a hunk, pick lines, annotate); **Side by
 * side** and **Inline**, the two whole texts compared in the code editor,
 * read-only, unchanged regions folded (`Comparison`, shared with the
 * commit document) — or the conflict view when the path is unmerged, at the
 * width a patch wants. The view is one choice for every patch — a commit's
 * file reads with it too — remembered on this machine (`usePanelView("patch")`).
 *
 * The header: the path, the side's word (`sideWords`), `+n −m`, the *View*
 * control, *History* (the commits that touched the file — a row opens a
 * commit document) and *Open the file*. The patch is `GET /git/diff` for
 * the path and side, the comparison `GET /git/sides` — the index against
 * the tree, or HEAD against the index — both re-read whenever a write
 * landed in the checkout (the session's `stale`);
 * a hunk staged here lands its rows through the one store helper the panel
 * uses (`filesLanded`), so the list beside and this document agree. The
 * review notes are filed under the project, read from the workstream once.
 *
 * An untracked file has no patch git can give without staging it, and a GET
 * does not stage: the document reads the file itself (`ide/file`) and draws
 * it as the patch git would write for it (`newFilePatch`), read-only — it is
 * staged whole from the list. The document fills its column by flex: the
 * scrollport is a flex column, the patch its `min-h-0 flex-1` content, no
 * hunk its own scrollport (ide/03 §The Files occupant's rule).
 */

import { useState } from "react";
import { api } from "../../api";
import type { FileSides, IdeFile, WorkstreamFileDiff } from "../../types";
import { Button, EmptyState, ErrorNote, ICON, SegmentedControl, SkeletonRows, Tooltip, fileIcon } from "../../ui";
import { setPanelView, usePanelView } from "../_workbench/rightPanelStore";
import { rootKey } from "../_workbench/workbenchModel.mjs";
import { Comparison } from "./Comparison";
import { ConflictView } from "./ConflictView";
import { FileHistory } from "./FileHistory";
import { diffStat } from "./gitFiles.mjs";
import { filesLanded, useGitSession } from "./gitPanelStore";
import { sideWords } from "./gitWords.mjs";
import { HunkDiff } from "./HunkDiff";
import { newFilePatch, newFileWords } from "./newFilePatch.mjs";
import { conflictedPaths, nextConflict } from "./operationModel.mjs";
import { PATCH_VIEWS, PATCH_VIEW_LABEL, needsSides, patchViewGlyph, patchViewHint } from "./patchViewModel.mjs";
import type { PatchView } from "./patchViewModel.mjs";
import { ReadOnlyDiff } from "./ReadOnlyDiff";
import { useAsync } from "./useAsync";
import { t } from "../../i18n/l10n.mjs";

export function PatchDocument({
  wid,
  path,
  staged,
  onOpenFile,
  onOpenPatch,
  onOpenCommit,
}: {
  wid: string;
  path: string;
  /** The index against HEAD, rather than the working tree against the index. */
  staged: boolean;
  /** Open the file itself as a document. */
  onOpenFile: (path: string) => void;
  /** A conflict settled: the next unmerged file's patch takes this one's place. */
  onOpenPatch: (path: string, staged: boolean) => void;
  /** A commit from the file's history. */
  onOpenCommit: (sha: string) => void;
}) {
  const scope = rootKey("workstream", wid);
  const session = useGitSession(scope);
  const detail = useAsync((s) => api.workstream(wid, s), [wid]);
  const status = useAsync((s) => api.workstreamGitStatus(wid, s), [wid, session.stale]);
  // The operation's facts — the sides' names — read only while one is half-done.
  const inProgress = status.data?.status.in_progress ?? null;
  const facts = useAsync((s) => (inProgress ? api.gitOperation(wid, s) : Promise.resolve(null)), [wid, inProgress, session.stale]);
  const diff = useAsync<WorkstreamFileDiff>((s) => api.gitDiff(wid, path, staged, s), [wid, path, staged, session.stale]);
  const [showHistory, setShowHistory] = useState(false);
  const patch = diff.data ?? null;
  const conflicted = patch?.file?.conflicted ?? false;
  // The view is one choice for every patch; a comparison reads the two whole texts.
  const view = usePanelView("patch");
  const comparing = needsSides(view) && !conflicted;
  const sides = useAsync<FileSides | null>((s) => (comparing ? api.gitSides(wid, path, staged, s) : Promise.resolve(null)), [wid, path, staged, comparing, session.stale]);
  // A file git has never seen: read it, and draw it as the patch git would write.
  const untracked = patch?.untracked === true;
  const content = useAsync<IdeFile | null>((s) => (untracked ? api.ideFile("workstream", wid, path, s) : Promise.resolve(null)), [wid, path, untracked, session.stale]);
  const newDiff = untracked && content.data?.text != null ? newFilePatch(path, content.data.text) : null;
  const stat = patch ? diffStat(newDiff ?? patch.diff) : null;
  const pid = detail.data?.workstream.project ?? "";
  const busy = session.busy !== null;
  const name = path.split("/").pop() ?? path;
  const Glyph = fileIcon(name);

  /** A conflict settled: on to the next one, so a five-file merge is settled without leaving the centre. */
  const moveOn = (conflictedNow: string[], settled: string) => {
    const next = nextConflict(conflictedNow, settled);
    if (next !== null) onOpenPatch(next, false);
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <header className="flex shrink-0 flex-wrap items-center gap-2 border-b border-hairline px-4 py-2 text-2xs">
        <Glyph size={14} aria-hidden className="shrink-0 text-text-dim" />
        <span className="min-w-0 truncate font-mono text-text" title={path}>
          {path}
        </span>
        <span className="text-text-dim">{sideWords(staged)}</span>
        {stat && (stat.insertions > 0 || stat.deletions > 0) && (
          <span className="tnum">
            <span className="text-ok">+{stat.insertions}</span> <span className="text-danger">−{stat.deletions}</span>
          </span>
        )}
        <span className="flex-1" />
        {!conflicted && (
          <SegmentedControl
            label={t("work-commit-document-view")}
            size="sm"
            iconOnly
            value={view}
            onChange={(v) => setPanelView("patch", v as PatchView)}
            options={PATCH_VIEWS.map((v) => ({ id: v, label: PATCH_VIEW_LABEL[v], icon: ICON[patchViewGlyph(v)], hint: patchViewHint(v) }))}
          />
        )}
        <Tooltip label={t("work-patch-document-commits-touched-file")}>
          <span className="inline-flex">
            <Button size="sm" variant="ghost" className="h-6 aria-pressed:bg-selected aria-pressed:text-text" aria-pressed={showHistory} onClick={() => setShowHistory((v) => !v)}>
              <ICON.history size={12} aria-hidden />{t("work-patch-document-history")}</Button>
          </span>
        </Tooltip>
        <Tooltip label={t("work-patch-document-open-document", { name })}>
          <span className="inline-flex">
            <Button size="sm" variant="ghost" className="h-6" onClick={() => onOpenFile(path)}>
              <ICON.file size={12} aria-hidden />{t("work-patch-document-open-file")}</Button>
          </span>
        </Tooltip>
      </header>
      <div className="flex min-h-0 flex-1 flex-col overflow-auto">
        <div className="flex min-h-0 flex-1 flex-col gap-3 p-4">
          {showHistory && <FileHistory wid={wid} path={path} onOpenCommit={onOpenCommit} />}
          {conflicted ? (
            <ConflictView
              wid={wid}
              pid={pid || null}
              path={path}
              inProgress={inProgress}
              facts={facts.data ?? null}
              onResolved={(next) => {
                filesLanded(scope, next);
                moveOn(conflictedPaths(next), path);
              }}
              onSettled={(settled) => moveOn(conflictedPaths(session.files ?? []), settled)}
            />
          ) : diff.loading && !patch ? (
            <SkeletonRows rows={6} />
          ) : diff.error ? (
            <ErrorNote error={diff.error} retry={diff.reload} />
          ) : comparing ? (
            sides.loading && !sides.data ? (
              <SkeletonRows rows={6} />
            ) : sides.error ? (
              <ErrorNote error={sides.error} retry={sides.reload} />
            ) : sides.data ? (
              <Comparison path={path} sides={sides.data} inline={view === "inline"} />
            ) : null
          ) : untracked ? (
            content.loading && !content.data ? (
              <SkeletonRows rows={6} />
            ) : content.error ? (
              <ErrorNote error={content.error} retry={content.reload} />
            ) : (
              <NewFile file={content.data} diff={newDiff} />
            )
          ) : patch && patch.diff.trim() ? (
            <HunkDiff pid={pid} wid={wid} path={path} staged={staged} diff={patch.diff} busy={busy} onApplied={(next) => filesLanded(scope, next)} onNoted={() => undefined} />
          ) : (
            <EmptyState title={t("work-patch-document-no-patch-side")} hint={t("work-patch-document-change-other-side-file-select-there")} className="py-6" action={null} />
          )}
        </div>
      </div>
    </div>
  );
}

/**
 * A file git has never seen: the sentence that says so, then the file as
 * the all-additions patch git would write for it, read-only — a binary file
 * says its size instead, a file cut at the read cap says what is shown.
 */
function NewFile({ file, diff }: { file: IdeFile | null; diff: string | null }) {
  const words = newFileWords(file);
  return (
    <div className="flex min-h-0 flex-col gap-2">
      <p className="text-2xs text-text-dim" role="status">
        {words.sentence}
        {words.cut && ` ${t("work-patch-document-first-2-mib-shown")}`}
      </p>
      {!words.binary && diff !== null && <ReadOnlyDiff diff={diff} />}
    </div>
  );
}
