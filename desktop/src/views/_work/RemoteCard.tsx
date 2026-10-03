/**
 * The project's remotes as a card: `origin` first — where the
 * project publishes — shown as `host · owner/name` with **Edit** (one field,
 * Save; `PUT …/git/remotes/origin` re-points it in place and the project
 * record follows) or **Add origin** when there is none; every other remote
 * listed under it with the same edit and a consented remove. Adding and
 * re-pointing are configuration, not history — safe; removing is consented
 * and its recovery ref is named in the toast.
 */
import { useState } from "react";
import { api } from "../../api";
import type { RemoteInfo } from "../../types";
import { Button, ConfirmDialog, ErrorNote, ICON, SkeletonRows, TextInput, Tooltip, failureText, useToast } from "../../ui";
import { confirmLabel } from "./gitWords.mjs";
import { isRemoteUrl, remoteSummary } from "./remoteModel.mjs";
import { attempt, useAsync } from "./useAsync";
import { t } from "../../i18n/l10n.mjs";

export function RemoteCard({ wid, onChanged }: { wid: string; onChanged?: () => void }) {
  const toast = useToast();
  const remotes = useAsync((s) => api.gitRemotes(wid, s), [wid]);
  /** The remote being edited (its name), or the name of a new one being added. */
  const [editing, setEditing] = useState<{ name: string; url: string; adding: boolean } | null>(null);
  const [removing, setRemoving] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const reload = () => {
    remotes.reload();
    onChanged?.();
  };

  const save = () => {
    if (!editing || busy) return;
    const name = editing.name.trim();
    const url = editing.url.trim();
    if (!name || !isRemoteUrl(url)) return;
    setBusy(true);
    void attempt(
      () => api.gitRemoteSet(wid, name, url),
      (e) => {
        setBusy(false);
        toast.error(e);
      },
      () => {
        setBusy(false);
        setEditing(null);
        toast.ok(`${name} → ${remoteSummary(url)}.`);
        reload();
      },
    );
  };

  const remove = async (name: string) => {
    setRemoving(null);
    if (busy) return;
    setBusy(true);
    try {
      const r = await api.gitRemoteRemove(wid, name);
      // The recovery ref pins the tree, not the URL: the toast says what went.
      void r;
      toast.ok(t("work-remote-card-deleted-remote", { name }));
      reload();
    } catch (e) {
      toast.error(failureText("work", "remote-card-failed", e));
    } finally {
      setBusy(false);
    }
  };

  if (remotes.loading && !remotes.data) return <SkeletonRows rows={2} />;
  if (remotes.error && !remotes.data) return <ErrorNote error={remotes.error} retry={remotes.reload} />;
  const list: RemoteInfo[] = remotes.data?.remotes ?? [];
  const origin = list.find((r) => r.name === "origin") ?? null;
  const others = list.filter((r) => r.name !== "origin");

  const row = (r: RemoteInfo) => (
    <li key={r.name} className="group flex items-center gap-2 text-2xs">
      <span className="w-14 shrink-0 font-mono text-text">{r.name}</span>
      <span className="min-w-0 flex-1 truncate text-text-dim" title={r.url}>
        {remoteSummary(r.url)} <span className="font-mono text-text-dim/70">{r.url}</span>
      </span>
      <span className="row-actions anim flex items-center gap-0.5">
        <Button size="sm" variant="ghost" disabled={busy} onClick={() => setEditing({ name: r.name, url: r.url, adding: false })}>
          <ICON.edit size={12} aria-hidden />{t("work-remote-card-edit")}</Button>
        <Tooltip label={t("work-remote-card-delete-remote", { r: r.name })}>
          <span className="inline-flex">
            <Button size="sm" variant="ghost" className="hover:text-danger" disabled={busy} aria-label={t("work-remote-card-delete-remote-2", { r: r.name })} onClick={() => setRemoving(r.name)}>
              <ICON.delete size={12} aria-hidden />
            </Button>
          </span>
        </Tooltip>
      </span>
    </li>
  );

  return (
    <div className="flex flex-col gap-2">
      {origin ? (
        <ul className="flex flex-col gap-1">{row(origin)}</ul>
      ) : (
        <div className="flex items-center gap-2 text-2xs text-text-dim">
          <span className="flex-1">{t("work-remote-card-no-origin-nothing-pull-from-push")}</span>
          <Button size="sm" variant="primary" disabled={busy || editing !== null} onClick={() => setEditing({ name: "origin", url: "", adding: true })}>
            <ICON.add size={12} aria-hidden />{t("work-remote-card-add-origin")}</Button>
        </div>
      )}
      {others.length > 0 && <ul className="flex flex-col gap-1">{others.map(row)}</ul>}
      {editing ? (
        <form
          className="flex flex-wrap items-center gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            save();
          }}
        >
          {editing.adding && editing.name !== "origin" ? (
            <TextInput
              value={editing.name}
              placeholder={t("work-remote-card-name")}
              aria-label={t("work-remote-card-remote-name")}
              className="h-6 w-24 font-mono text-2xs"
              onChange={(e) => setEditing({ ...editing, name: e.target.value })}
            />
          ) : (
            <span className="w-14 shrink-0 font-mono text-2xs text-text">{editing.name}</span>
          )}
          <TextInput
            autoFocus
            value={editing.url}
            placeholder="git@github.com:owner/repo.git" // for the machine
            aria-label={t("work-remote-card-url", { editing: editing.name })}
            className="h-6 min-w-0 flex-1 font-mono text-2xs"
            onChange={(e) => setEditing({ ...editing, url: e.target.value })}
          />
          <Button size="sm" variant="primary" type="submit" disabled={busy || !editing.name.trim() || !isRemoteUrl(editing.url)}>{t("work-agent-editor-save")}</Button>
          <Button size="sm" variant="ghost" onClick={() => setEditing(null)}>{t("work-agent-editor-cancel")}</Button>
        </form>
      ) : (
        origin && (
          <div>
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => setEditing({ name: "", url: "", adding: true })}>
              <ICON.add size={12} aria-hidden />{t("work-remote-card-add-another-remote")}</Button>
          </div>
        )
      )}
      <ConfirmDialog
        open={removing !== null}
        onClose={() => setRemoving(null)}
        onConfirm={() => removing && void remove(removing)}
        title={t("work-remote-card-delete-remote-3", { removing: removing ?? "" })}
        body={
          <div className="flex flex-col gap-2">
            <p>{t("work-remote-card-url-and-tracking-branches-go")}</p>
            <p className="text-text-dim">{t("work-remote-card-nothing-pinned-safety-remote-url-configuration")}</p>
          </div>
        }
        confirmLabel={confirmLabel("delete")}
        danger
      />
    </div>
  );
}
