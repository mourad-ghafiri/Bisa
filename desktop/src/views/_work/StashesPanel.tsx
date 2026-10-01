/**
 * The Git tab's Stashes view (ide/04 §Stash): work parked for later, as a
 * view of its own beside Changes · Branches · History — a person reaches for
 * a stash on purpose, and a list nobody sees is a list nobody uses.
 *
 * The list is the **repository's**: `refs/stash` is shared by every
 * worktree of a project, so a stash made in another workstream is here too,
 * and *on* says where each was made. Every verb is the consented tier's,
 * recovery first: *Apply* keeps the entry, *Pop* and *Drop* pin it in Safety
 * before anything, and both ask first. *Stash changes…* in the header parks
 * the working tree of **this** checkout through the same dialog the Changes
 * view's *Stash…* opens; the one reason it is off is a line under it.
 *
 * A row shows the entry and reveals its verbs on hover — the patch behind a
 * chevron, *Apply* · *Pop* · *Drop* and a `⋮` with the same three — and the
 * state — what is busy, which patch is shown, the pending question — lives in
 * the checkout's session (`gitPanelStore`), so a tab switch loses nothing.
 */

import { useEffect, useRef } from "react";
import { api } from "../../api";
import type { GitStash, GitStatusInfo } from "../../types";
import { Button, Chip, ConfirmDialog, EmptyState, ErrorNote, ICON, Menu, RelativeTime, SectionHeader, SkeletonRows, Tooltip, WorkingDot } from "../../ui";
import type { MenuItem } from "../../ui";
import { GIT_STASH, onDoor } from "../../shell/shortcuts";
import { rootKey } from "../_workbench/workbenchModel.mjs";
import * as ops from "./gitOps";
import { readsFor } from "./gitChangeModel.mjs";
import { patchSession, useGitSession } from "./gitPanelStore";
import { useGitChanges } from "./useGitChanges";
import { VERB } from "./gitWords.mjs";
import { ReadOnlyDiff } from "./ReadOnlyDiff";
import { ReasonLine } from "./ReasonLine";
import { SafetyNote } from "./SafetyNote";
import { StashDialog } from "./StashDialog";
import { canStash, stashConfirm, stashRow, stashablePaths } from "./stashModel.mjs";
import { useAsync } from "./useAsync";
import { t } from "../../i18n/l10n.mjs";

/** One stash entry's patch, under its row. */
function StashPatch({ wid, sha }: { wid: string; sha: string }) {
  const patch = useAsync((s) => api.gitStashDiff(wid, sha, s), [wid, sha]);
  if (patch.loading && !patch.data) return <SkeletonRows rows={3} />;
  if (patch.error) return <ErrorNote error={patch.error} retry={patch.reload} />;
  const d = patch.data;
  if (!d) return null;
  return (
    <div className="flex flex-col gap-1">
      {d.truncated && <p className="text-2xs text-warn">{t("work-commit-document-patch-cut-2-mib")}</p>}
      {d.diff.trim() ? <ReadOnlyDiff diff={d.diff} /> : <EmptyState title={t("work-stashes-panel-empty-stash")} hint={t("work-stashes-panel-nothing-differs-from-commit-made")} className="py-2" action={null} />}
    </div>
  );
}

export function StashesPanel({ wid, status, onChanged }: { wid: string; status: GitStatusInfo; onChanged?: () => void }) {
  const scope = rootKey("workstream", wid);
  const session = useGitSession(scope);
  const { busy, pending, stashing, shownStash, failure } = session;
  const stashes = useAsync((s) => api.gitStashes(wid, s), [wid]);
  const setShownStash = (sha: string | null) => patchSession(scope, { shownStash: sha });
  const setStashing = (paths: string[] | null) => patchSession(scope, { stashing: paths });
  const stashable = canStash(status);
  // The tracked changes the dialog may scope a stash to — read when it
  // opens, once, never while the list is only being shown.
  const scopable = useAsync(async (s) => (stashing === null ? null : stashablePaths((await api.gitFiles(wid, s)).files)), [wid, stashing !== null]);

  // A write landed — here or while this view was away: the list is stale.
  const seenStale = useRef(session.stale);
  useEffect(() => {
    if (seenStale.current === session.stale) return;
    seenStale.current = session.stale;
    stashes.reload();
    onChanged?.();
    // The reload is a stable callback; the nonce is what changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session.stale]);
  // The stash ref moved outside the platform — `git stash` in a terminal (ide/04 §Live).
  useGitChanges(wid, (kinds) => {
    if (readsFor(kinds).stashes) stashes.reload();
  });
  // The Omnibox's *Stash* lands on this view: the dialog for the whole tree.
  useEffect(() => {
    return onDoor(GIT_STASH, () => setStashing([]));
    // `setStashing` is a plain function remade every render; the door is the scope's.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scope]);

  const stashAct = (kind: "apply" | "pop" | "drop", entry: GitStash) => ops.stashAct(scope, wid, kind, entry);
  const ask = (kind: "stash_pop" | "stash_drop", entry: GitStash) => patchSession(scope, { pending: { kind, entry } });
  const stashPending = pending && (pending.kind === "stash_pop" || pending.kind === "stash_drop") ? pending : null;
  const copy = stashPending ? stashConfirm(stashPending.kind === "stash_pop" ? "pop" : "drop", stashPending.entry) : null;
  const confirmPending = () => {
    const p = stashPending;
    patchSession(scope, { pending: null });
    if (p) void stashAct(p.kind === "stash_pop" ? "pop" : "drop", p.entry);
  };
  const entries = stashes.data?.stashes ?? [];
  const verbsOf = (entry: GitStash): MenuItem[] => [
    { label: t("work-stashes-panel-keep-entry", { apply: VERB.apply }), icon: ICON.discard, disabled: busy !== null, onSelect: () => void stashAct("apply", entry) },
    { label: t("work-stashes-panel-apply-drop", { VERB: VERB.pop }), icon: ICON.stash, disabled: busy !== null, onSelect: () => ask("stash_pop", entry) },
    { label: `${VERB.drop}…`, icon: ICON.delete, danger: true, disabled: busy !== null, separatorBefore: true, onSelect: () => ask("stash_drop", entry) },
  ];

  return (
    <div className="mt-2 flex min-w-0 flex-col gap-2">
      <SectionHeader
        title={t("work-stashes-panel-stashes")}
        count={entries.length}
        showZero
        trailing={busy === "stash" ? <WorkingDot title={t("work-stashes-panel-stashing")} /> : undefined}
        alwaysAction
        action={
          <Button size="sm" variant="ghost" disabled={!stashable.ok || busy !== null} onClick={() => setStashing([])}>
            <ICON.stash size={12} aria-hidden />{t("work-stashes-panel-changes", { stash: VERB.stash })}</Button>
        }
      />
      {!stashable.ok && stashable.reason && <ReasonLine>{stashable.reason}</ReasonLine>}
      {failure && (failure.kind === "stash" || failure.kind.startsWith("stash_")) && <ErrorNote error={failure.error} retry={stashes.reload} />}
      {stashes.error && <ErrorNote error={stashes.error} retry={stashes.reload} />}
      {stashes.loading && !stashes.data ? (
        <SkeletonRows rows={3} />
      ) : (
        <ul className="flex flex-col">
          {entries.map((entry) => {
            const row = stashRow(entry);
            const shown = shownStash === entry.commit;
            return (
              <li key={entry.commit} className="flex flex-col">
                <div className="group flex h-row-sm items-center gap-2 px-1 text-2xs">
                  <Tooltip label={shown ? t("work-stashes-panel-hide-patch") : t("work-stashes-panel-show-patch")}>
                    <button type="button" className="anim inline-flex h-5 w-5 shrink-0 items-center justify-center rounded text-text-dim hover:text-text" aria-expanded={shown} aria-label={t("work-stashes-panel-hide-show-patch", { flag: (shown) ? "yes" : "no", row: row.id })} onClick={() => setShownStash(shown ? null : entry.commit)}>
                      <ICON.collapsed size={11} aria-hidden className={`anim ${shown ? "rotate-90" : ""}`} />
                    </button>
                  </Tooltip>
                  <span className="shrink-0 font-mono text-text">{row.id}</span>
                  <span className="min-w-0 truncate text-text" title={entry.subject}>
                    {row.title}
                  </span>
                  <span className="shrink-0 font-mono text-text-dim">{row.where}</span>
                  {row.untracked && <Chip tone="quiet">{t("work-stashes-panel-untracked")}</Chip>}
                  <RelativeTime at={row.at} className="shrink-0 text-text-dim" />
                  <span className="flex-1" />
                  <span className="row-actions anim flex items-center gap-0.5">
                    <Tooltip label={t("work-stashes-panel-onto-working-tree-entry-stays")}>
                      <span className="inline-flex">
                        <Button size="sm" variant="ghost" className="h-6" disabled={busy !== null} onClick={() => void stashAct("apply", entry)}>
                          {VERB.apply}
                        </Button>
                      </span>
                    </Tooltip>
                    <Tooltip label={t("work-stashes-panel-apply-then-drop-entry-asked-first")}>
                      <span className="inline-flex">
                        <Button size="sm" variant="ghost" className="h-6" disabled={busy !== null} onClick={() => ask("stash_pop", entry)}>
                          {VERB.pop}
                        </Button>
                      </span>
                    </Tooltip>
                    <Menu
                      label={t("work-stashes-panel-more", { row: row.id })}
                      items={verbsOf(entry)}
                      trigger={
                        <span className="anim flex h-6 w-6 items-center justify-center rounded text-text-dim hover:bg-surface hover:text-text">
                          <ICON.more size={13} aria-hidden />
                        </span>
                      }
                    />
                  </span>
                </div>
                {shown && (
                  <div className="ml-7 mb-2 flex min-w-0 flex-col gap-1">
                    <SafetyNote kind="tree" />
                    <StashPatch wid={wid} sha={entry.commit} />
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      )}
      {stashes.data && entries.length === 0 && (
        <EmptyState
          icon={ICON.stash}
          title={t("work-stashes-panel-nothing-stashed")}
          hint={t("work-stashes-panel-stash-parks-working-tree-s-changes")}
          className="py-4"
          action={
            <Button size="sm" disabled={!stashable.ok || busy !== null} onClick={() => setStashing([])}>
              <ICON.stash size={12} aria-hidden />{t("work-stashes-panel-changes", { stash: VERB.stash })}</Button>
          }
        />
      )}

      <StashDialog open={stashing !== null} files={scopable.data ?? null} status={status} onClose={() => setStashing(null)} submit={(body) => ops.stashPush(scope, wid, body)} />
      <ConfirmDialog
        open={stashPending !== null}
        onClose={() => patchSession(scope, { pending: null })}
        onConfirm={confirmPending}
        title={copy?.title ?? ""}
        body={
          <div className="flex flex-col gap-2">
            <p>{copy?.body ?? ""}</p>
            <SafetyNote kind="stash" />
          </div>
        }
        confirmLabel={copy?.confirm ?? t("work-git-panel-confirm")}
        danger={copy?.danger ?? true}
      />
    </div>
  );
}
