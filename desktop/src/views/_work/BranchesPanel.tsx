/**
 * Branches, tags, remotes — and **Safety**, the recovery points every
 * consented operation wrote before it ran (ide/04 §The Branches view).
 *
 * Four sections, every one open with its count. A branch row shows where it
 * stands — `↑2 ↓1` against its upstream, *merged* when the default branch
 * has it — one verb on hover, *Switch*, and the rest behind its `⋮`
 * (`branchActionsModel.mjs`): merge into the current branch, rebase the
 * current onto it, cherry-pick from it, its upstream, a workstream, rename,
 * delete; the current branch offers an interactive rebase and a push with
 * lease. A filter narrows a long list. Every act with options has one
 * dialog of its own, the verb as its button and the `SafetyNote` said once;
 * every act runs through `gitOps` against the checkout's session, so a tab
 * switch loses nothing and nothing runs while something else does. What
 * git has left half-done is the Resolve card's above — every consented
 * act here is off with that one sentence until it is settled or aborted.
 */

import { useMemo, useState } from "react";
import { rootKey } from "../_workbench/workbenchModel.mjs";
import { gitPlace } from "../_workbench/idePlacesModel.mjs";
import { api } from "../../api";
import { useViewState } from "../../shell/viewMemoryStore";
import { textValue } from "../../shell/viewValuesModel.mjs";
import { NEW_WORKSTREAM, fire } from "../../shell/shortcuts";
import { useResolvedSettings } from "../../shell/useResolvedSettings";
import type { BranchInfo, GitRecoveryRef, GitStatusInfo, RemoteBranchInfo } from "../../types";
import { Button, Chip, ConfirmDialog, CopyText, EmptyState, ErrorNote, ICON, MoreMenu, PromptDialog, RelativeTime, SectionHeader, SkeletonRows, TextInput, Tooltip, copyText, useToast } from "../../ui";
import type { MenuItem } from "../../ui";
import { branchActions, branchFilter, branchOrder, consentWords, doneWords, localNameFor, standingWords } from "./branchActionsModel.mjs";
import type { BranchAction, BranchConsentKind } from "./branchActionsModel.mjs";
import { CherryPickDialog } from "./CherryPickDialog";
import { refNameProblem } from "./commitActionsModel.mjs";
import { DeleteBranchDialog } from "./DeleteBranchDialog";
import { recoveryWords, shortRef } from "./gitDiscardModel.mjs";
import * as ops from "./gitOps";
import { readsFor } from "./gitChangeModel.mjs";
import { useGitSession } from "./gitPanelStore";
import { useGitChanges } from "./useGitChanges";
import { VERB } from "./gitWords.mjs";
import { MergeDialog } from "./MergeDialog";
import { mergeDefault } from "./mergeModel.mjs";
import { NewBranchDialog } from "./NewBranchDialog";
import { PushWithLeaseDialog } from "./PushWithLeaseDialog";
import { RebaseDialog } from "./RebaseDialog";
import { RebaseEditorDialog } from "./RebaseEditorDialog";
import { RemotesSection } from "./RemotesSection";
import { SafetyNote } from "./SafetyNote";
import { TagDialog } from "./TagDialog";
import { UpstreamDialog } from "./UpstreamDialog";
import { useAsync } from "./useAsync";
import { t as tr } from "../../i18n/l10n.mjs";

/** A consented act waiting on the person's word in the one plain confirmation. */
type Pending = { kind: BranchConsentKind; name: string; from?: string; remote?: string; rec?: GitRecoveryRef };

/** A dialog with options of its own. */
type Open =
  | { kind: "merge"; source: string }
  | { kind: "rebase"; target: string }
  | { kind: "pick"; source: string }
  | { kind: "plan" }
  | { kind: "new" }
  | { kind: "delete"; branch: BranchInfo }
  | { kind: "upstream"; branch: BranchInfo }
  | { kind: "rename"; from: string }
  | { kind: "tag" }
  | { kind: "lease" }
  | null;

export function BranchesPanel({ pid, wid, status, defaultBranch = null, onChanged }: { pid: string; wid: string; status: GitStatusInfo; defaultBranch?: string | null; onChanged?: () => void }) {
  const toast = useToast();
  const branches = useAsync((s) => api.gitBranches(wid, s), [wid]);
  const tags = useAsync((s) => api.gitTags(wid, s), [wid]);
  const recovery = useAsync((s) => api.gitRecovery(wid, s), [wid]);
  const remotes = useAsync((s) => api.gitRemotes(wid, s), [wid]);
  const remoteBranches = useAsync((s) => api.gitRemoteBranches(wid, s), [wid]);
  const { resolved } = useResolvedSettings(pid);
  const mergeMode = mergeDefault(resolved?.find((r) => r.key === "git.merge_strategy")?.value);
  // Every act runs against the checkout's session: one busy flag for the
  // whole Git tab, and a result that lands whether or not this panel is
  // still mounted.
  const scope = rootKey("workstream", wid);
  const session = useGitSession(scope);
  const busy = session.busy !== null;
  const inProgress = status.in_progress ?? null;
  const [pending, setPending] = useState<Pending | null>(null);
  const [open, setOpen] = useState<Open>(null);
  // The filter is how the panel stood: a view switch keeps it, and a restart.
  const [filter, setFilter] = useViewState(gitPlace(scope), "branches.filter", "", textValue);
  /** Every list read again — after a write here, or a ref that moved under the checkout. */
  const reloadAll = () => {
    branches.reload();
    tags.reload();
    recovery.reload();
    remotes.reload();
    remoteBranches.reload();
  };
  // A write landed: every list is read again.
  const stale = session.stale;
  const [seen, setSeen] = useState(stale);
  if (seen !== stale) {
    setSeen(stale);
    reloadAll();
    onChanged?.();
  }
  // A ref or HEAD moved outside the platform — a branch made in a terminal,
  // a fetch from elsewhere, a rebase in progress (ide/04 §Live).
  useGitChanges(wid, (kinds) => {
    if (readsFor(kinds).branches) reloadAll();
  });

  const all = useMemo(() => branches.data?.branches ?? [], [branches.data?.branches]);
  const current = all.find((b) => b.current) ?? null;
  const currentName = current?.name ?? null;
  const ctx = { current: currentName, defaultBranch, busy, inProgress };
  const rows = useMemo(() => branchOrder(branchFilter(all, filter), defaultBranch), [all, filter, defaultBranch]);
  const remoteRows = remoteBranches.data?.remote_branches ?? [];
  const remoteNames = remoteRows.map((b) => `${b.remote}/${b.name}`);
  const starts = [...all.map((b) => b.name), ...remoteNames, ...(tags.data?.tags ?? []).map((t) => t.name)];
  const dirty = !status.clean;

  const done = (kind: Parameters<typeof doneWords>[0], facts?: Parameters<typeof doneWords>[1]) => doneWords(kind, facts);

  /** The person said yes in the plain confirmation. */
  const confirm = () => {
    const p = pending;
    setPending(null);
    if (!p) return;
    switch (p.kind) {
      case "switch":
        return void ops.checkout(scope, wid, p.name, done("switch", { name: p.name }));
      case "checkout_remote":
        return void ops.branchCreate(scope, wid, { name: p.name, start: p.from, track: true, switch: true }, done("checkout_remote", { name: p.name, to: p.from }));
      case "delete_tag":
        return void ops.tagDelete(scope, wid, p.name, done("delete_tag", { name: p.name }));
      case "delete_remote":
        return void ops.deleteRemoteBranch(scope, wid, p.remote ?? "origin", p.name, done("delete_remote", { name: p.name, to: p.remote ?? "origin" }));
      case "restore":
        return p.rec ? void ops.restore(scope, wid, p.rec.ref_name, done("restore", { name: shortRef(p.rec.ref_name) })) : undefined;
      default:
        return;
    }
  };

  const act = (b: BranchInfo, id: BranchAction["id"]) => {
    switch (id) {
      case "switch":
        return setPending({ kind: "switch", name: b.name });
      case "merge":
        return setOpen({ kind: "merge", source: b.name });
      case "rebase":
        return setOpen({ kind: "rebase", target: b.name });
      case "cherry_pick":
        return setOpen({ kind: "pick", source: b.name });
      case "rebase_plan":
        return setOpen({ kind: "plan" });
      case "push_lease":
        return setOpen({ kind: "lease" });
      case "upstream":
        return setOpen({ kind: "upstream", branch: b });
      case "delete":
        return setOpen({ kind: "delete", branch: b });
      case "rename":
        return setOpen({ kind: "rename", from: b.name });
      case "workstream":
        // A door: the New workstream dialog opens on this branch (the rail
        // listens), and nothing here moves.
        return fire(NEW_WORKSTREAM, { preset: { kind: "branch", branch: b.name } });
      default:
        return;
    }
  };

  const actRemote = (b: RemoteBranchInfo, id: string) => {
    const full = `${b.remote}/${b.name}`;
    switch (id) {
      case "checkout":
        return setPending({ kind: "checkout_remote", name: localNameFor(b, all), from: full });
      case "merge":
        return setOpen({ kind: "merge", source: full });
      case "rebase":
        return setOpen({ kind: "rebase", target: full });
      case "cherry_pick":
        return setOpen({ kind: "pick", source: full });
      case "fetch":
        return void api
          .gitFetch(wid, b.remote)
          .then(() => {
            toast.ok(tr("work-branches-panel-fetched", { full }));
            remoteBranches.reload();
            branches.reload();
          })
          .catch((e: unknown) => toast.error(e instanceof Error ? e.message : String(e)));
      case "delete_remote":
        return setPending({ kind: "delete_remote", name: b.name, remote: b.remote });
      case "workstream":
        return fire(NEW_WORKSTREAM, { preset: { kind: "remote", remote: b.remote, branch: b.name } });
      case "copy_name":
        return void copyText(b.name).then((ok) => ok && toast.ok(tr("work-branches-panel-copied", { b: b.name })));
      case "copy_full":
        return void copyText(full).then((ok) => ok && toast.ok(tr("work-branches-panel-copied-2", { full })));
      default:
        return;
    }
  };

  const BranchRow = ({ b }: { b: BranchInfo }) => {
    const actions = branchActions(b, ctx);
    const hover = actions.find((a) => a.hover) ?? null;
    const menu: MenuItem[] = actions
      .filter((a) => !a.hover)
      .map((a) => ({
        label: a.disabled && a.reason ? `${a.label} — ${a.reason}` : a.label,
        icon: ICON[a.icon],
        danger: a.danger,
        disabled: a.disabled,
        separatorBefore: a.separatorBefore,
        onSelect: () => act(b, a.id),
      }));
    const standing = standingWords(b);
    return (
      <li className={`group flex h-row-sm items-center gap-2 rounded-control px-1 text-2xs ${b.current ? "bg-surface-2" : ""}`}>
        <span className={`w-3 shrink-0 text-center ${b.current ? "text-accent-ink" : "text-transparent"}`} aria-hidden>
          ●
        </span>
        <span className={`min-w-0 truncate font-mono ${b.current ? "text-text" : "text-text-dim"}`} title={b.subject}>
          {b.name}
        </span>
        {b.current && <span className="shrink-0 text-text-dim">{tr("work-branches-panel-current")}</span>}
        {b.merged && !(defaultBranch !== null && b.name === defaultBranch) && (
          <Tooltip label={tr("work-branches-panel-every-commit", { defaultBranch: defaultBranch ?? tr("work-after-merge-dialog-default-branch") })}>
            <Chip tone="ok">{tr("work-branches-panel-merged")}</Chip>
          </Tooltip>
        )}
        {b.upstream && (
          <Tooltip label={standing ? tr("work-branches-panel-against", { standing, upstream: b.upstream }) : tr("work-branches-panel-step", { upstream: b.upstream })}>
            <span className="tnum shrink-0 text-text-dim">{standing || "↑ " + b.upstream}</span>
          </Tooltip>
        )}
        <RelativeTime at={b.timestamp} className="shrink-0 text-text-dim" />
        <span className="flex-1" />
        <span className="row-actions anim flex items-center gap-0.5">
          {hover && (
            <Tooltip label={hover.reason ?? hover.label}>
              <span className="inline-flex">
                <Button size="sm" variant="ghost" className="h-6" disabled={hover.disabled} onClick={() => act(b, hover.id)}>
                  <ICON.switchBranch size={12} aria-hidden />
                  {VERB.switch}
                </Button>
              </span>
            </Tooltip>
          )}
          <MoreMenu label={tr("work-branches-panel-more", { b: b.name })} items={menu} />
        </span>
      </li>
    );
  };

  if (branches.loading && !branches.data) return <SkeletonRows rows={3} />;
  if (branches.error && !branches.data) return <ErrorNote error={branches.error} retry={branches.reload} />;

  const copy = pending ? consentWords(pending.kind, { name: pending.name, from: pending.from, remote: pending.remote, current: currentName, rec: pending.rec ?? null }) : null;
  const cur = currentName ?? "HEAD";

  return (
    <div className="mt-2 flex min-w-0 flex-col gap-3">
      <div>
        <SectionHeader
          title={tr("work-branches-panel-branches")}
          count={all.length}
          showZero
          alwaysAction
          action={
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => setOpen({ kind: "new" })}>
              <ICON.branchHere size={12} aria-hidden />{tr("work-branches-panel-new-branch")}</Button>
          }
        />
        {all.length > 6 && (
          <div className="px-1 pb-1">
            <TextInput value={filter} placeholder={tr("work-branches-panel-filter-branches")} aria-label={tr("work-branches-panel-filter-branches-name-upstream")} className="h-6 text-2xs" onChange={(e) => setFilter(e.target.value)} />
          </div>
        )}
        <ul className="flex flex-col">
          {rows.map((b) => (
            <BranchRow key={b.name} b={b} />
          ))}
        </ul>
        {filter.trim() && rows.length === 0 && <p className="px-2 py-1 text-2xs text-text-dim">{tr("work-branches-panel-no-branch-matches")}</p>}
      </div>

      <div>
        <SectionHeader
          title={tr("work-agent-editor-tags")}
          count={tags.data?.tags.length ?? 0}
          showZero
          alwaysAction
          action={
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => setOpen({ kind: "tag" })}>
              <ICON.tagHere size={12} aria-hidden />{tr("work-branches-panel-tag-head")}</Button>
          }
        />
        {tags.error && !tags.data && <ErrorNote error={tags.error} retry={tags.reload} />}
        {tags.loading && !tags.data && <SkeletonRows rows={2} />}
        {tags.data && tags.data.tags.length === 0 && <EmptyState icon={ICON.tag} title={tr("work-branches-panel-no-tags")} hint={tr("work-branches-panel-tag-name-commit")} className="py-3" action={null} />}
        <ul className="flex flex-col">
          {tags.data?.tags.map((t) => (
            <li key={t.name} className="group flex h-row-sm items-center gap-2 px-1 text-2xs">
              <span className="font-mono text-warn">{t.name}</span>
              <span className="min-w-0 truncate text-text-dim" title={t.target}>
                {t.subject}
              </span>
              <RelativeTime at={t.timestamp} className="text-text-dim" />
              <span className="flex-1" />
              <span className="row-actions anim inline-flex items-center gap-1">
                <Tooltip label={tr("work-branches-panel-tag", { delete: VERB.delete, t: t.name })}>
                  <span className="inline-flex">
                    <Button size="sm" variant="ghost" className="h-6 hover:text-danger" disabled={busy} aria-label={tr("work-branches-panel-delete-tag", { t: t.name })} onClick={() => setPending({ kind: "delete_tag", name: t.name })}>
                      <ICON.delete size={12} aria-hidden />
                    </Button>
                  </span>
                </Tooltip>
                <MoreMenu
                  label={tr("work-branches-panel-tag-actions", { t: t.name })}
                  items={[
                    { label: tr("work-branches-panel-open-workstream", { t: t.name }), icon: ICON.workstream, onSelect: () => fire(NEW_WORKSTREAM, { preset: { kind: "tag", tag: t.name } }) },
                    { label: tr("work-branches-panel-tag-2", { delete: VERB.delete }), icon: ICON.delete, danger: true, disabled: busy, separatorBefore: true, onSelect: () => setPending({ kind: "delete_tag", name: t.name }) },
                  ]}
                />
              </span>
            </li>
          ))}
        </ul>
      </div>

      {remotes.error && !remotes.data ? (
        <ErrorNote error={remotes.error} retry={remotes.reload} />
      ) : (remotes.loading && !remotes.data) || (remoteBranches.loading && !remoteBranches.data) ? (
        <SkeletonRows rows={2} />
      ) : (
        <RemotesSection wid={wid} remotes={remotes.data?.remotes ?? []} remoteBranches={remoteRows} branches={all} branchCtx={ctx} defaultBranch={defaultBranch} busy={busy} onBranchAct={actRemote} onChanged={() => void 0} />
      )}

      <div>
        <SectionHeader title={tr("work-branches-panel-safety")} count={recovery.data?.recovery.length ?? 0} showZero trailing={<SafetyNote />} />
        {recovery.error && !recovery.data && <ErrorNote error={recovery.error} retry={recovery.reload} />}
        {recovery.loading && !recovery.data && <SkeletonRows rows={2} />}
        <ul className="flex flex-col">
          {recovery.data?.recovery.map((r) => (
            <li key={r.ref_name} className="group flex h-row-sm items-center gap-2 px-1 text-2xs">
              <span className="font-mono text-text">{r.op.replaceAll("_", " ")}</span>
              {r.branch && <span className="font-mono text-text-dim">{tr("work-branches-panel-words", { branch: r.branch })}</span>}
              <span className="text-text-dim">{recoveryWords(r.kind)}</span>
              <RelativeTime at={r.at} className="text-text-dim" />
              <span className="flex-1" />
              <Button size="sm" variant="ghost" className="h-6" disabled={busy} onClick={() => setPending({ kind: "restore", name: shortRef(r.ref_name), rec: r })}>
                <ICON.discard size={12} aria-hidden />
                {VERB.restore}
              </Button>
            </li>
          ))}
        </ul>
        {recovery.data && recovery.data.recovery.length === 0 && (
          <EmptyState
            title={tr("work-branches-panel-nothing-saved-yet")}
            hint={tr("work-branches-panel-recovery-point-appears-here-first-time")}
            className="py-3"
            // Nothing to do: the list fills as a side effect of the verbs
            // above, and offering one of them here would be a way to move
            // the tree from a section that promised only to show what was saved.
            action={null}
          />
        )}
        <div className="mt-1 flex flex-wrap items-center gap-2 text-2xs text-text-dim">
          <span>{tr("work-branches-panel-pruning-person-s-job")}</span>
          <CopyText value="just prune-recovery-refs" />
        </div>
      </div>

      {/* The plain confirmations — a switch, a remote checkout, a tag's or a
          remote branch's delete, a restore — one dialog, the words the
          model's, the promise the SafetyNote's. */}
      <ConfirmDialog
        open={pending !== null}
        onClose={() => setPending(null)}
        onConfirm={confirm}
        title={copy?.title ?? ""}
        body={
          <div className="flex flex-col gap-2">
            {copy?.body && <p>{copy.body}</p>}
            <SafetyNote kind={copy?.kind} />
          </div>
        }
        confirmLabel={copy?.confirm ?? VERB.continue}
        danger={copy?.danger ?? false}
      />
      <MergeDialog
        open={open?.kind === "merge"}
        onClose={() => setOpen(null)}
        wid={wid}
        source={open?.kind === "merge" ? open.source : ""}
        target={cur}
        defaultMode={mergeMode}
        busy={busy}
        onConfirm={(mode, message) => {
          const source = open?.kind === "merge" ? open.source : "";
          setOpen(null);
          void ops.mergeBranch(scope, wid, { source, mode, message }, cur, done("merge", { name: source }));
        }}
      />
      <RebaseDialog
        open={open?.kind === "rebase"}
        onClose={() => setOpen(null)}
        wid={wid}
        current={cur}
        target={open?.kind === "rebase" ? open.target : ""}
        dirty={dirty}
        busy={busy}
        onConfirm={(body) => {
          setOpen(null);
          void ops.rebase(scope, wid, body, cur, done("rebase", { name: body.onto ?? body.upstream }));
        }}
      />
      <CherryPickDialog
        open={open?.kind === "pick"}
        onClose={() => setOpen(null)}
        wid={wid}
        source={open?.kind === "pick" ? open.source : null}
        current={cur}
        busy={busy}
        onConfirm={(commits, opts) => {
          setOpen(null);
          void ops.cherryPick(scope, wid, { commits, record_origin: opts.recordOrigin, no_commit: opts.noCommit }, cur, done("cherry_pick", { count: commits.length }));
        }}
      />
      <RebaseEditorDialog
        open={open?.kind === "plan"}
        onClose={() => setOpen(null)}
        wid={wid}
        current={cur}
        targets={[...all.filter((b) => !b.current).map((b) => b.name), ...remoteNames]}
        defaultTarget={current?.upstream ?? defaultBranch}
        busy={busy}
        onConfirm={(plan, summary) => {
          setOpen(null);
          void ops.rebasePlan(scope, wid, plan, cur, done("rebase_plan", { name: cur, to: summary }));
        }}
      />
      <NewBranchDialog
        open={open?.kind === "new"}
        onClose={() => setOpen(null)}
        current={currentName}
        starts={starts}
        busy={busy}
        onConfirm={(body) => {
          setOpen(null);
          void ops.branchCreate(scope, wid, body, done(body.switch ? "create_switch" : "create", { name: body.name }));
        }}
      />
      <DeleteBranchDialog
        open={open?.kind === "delete"}
        onClose={() => setOpen(null)}
        branch={open?.kind === "delete" ? open.branch : null}
        busy={busy}
        onConfirm={(alsoRemote) => {
          const b = open?.kind === "delete" ? open.branch : null;
          setOpen(null);
          if (!b) return;
          void ops.branchDelete(scope, wid, b.name, done("delete_branch", { name: b.name })).then((ran) => {
            if (ran && alsoRemote) void ops.deleteRemoteBranch(scope, wid, alsoRemote.remote, alsoRemote.name, done("delete_remote", { name: alsoRemote.name, to: alsoRemote.remote }));
          });
        }}
      />
      <UpstreamDialog
        open={open?.kind === "upstream"}
        onClose={() => setOpen(null)}
        branch={open?.kind === "upstream" ? open.branch : null}
        remoteBranches={remoteNames}
        busy={busy}
        onConfirm={(upstream) => {
          const b = open?.kind === "upstream" ? open.branch : null;
          setOpen(null);
          if (b) void ops.setUpstream(scope, wid, b.name, upstream, done("upstream", { name: b.name, to: upstream ?? "" }));
        }}
      />
      <PromptDialog
        open={open?.kind === "rename"}
        onClose={() => setOpen(null)}
        title={`${VERB.rename} ${open?.kind === "rename" ? open.from : ""}`}
        description={tr("work-branches-panel-branch-keeps-commits-upstream-workstream-checked")}
        label={tr("work-branches-panel-new-name")}
        initial={open?.kind === "rename" ? open.from : ""}
        placeholder="feature/from-here" // for the machine
        mono
        submitLabel={VERB.rename}
        busy={busy}
        validate={(v) => refNameProblem(v.trim())}
        onSubmit={(name) => {
          const from = open?.kind === "rename" ? open.from : null;
          setOpen(null);
          if (!from || name.trim() === from) return;
          void ops.branchRename(scope, wid, from, name.trim(), done("rename", { name: from, to: name.trim() }));
        }}
      />
      <TagDialog
        open={open?.kind === "tag"}
        onClose={() => setOpen(null)}
        target={cur}
        busy={busy}
        onSubmit={(name, message) => {
          setOpen(null);
          void ops.tagCreate(scope, wid, name, null, message || null, done("tag", { name }));
        }}
      />
      <PushWithLeaseDialog open={open?.kind === "lease"} onClose={() => setOpen(null)} wid={wid} branch={currentName} />
    </div>
  );
}
