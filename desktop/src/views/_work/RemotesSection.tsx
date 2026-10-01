/**
 * The Branches view's Remotes section (ide/04 §Remotes): the count, *Fetch
 * all* and *Add remote…* in the header with the layout switch beside them;
 * one **name row** per remote — the chevron, `origin` in mono, a count
 * chip, *Fetch* on hover and a `⋮` from `remoteActionsModel.mjs`; where a
 * remote lives, how it is reached and its URL are the name's tooltip and
 * the `⋮`'s verbs, never the row's width — and, open under it, its
 * branches as one kit tree (`TreeList`, ide/03) in the explorer's shape
 * through `remoteTreeModel.mjs`: folders from the slashes in the tree
 * layout, every branch newest first in the list, a filter over all of them
 * when the section is long. A branch row is its basename, its standing as
 * chips (*default*, *tracked by main*), when, and on hover **Check out**
 * and a `⋮` holding the acts against it (`branchActionsModel.remoteBranchActions`),
 * which the Branches view runs; the commit it points at is the row's
 * tooltip. Adding and re-pointing a remote are configuration, not history;
 * deleting one is consented and the dialog says the truth — nothing is
 * pinned in Safety for a remote — and offers the URL to copy first.
 *
 * The reads are the parent's (`BranchesPanel` owns the lists and the
 * session's `busy`); this section runs the remote writes itself and hands
 * a branch's act up.
 */
import { useMemo, useRef, useState, type KeyboardEvent } from "react";
import { api, openExternal } from "../../api";
import type { BranchInfo, RemoteBranchInfo, RemoteInfo } from "../../types";
import { Button, Chip, ConfirmDialog, CopyText, Dialog, Field, ICON, MoreMenu, RelativeTime, SectionHeader, SegmentedControl, TextInput, Tooltip, TreeList, cn, copyText, useCollapsed, useCollapsedUnder, useToast, useTokenPx } from "../../ui";
import type { MenuItem, TreeRowState } from "../../ui";
import { setCollapsed } from "../../ui/collapsedStore";
import { REMOTE_LAYOUTS, REMOTE_LAYOUT_LABEL } from "../_workbench/rightPanelModel.mjs";
import type { RemoteLayout } from "../_workbench/rightPanelModel.mjs";
import { setPanelView, usePanelView } from "../_workbench/rightPanelStore";
import { VERB } from "./gitWords.mjs";
import { remoteBranchActions } from "./branchActionsModel.mjs";
import type { BranchContext, RemoteBranchActionId } from "./branchActionsModel.mjs";
import { deleteRemoteWords, fetchWords, fetchedWords, groupRemoteBranches, hostPage, noBranchesWords, remoteActions, remoteTitle } from "./remoteActionsModel.mjs";
import type { RemoteAction, RemoteGroup } from "./remoteActionsModel.mjs";
import { isRemoteUrl, remoteSummary } from "./remoteModel.mjs";
import { countWords, remoteBranchRows, wantsFilter } from "./remoteTreeModel.mjs";
import type { RemoteBranchRow, RemoteDirRow, RemoteRow } from "./remoteTreeModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The one form: a new remote (name and URL) or a remote's new URL. */
type Editing = { kind: "add"; name: string; url: string } | { kind: "edit"; name: string; url: string };

const LAYOUT_ICON = { tree: ICON.tree, list: ICON.list } as const;
/** The explorer's indent, so the two trees read alike. */
const INDENT = 14;
const ROW_HEIGHT_FALLBACK = 28;
const ROW = "group anim relative mr-1 flex h-row-sm items-center gap-1 rounded-control pr-1";
const CURSOR = "ring-1 ring-inset ring-accent/50";

type Actionable<Id extends string> = readonly { id: Id; label: string; icon: keyof typeof ICON; hover: boolean; danger: boolean; disabled: boolean; reason: string | null; separatorBefore?: boolean }[];

/** An action list as a menu — the hover verb left out, a reason folded into a disabled label. */
function menuOf<Id extends string>(actions: Actionable<Id>, act: (id: Id) => void): MenuItem[] {
  return actions
    .filter((a) => !a.hover)
    .map((a) => ({
      label: a.disabled && a.reason ? `${a.label} — ${a.reason}` : a.label,
      icon: ICON[a.icon],
      danger: a.danger,
      disabled: a.disabled,
      separatorBefore: a.separatorBefore,
      onSelect: () => act(a.id),
    }));
}

export function RemotesSection({
  wid,
  remotes,
  remoteBranches,
  branches,
  branchCtx,
  defaultBranch = null,
  busy,
  onBranchAct,
  onChanged,
}: {
  wid: string;
  remotes: RemoteInfo[];
  remoteBranches: RemoteBranchInfo[];
  branches: BranchInfo[];
  /** What a remote branch's verbs are read against — the current branch, the default, what is half-done. */
  branchCtx: BranchContext;
  /** The project's default branch, for the *default* chip. */
  defaultBranch?: string | null;
  /** The view's shared flag: an operation elsewhere in the view is running. */
  busy: boolean;
  /** A remote branch's act, run by the Branches view. */
  onBranchAct: (branch: RemoteBranchInfo, id: RemoteBranchActionId) => void;
  onChanged: () => void;
}) {
  const toast = useToast();
  const [working, setWorking] = useState<{ kind: "fetch"; names: string[] } | { kind: "write" } | null>(null);
  const [editing, setEditing] = useState<Editing | null>(null);
  const [deleting, setDeleting] = useState<RemoteInfo | null>(null);
  const [filter, setFilter] = useState("");
  const urlField = useRef<HTMLInputElement>(null);
  const layout = usePanelView("remotes");
  const held = busy || working !== null;
  const groups = useMemo(() => groupRemoteBranches(remotes, remoteBranches, branches), [remotes, remoteBranches, branches]);
  const fetching = working?.kind === "fetch" ? new Set(working.names) : new Set<string>();

  /** Fetch some remotes one after the other; one toast says what came in. */
  const fetchAll = async (names: string[]) => {
    if (held || names.length === 0) return;
    setWorking({ kind: "fetch", names });
    const results: { name: string; ok: boolean; error?: string }[] = [];
    for (const name of names) {
      try {
        await api.gitFetch(wid, name);
        results.push({ name, ok: true });
      } catch (e) {
        results.push({ name, ok: false, error: e instanceof Error ? e.message : String(e) });
      }
    }
    setWorking(null);
    if (results.every((r) => r.ok)) toast.ok(fetchedWords(results));
    else toast.error(fetchedWords(results));
    onChanged();
  };

  const save = async () => {
    if (!editing || held) return;
    const name = editing.name.trim();
    const url = editing.url.trim();
    if (!name || !isRemoteUrl(url)) return;
    setWorking({ kind: "write" });
    try {
      if (editing.kind === "add") await api.gitRemoteAdd(wid, name, url);
      else await api.gitRemoteSet(wid, name, url);
      setEditing(null);
      toast.ok(`${name} → ${remoteSummary(url)}.`);
      onChanged();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setWorking(null);
    }
  };

  const remove = async (remote: RemoteInfo) => {
    setDeleting(null);
    if (held) return;
    setWorking({ kind: "write" });
    try {
      await api.gitRemoteRemove(wid, remote.name);
      toast.ok(deleteRemoteWords(remote).done);
      onChanged();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setWorking(null);
    }
  };

  const actRemote = (remote: RemoteInfo, id: RemoteAction["id"]) => {
    switch (id) {
      case "fetch":
        return void fetchAll([remote.name]);
      case "edit_url":
        return setEditing({ kind: "edit", name: remote.name, url: remote.url });
      case "copy_url":
        return void copyText(remote.url).then((ok) => ok && toast.ok(t("work-remotes-section-copied-url")));
      case "open_host": {
        const page = hostPage(remote.url);
        return page ? void openExternal(page).catch((e: unknown) => toast.error(e instanceof Error ? e.message : String(e))) : undefined;
      }
      case "delete":
        return setDeleting(remote);
      default:
        return;
    }
  };

  const words = deleteRemoteWords(deleting ?? { name: "", url: "" });

  return (
    <div>
      <SectionHeader
        title={t("work-checkout-view-remotes")}
        count={remotes.length}
        showZero
        alwaysAction
        action={
          <span className="flex items-center gap-0.5">
            {remotes.length > 0 && (
              <SegmentedControl
                label={t("work-remotes-section-remote-branches-layout")}
                size="sm"
                iconOnly
                value={layout}
                onChange={(l) => setPanelView("remotes", l as RemoteLayout)}
                options={REMOTE_LAYOUTS.map((l) => ({ id: l, label: REMOTE_LAYOUT_LABEL[l], icon: LAYOUT_ICON[l] }))}
              />
            )}
            {remotes.length > 0 && (
              <Button size="sm" variant="ghost" disabled={held} onClick={() => void fetchAll(remotes.map((r) => r.name))}>
                <ICON.refresh size={12} aria-hidden className={working?.kind === "fetch" ? "animate-spin" : undefined} />
                {fetchWords(remotes.length, { running: working?.kind === "fetch" })}
              </Button>
            )}
            <Button size="sm" variant="ghost" disabled={held} onClick={() => setEditing({ kind: "add", name: remotes.length === 0 ? "origin" : "", url: "" })}>
              <ICON.add size={12} aria-hidden />{t("work-remotes-section-add-remote")}</Button>
          </span>
        }
      />
      {remotes.length === 0 && <p className="px-2 py-2 text-2xs text-text-dim">{t("work-remotes-section-no-remote-nothing-fetch-from-push")}</p>}
      {wantsFilter(groups) && (
        <div className="px-1 pb-1">
          <TextInput value={filter} placeholder={t("work-remotes-section-filter-remote-branches")} aria-label={t("work-remotes-section-filter-remote-branches-name")} className="h-6 text-2xs" onChange={(e) => setFilter(e.target.value)} />
        </div>
      )}
      <ul className="flex flex-col gap-1">
        {groups.map((g) => (
          <RemoteGroupRows
            key={g.remote.name}
            wid={wid}
            group={g}
            layout={layout}
            filter={filter}
            defaultBranch={defaultBranch}
            held={held}
            fetching={fetching.has(g.remote.name)}
            branchCtx={branchCtx}
            onRemote={(id) => actRemote(g.remote, id)}
            onBranch={onBranchAct}
          />
        ))}
      </ul>

      <Dialog
        open={editing !== null}
        onClose={() => setEditing(null)}
        title={editing?.kind === "add" ? t("work-remotes-section-add-remote-2") : t("work-remotes-section-re-point", { editing: editing?.name ?? "" })}
        description={editing?.kind === "add" ? t("work-remotes-section-name-where-lives-configuration-not-history") : t("work-remotes-section-remote-keeps-name-branches-only-where")}
        width="max-w-md"
        initialFocus={urlField}
        footer={
          <>
            <Button size="sm" variant="ghost" onClick={() => setEditing(null)}>{t("work-agent-editor-cancel")}</Button>
            <Button size="sm" variant="primary" disabled={held || !editing?.name.trim() || !isRemoteUrl(editing?.url ?? "")} onClick={() => void save()}>
              {editing?.kind === "add" ? t("work-model-plan-editor-add") : t("work-agent-editor-save")}
            </Button>
          </>
        }
      >
        {editing && (
          <form
            className="flex flex-col gap-3"
            onSubmit={(e) => {
              e.preventDefault();
              void save();
            }}
          >
            <Field label={t("work-agent-editor-name")}>
              <TextInput
                value={editing.name}
                readOnly={editing.kind === "edit"}
                placeholder="origin" // for the machine
                aria-label={t("work-remote-card-remote-name")}
                className="h-7 font-mono text-xs"
                onChange={(e) => setEditing({ ...editing, name: e.target.value })}
              />
            </Field>
            <Field label={t("work-remotes-section-url")} hint={editing.url.trim() && !isRemoteUrl(editing.url) ? t("work-remotes-section-not-url-git-can-reach-ssh") : undefined}>
              <TextInput ref={urlField} value={editing.url} /* for the machine */ placeholder="git@github.com:owner/repo.git" aria-label={t("work-remotes-section-url-2", { remote: editing.name || t("work-remotes-section-the-remote") })} className="h-7 font-mono text-xs" onChange={(e) => setEditing({ ...editing, url: e.target.value })} />
            </Field>
          </form>
        )}
      </Dialog>

      <ConfirmDialog
        open={deleting !== null}
        onClose={() => setDeleting(null)}
        onConfirm={() => deleting && void remove(deleting)}
        title={words.title}
        body={
          <div className="flex flex-col gap-2">
            <p>{words.body}</p>
            {deleting && (
              <div className="flex items-center gap-2 rounded-control border border-border bg-surface-2 px-2 py-1 font-mono text-2xs">
                <span className="min-w-0 flex-1 truncate">{deleting.url}</span>
                <CopyText value={deleting.url} label={t("work-remotes-section-copy-url")} />
              </div>
            )}
          </div>
        }
        confirmLabel={words.confirm}
        danger={words.danger}
      />
    </div>
  );
}

/** One remote's name row and, open under it, its branches as a tree. */
function RemoteGroupRows({
  wid,
  group,
  layout,
  filter,
  defaultBranch,
  held,
  fetching,
  branchCtx,
  onRemote,
  onBranch,
}: {
  wid: string;
  group: RemoteGroup;
  layout: RemoteLayout;
  filter: string;
  defaultBranch: string | null;
  held: boolean;
  /** This remote is being fetched. */
  fetching: boolean;
  branchCtx: BranchContext;
  onRemote: (id: RemoteAction["id"]) => void;
  onBranch: (b: RemoteBranchInfo, id: RemoteBranchActionId) => void;
}) {
  const r = group.remote;
  const [folded, toggle] = useCollapsed(`remote.${wid}.${r.name}`, r.name !== "origin");
  const prefix = `remote.${wid}.${r.name}.`;
  const foldedFolders = useCollapsedUnder(prefix);
  const rows = useMemo(() => remoteBranchRows(wid, group, { layout, folded: foldedFolders, filter, defaultBranch }), [wid, group, layout, foldedFolders, filter, defaultBranch]);
  const [cursor, setCursor] = useState<string | null>(null);
  const rowHeight = useTokenPx("--spacing-row-sm", ROW_HEIGHT_FALLBACK);
  const actions = remoteActions(r, { busy: held });
  const hover = actions.find((a) => a.hover) ?? null;
  const filtering = filter.trim() !== "";

  const fold = (id: string, open: boolean) => setCollapsed(id, !open);
  const verbOn = (row: RemoteBranchRow, id: RemoteBranchActionId) => onBranch(row.branch, id);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const row = rows.find((x) => x.id === cursor);
    if (!row) return;
    switch (e.key) {
      case "Enter":
        if (row.kind === "dir") fold(row.id, !row.expanded);
        else return;
        break;
      case " ": {
        if (row.kind !== "branch") return;
        const door = remoteBranchActions(row.branch, branchCtx).find((a) => a.hover);
        if (door && !door.disabled) verbOn(row, door.id as RemoteBranchActionId);
        break;
      }
      default:
        return;
    }
    e.preventDefault();
  };

  const renderDir = (row: RemoteDirRow, rs: TreeRowState) => {
    const Glyph = row.expanded ? ICON.folderOpen : ICON.folder;
    return (
      <div style={{ marginLeft: rs.indent }} className={cn(ROW, "cursor-pointer hover:bg-surface-2", rs.cursor && CURSOR)} onMouseDown={() => setCursor(row.id)} onClick={() => fold(row.id, !row.expanded)} title={`${row.path}/ — ${countWords(row.count)}`}>
        <span className="flex h-4 w-4 shrink-0 items-center justify-center text-text-dim">
          {row.expanded ? <ICON.expanded size={12} aria-hidden /> : <ICON.collapsed size={12} aria-hidden />}
        </span>
        <Glyph size={13} aria-hidden className="shrink-0 text-text-dim" />
        <span className="min-w-0 flex-1 truncate font-mono text-2xs text-text">{row.label}</span>
        <span className="tnum text-2xs text-text-dim/70">{row.count}</span>
      </div>
    );
  };

  const renderBranch = (row: RemoteBranchRow, rs: TreeRowState) => {
    const b = row.branch;
    const acts = remoteBranchActions(b, branchCtx);
    const door = acts.find((a) => a.hover) ?? null;
    return (
      <div style={{ marginLeft: rs.indent }} className={cn(ROW, "text-2xs", rs.cursor && CURSOR)} onMouseDown={() => setCursor(row.id)} title={row.title}>
        <span aria-hidden className="w-4 shrink-0" />
        <ICON.branch size={12} aria-hidden className="shrink-0 text-text-dim" />
        <span className="min-w-0 flex-1 truncate font-mono text-text">{row.label}</span>
        {row.standing.isDefault && (
          <Tooltip label={t("work-remotes-section-project-s-default-branch")}>
            <span className="inline-flex">
              <Chip tone="accent">{t("work-remotes-section-default")}</Chip>
            </span>
          </Tooltip>
        )}
        {row.standing.trackedBy && (
          <Tooltip label={t("work-remotes-section-branch-tracks", { trackedBy: row.standing.trackedBy })}>
            <span className="inline-flex">
              <Chip tone="quiet">{t("work-remotes-section-tracked", { trackedBy: row.standing.trackedBy })}</Chip>
            </span>
          </Tooltip>
        )}
        <RelativeTime at={b.timestamp} className="shrink-0 text-text-dim" />
        <span className="row-actions anim flex shrink-0 items-center gap-0.5" onClick={(e) => e.stopPropagation()}>
          {door && (
            <Tooltip label={door.reason ?? door.label}>
              <span className="inline-flex">
                <Button size="sm" variant="ghost" className="h-6" disabled={door.disabled} onClick={() => verbOn(row, door.id as RemoteBranchActionId)}>
                  <ICON.switchBranch size={12} aria-hidden />{t("work-remotes-section-check-out")}</Button>
              </span>
            </Tooltip>
          )}
          <MoreMenu label={t("work-remotes-section-more", { full: b.full })} items={menuOf(acts, (id) => verbOn(row, id as RemoteBranchActionId))} />
        </span>
      </div>
    );
  };

  return (
    <li className="flex flex-col">
      <div className="group flex h-row-sm items-center gap-2 rounded-control px-1 text-2xs">
        <button type="button" onClick={toggle} aria-expanded={!folded} aria-label={t("work-remotes-section-show-hide-branches", { flag: (folded) ? "yes" : "no", r: r.name })} className="flex w-4 shrink-0 items-center justify-center text-text-dim">
          <ICON.collapsed size={11} aria-hidden className={`anim ${folded ? "" : "rotate-90"}`} />
        </button>
        <Tooltip label={remoteTitle(r)}>
          <span className="min-w-0 truncate font-mono text-text">{r.name}</span>
        </Tooltip>
        <Chip tone="quiet">{countWords(group.branches.length)}</Chip>
        <span className="flex-1" />
        <span className="row-actions anim flex items-center gap-0.5">
          {hover && (
            <Tooltip label={hover.reason ?? hover.label}>
              <span className="inline-flex">
                <Button size="sm" variant="ghost" className="h-6" disabled={hover.disabled} onClick={() => onRemote(hover.id)}>
                  <ICON.refresh size={12} aria-hidden className={fetching ? "animate-spin" : undefined} />
                  {VERB.fetch}
                </Button>
              </span>
            </Tooltip>
          )}
          <MoreMenu label={t("work-remotes-section-more-2", { r: r.name })} items={menuOf(actions, onRemote)} />
        </span>
      </div>
      {!folded &&
        (group.branches.length === 0 ? (
          <p className="py-1 pl-6 pr-1 text-2xs text-text-dim">{noBranchesWords(r.name)}</p>
        ) : rows.length === 0 && filtering ? (
          <p className="py-1 pl-6 pr-1 text-2xs text-text-dim">{t("work-branches-panel-no-branch-matches")}</p>
        ) : (
          <div className="pl-3">
            <TreeList
              rows={rows}
              rowHeight={rowHeight}
              measure
              indent={INDENT}
              guides={layout === "tree"}
              unbounded
              cursor={cursor}
              onCursor={(id) => setCursor(id)}
              onAction={(a) => {
                const ids = a.kind === "expandAll" ? a.ids : [a.id];
                for (const id of ids) fold(id, a.kind !== "collapse");
              }}
              onKeyDown={onKeyDown}
              label={t("work-remotes-section-branches", { r: r.name })}
              className="rounded-control focus-visible:ring-1 focus-visible:ring-accent/50"
              render={(row: RemoteRow, rs) => (row.kind === "dir" ? renderDir(row, rs) : renderBranch(row, rs))}
            />
          </div>
        ))}
    </li>
  );
}
