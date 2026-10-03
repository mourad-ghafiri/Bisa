/**
 * Settings › Git & code hosts › Identity — *Profiles by organization* (ide/04):
 * who you are for one owner on one code host. Each profile is one git config
 * file the platform owns, included by your global config for that owner's
 * remotes — so a checkout under `github.com/acme` commits as the profile's
 * author, pushes with its key and opens pull requests as its account, and
 * every other checkout does not. The list, one dialog to add or edit, a
 * confirmed remove; the includes of your global file that are not the
 * platform's are shown and left alone. The words and the checks are
 * `gitProfilesModel.mjs`'s.
 */

import { useState } from "react";
import { api } from "../../api";
import { useEngineEvents } from "../../bus";
import type { GitProfileView, ProfileSpec } from "../../types";
import { Button, Card, Chip, ConfirmDialog, Dialog, ErrorNote, Field, ICON, Pending, ReadLine, Section, Select, TextInput, useToast } from "../../ui";
import { KINDS } from "../_work/codeHostWords.mjs";
import { attempt, useAsync } from "../_work/useAsync";
import { pendingRows, readWords } from "./loadModel.mjs";
import {
  aliasesText,
  emptySpec,
  foreignWords,
  profileRow,
  removedWords,
  savedWords,
  slugFor,
  specBody,
  specOf,
  validateSpec,
  versionNote,
} from "./gitProfilesModel.mjs";
import { t } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";

export function GitProfilesPanel() {
  const toast = useToast();
  const view = useAsync((s) => api.gitProfiles(s), []);
  // Side reads for the dialog's pickers: a failure is said on a line, never swallowed.
  const keys = useAsync((s) => api.sshOverview(s), []);
  // Every kind's stored accounts: a profile binds an owner on any of the three hosts.
  const accounts = useAsync((s) => Promise.all(KINDS.map((kind) => api.codeHostAccounts(kind, s))), []);
  useEngineEvents((e) => {
    if (e.payload.type === "git_setup_changed") {
      view.reload();
      keys.reload();
      accounts.reload();
    }
  });
  const [editing, setEditing] = useState<{ slug: string | null; spec: ProfileSpec } | null>(null);
  const [removing, setRemoving] = useState<GitProfileView | null>(null);

  if (view.loading && !view.data) return <Pending what={t("settings-git-profiles-panel-git-profiles")} rows={pendingRows(t("settings-git-profiles-panel-git-profiles"))} />;
  if (view.error && !view.data) return <ErrorNote error={view.error} retry={view.reload} />;
  const data = view.data!;
  const sideWords = [
    keys.error && !keys.data ? { words: readWords({ what: t("settings-git-profiles-panel-ssh-keys"), error: keys.error, data: null }), reload: keys.reload } : null,
    accounts.error && !accounts.data ? { words: readWords({ what: t("settings-git-profiles-panel-stored-accounts"), error: accounts.error, data: null }), reload: accounts.reload } : null,
  ].filter((w): w is { words: { text: string; failed: boolean } | null; reload: () => void } => w !== null);
  const note = versionNote(data);
  const keyPaths = (keys.data?.keys ?? []).map((k) => ({ name: k.name, path: String(k.path) }));
  const logins = [...new Set((accounts.data ?? []).flatMap((view) => [...(view?.accounts ?? []).map((a) => a.login), ...(view?.cli_login ? [view.cli_login] : [])]))];

  const remove = async (p: GitProfileView) => {
    await attempt(() => api.deleteGitProfile(p.slug), toast.error, () => {
      toast.ok(removedWords(p.slug));
      view.reload();
    });
  };

  return (
    <Section
      title={
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <h2 className="text-sm font-semibold text-text">{t("settings-git-profiles-panel-profiles-organization")}</h2>
          <Chip tone={data.profiles.length > 0 ? "ok" : "quiet"} icon={ICON.organization}>
            {data.profiles.length === 0 ? t("settings-git-profiles-panel-none") : `${data.profiles.length}`}
          </Chip>
        </div>
      }
      action={
        <Button size="sm" variant="primary" disabled={!data.hasconfig_supported} onClick={() => setEditing({ slug: null, spec: emptySpec() })}>{t("settings-git-profiles-panel-new-profile")}</Button>
      }
    >
      <Card className="flex flex-col gap-3">
        {sideWords.map((w, n) => (
          <ReadLine key={n} words={w.words} onReload={w.reload} />
        ))}
        <p className="max-w-measure text-2xs leading-relaxed text-text-dim">{rich("settings-git-profiles-panel-profile-who-you-are")}</p>
        {note && (
          <p className="rounded-control border border-warn/40 bg-warn/10 px-2 py-1 text-2xs text-warn" role="alert">
            {note}
          </p>
        )}
        {data.profiles.length > 0 && (
          <ul className="flex flex-col gap-2" aria-label={t("settings-git-profiles-panel-profiles")}>
            {data.profiles.map((p) => {
              const row = profileRow(p);
              return (
                <li key={p.slug} className="rounded-control bg-surface-2/50 p-2">
                  <div className="flex flex-wrap items-center gap-2">
                    <ICON.organization size={13} aria-hidden className="shrink-0 text-text-dim" />
                    <span className="text-xs font-medium">{row.title}</span>
                    <span className="font-mono text-2xs text-text-dim">{row.where}</span>
                    <span className="flex-1" />
                    <Button size="sm" variant="ghost" onClick={() => setEditing({ slug: p.slug, spec: specOf(p) })}>{t("settings-connectors-panel-edit")}</Button>
                    <Button size="sm" variant="ghost" onClick={() => setRemoving(p)}>{t("settings-git-profiles-panel-remove")}</Button>
                  </div>
                  <p className="mt-1 flex flex-wrap gap-x-3 text-2xs text-text-dim">
                    <span className="font-mono">{row.who}</span>
                    {row.key && (
                      <span>{rich("settings-git-profiles-panel-key-slot", { key: <span className="font-mono">{row.key}</span> })}</span>
                    )}
                    {row.account && (
                      <span>{rich("settings-git-profiles-panel-account-slot", { account: <span className="font-mono">{row.account}</span> })}</span>
                    )}
                  </p>
                  <p className="mt-0.5 text-2xs text-text-dim">{row.matches}</p>
                </li>
              );
            })}
          </ul>
        )}
        {data.foreign_includes.length > 0 && (
          <div className="text-2xs text-text-dim">
            <p className="max-w-measure leading-relaxed">{t("settings-git-profiles-panel-global-git-config-also-includes-own")}</p>
            <ul className="mt-1 flex flex-col gap-0.5 font-mono text-2xs">
              {data.foreign_includes.map((f) => (
                <li key={`${f.condition}:${String(f.path)}`}>{foreignWords({ ...f, path: String(f.path) })}</li>
              ))}
            </ul>
          </div>
        )}
        {editing && (
          <ProfileDialog
            slug={editing.slug}
            initial={editing.spec}
            keys={keyPaths}
            logins={logins}
            onClose={() => setEditing(null)}
            onSaved={(p) => {
              toast.ok(savedWords(p));
              setEditing(null);
              view.reload();
            }}
            onError={toast.error}
          />
        )}
        <ConfirmDialog
          open={removing !== null}
          onClose={() => setRemoving(null)}
          onConfirm={() => {
            const p = removing;
            setRemoving(null);
            if (p) void remove(p);
          }}
          title={t("settings-git-profiles-panel-remove-profile", { removing: removing?.label ?? "" })}
          body={
            <>{rich("settings-git-profiles-panel-remove-body", { under: <span className="font-mono">{removing?.host}/{removing?.owner}</span> })}</>
          }
          confirmLabel={t("settings-git-profiles-panel-remove-2")}
          danger
        />
      </Card>
    </Section>
  );
}

function ProfileDialog({
  slug,
  initial,
  keys,
  logins,
  onClose,
  onSaved,
  onError,
}: {
  slug: string | null;
  initial: ProfileSpec;
  keys: { name: string; path: string }[];
  logins: string[];
  onClose: () => void;
  onSaved: (p: GitProfileView) => void;
  onError: (message: string) => void;
}) {
  const [spec, setSpec] = useState<ProfileSpec>(initial);
  const [aliases, setAliases] = useState(aliasesText(initial.aliases));
  const [busy, setBusy] = useState(false);
  const full: ProfileSpec = specBody(spec, aliases);
  const problems = validateSpec(full);
  const targetSlug = slug ?? slugFor(spec.label);
  const ok = Object.keys(problems).length === 0 && !!targetSlug;
  const set = (patch: Partial<ProfileSpec>) => setSpec((s) => ({ ...s, ...patch }));
  const save = async () => {
    if (!ok || busy || !targetSlug) return;
    setBusy(true);
    await attempt(
      () =>
        api.putGitProfile(targetSlug, full),
      onError,
      onSaved,
    );
    setBusy(false);
  };
  const problem = (field: string) => (problems[field] ? <span className="text-danger">{problems[field]}</span> : undefined);
  return (
    <Dialog
      open
      onClose={onClose}
      title={slug ? t("settings-git-profiles-panel-edit-profile", { initial: initial.label }) : t("settings-git-profiles-panel-new-profile-2")}
      description={t("settings-git-profiles-panel-who-one-organization-repositories-commit-author")}
      footer={
        <>
          <Button size="sm" variant="ghost" onClick={onClose}>{t("settings-connectors-panel-cancel")}</Button>
          <Button size="sm" variant="primary" disabled={!ok || busy} onClick={() => void save()}>
            {busy ? t("settings-connectors-panel-saving") : t("settings-git-profiles-panel-save-profile")}
          </Button>
        </>
      }
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
      >
        <Field label={t("settings-git-profiles-panel-name")} hint={problem("label") ?? (targetSlug ? t("settings-git-profiles-panel-saved", { targetSlug }) : t("settings-git-profiles-panel-name-slug-made-from"))}>
          <TextInput value={spec.label} placeholder={t("settings-git-profiles-panel-acme")} autoFocus onChange={(e) => set({ label: e.target.value })} />
        </Field>
        <div className="grid grid-cols-2 gap-3">
          <Field label={t("settings-git-profiles-panel-code-host")} hint={problem("host")}>
            <TextInput value={spec.host} className="font-mono" onChange={(e) => set({ host: e.target.value })} />
          </Field>
          <Field label={t("settings-git-profiles-panel-organization")} hint={problem("owner") ?? t("settings-git-profiles-panel-owner-remote-url-organization-user")}>
            <TextInput value={spec.owner} /* content, never translated */ placeholder="acme" className="font-mono" onChange={(e) => set({ owner: e.target.value })} />
          </Field>
        </div>
        <Field label={t("settings-git-profiles-panel-ssh-aliases")} hint={problem("aliases") ?? t("settings-git-profiles-panel-host-aliases-from-ssh-config-stand")}>
          <TextInput value={aliases} /* for the machine */ placeholder="github-acme" className="font-mono" onChange={(e) => setAliases(e.target.value)} />
        </Field>
        <div className="grid grid-cols-2 gap-3">
          <Field label={t("settings-git-profiles-panel-author-name")} hint={problem("name")}>
            <TextInput value={spec.name} placeholder={t("settings-git-profiles-panel-ada-lovelace")} onChange={(e) => set({ name: e.target.value })} />
          </Field>
          <Field label={t("settings-git-profiles-panel-author-email")} hint={problem("email")}>
            <TextInput value={spec.email} /* content, never translated */ placeholder="ada@acme.example" className="font-mono" onChange={(e) => set({ email: e.target.value })} />
          </Field>
        </div>
        <Field label={t("settings-git-profiles-panel-ssh-key")} hint={problem("ssh_key") ?? t("settings-git-profiles-panel-private-key-push-hands-over-key")}>
          <Select value={spec.ssh_key ?? ""} onChange={(e) => set({ ssh_key: e.target.value || null })}>
            <option value="">{t("settings-git-profiles-panel-whatever-ssh-would-offer")}</option>
            {keys.map((k) => (
              <option key={k.path} value={k.path}>
                {k.name}
              </option>
            ))}
            {spec.ssh_key && !keys.some((k) => k.path === spec.ssh_key) && <option value={spec.ssh_key}>{spec.ssh_key}</option>}
          </Select>
        </Field>
        <Field label={t("settings-git-profiles-panel-code-host-account")} hint={problem("account") ?? t("settings-git-profiles-panel-login-whose-credential-opens-pull-requests")}>
          <Select value={spec.account ?? ""} onChange={(e) => set({ account: e.target.value || null })}>
            <option value="">{t("settings-git-profiles-panel-default-account")}</option>
            {logins.map((l) => (
              <option key={l} value={l}>
                @{l}
              </option>
            ))}
            {spec.account && !logins.includes(spec.account) && <option value={spec.account}>@{spec.account}</option>}
          </Select>
        </Field>
      </form>
    </Dialog>
  );
}
