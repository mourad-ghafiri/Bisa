/**
 * About › Checkout's first card (ide/04 §The repository under About › Settings): what this
 * checkout will use when it talks to its remote — the remote and its
 * protocol, the profile it falls under, who commits and where that comes
 * from, the transport (the key ssh would offer and whether ssh-agent holds it;
 * git's helper over HTTPS), the account the code host will be asked as — and
 * the cautions, each with a door to the settings tab that fixes it. *Check*
 * runs the three read-only probes. The account pin is a draft the view's
 * toolbar saves, like everything else on the view. The facts are the node's
 * (`GET /workstreams/{wid}/git/connection`); the words are `connectionModel.mjs`'s.
 */

import { useState } from "react";
import { api } from "../../api";
import type { ConnectionCheck } from "../../types";
import { Button, Chip, CopyText, ErrorNote, ICON, Select, Skeleton, Tooltip, useToast } from "../../ui";
import { SettingsLink } from "../_settings/SettingsTabLink";
import { hostLabel, settingsTabFor } from "./codeHostWords.mjs";
import { accountRow, cautionMeta, checkLines, identityRow, profileRow, remoteRow, transportRow } from "./connectionModel.mjs";
import { attempt } from "./useAsync";
import type { ProjectSettingsDraft } from "./useProjectSettingsDraft";
import { t } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";

export function ConnectionCard({ wid, draft }: { wid: string; draft: ProjectSettingsDraft }) {
  const toast = useToast();
  const [check, setCheck] = useState<ConnectionCheck | null>(null);
  const [busy, setBusy] = useState<"check" | null>(null);

  if (!draft.connection && draft.loading) return <Skeleton className="h-28 w-full" />;
  if (!draft.connection) return <ErrorNote error={draft.error ?? t("work-connection-card-connection-could-not-read")} retry={draft.reload} />;
  const c = draft.connection;
  // The pin as drafted, else as the node holds it.
  const pinned = draft.draft.account !== undefined ? draft.draft.account : c.account.source === "local" ? (c.account.login ?? null) : null;
  const remote = remoteRow(c.remote);
  const profile = profileRow(c.profile);
  const identity = identityRow(c.identity, c.profile);
  const transport = transportRow(c.transport);
  const account = accountRow(c.account, c.code_host ?? null, c.profile);
  // What a pin may name: every stored account, and the login the CLI or git's
  // helper answered with — the one already speaking for this machine.
  const pinnable = [...new Set([...c.account.stored, ...((c.account.source === "cli" || c.account.source === "git_helper") && c.account.login ? [c.account.login] : [])])];

  const runCheck = async () => {
    setBusy("check");
    await attempt(() => api.checkGitConnection(wid), toast.error, setCheck);
    setBusy(null);
  };
  const toneClass = (tone: string) => (tone === "ok" ? "text-text" : tone === "warn" ? "text-warn" : tone === "danger" ? "text-danger" : "text-text-dim");

  if (!remote) {
    return (
      <div className="flex flex-col gap-2 text-2xs text-text-dim">
        <p>{rich("work-connection-card-no-remote-yet", { code: (inner) => <span className="font-mono">{inner}</span> })}</p>
        <Row label={t("work-connection-card-who-commits")} tone={identity.tone}>
          {identity.who ? <span className="font-mono">{identity.who}</span> : null} <span className="text-text-dim">{identity.origin}</span>
        </Row>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-2">
      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1.5 text-2xs">
        <Row label={t("work-connection-card-remote")} tone="ok">
          <span className="font-mono">{remote.summary}</span>
          <Chip tone={remote.transport === "ssh" ? "accent" : remote.transport === "https" ? "neutral" : "quiet"}>{remote.protocol}</Chip>
          {c.code_host && <Chip tone="quiet">{hostLabel(c.code_host)}</Chip>}
          <CopyText value={remote.url} label={t("work-connection-card-copy-url")} />
          {remote.note && <span className="text-text-dim">{remote.note}</span>}
        </Row>
        <Row label={t("work-connection-card-profile")} tone={profile.tone}>
          {c.profile ? <ICON.organization size={12} aria-hidden className="text-text-dim" /> : null}
          <span>{profile.text}</span>
          <SettingsLink tab="git">{c.profile ? t("work-connection-card-edit-profiles") : t("work-connection-card-profiles")}</SettingsLink>
        </Row>
        <Row label={t("work-connection-card-who-commits")} tone={identity.tone}>
          {identity.who ? <span className="font-mono">{identity.who}</span> : null}
          <span className="text-text-dim">{identity.origin}</span>
        </Row>
        <Row label={t("work-connection-card-transport")} tone={transport.tone}>
          <span title={transport.detail ?? undefined}>{transport.text}</span>
          {remote.transport === "ssh" && <SettingsLink tab="git-ssh">{t("work-connection-card-ssh-keys")}</SettingsLink>}
        </Row>
        {c.code_host && (
          <Row label={t("work-connection-card-account")} tone={account.tone}>
            {account.login ? <span className="font-mono">{account.login}</span> : null}
            <span className="text-text-dim">{account.source}</span>
            {pinnable.length > 0 && !c.account.env_override && (
              <Select aria-label={t("work-connection-card-pin-repository-account")} value={pinned ?? ""} className="w-44" onChange={(e) => draft.editAccount(e.target.value || null)}>
                <option value="">{pinned ? t("work-connection-card-unpin") : t("work-connection-card-pin-account")}</option>
                {pinnable.map((l) => (
                  <option key={l} value={l}>
                    @{l}
                  </option>
                ))}
              </Select>
            )}
            {draft.draft.account !== undefined && draft.draft.account !== draft.current?.account && <Chip tone="accent">{t("work-connection-card-changed")}</Chip>}
            <SettingsLink tab={settingsTabFor(c.code_host) ?? "github"}>{hostLabel(c.code_host)}</SettingsLink>
          </Row>
        )}
      </dl>
      {c.cautions.length > 0 && (
        <ul className="flex flex-col gap-1" aria-label={t("work-connection-card-cautions")}>
          {c.cautions.map((caution) => {
            const meta = cautionMeta(caution.id, c.code_host ?? null);
            return (
              <li key={caution.id} className={`flex flex-wrap items-start gap-2 rounded-control border px-2 py-1 text-2xs ${meta.tone === "danger" ? "border-danger/40 bg-danger/10 text-danger" : "border-warn/40 bg-warn/10 text-warn"}`}>
                <ICON.warn size={12} aria-hidden className="mt-0.5 shrink-0" />
                <span className="min-w-0 flex-1">{caution.sentence}</span>
                {meta.tab && <SettingsLink tab={meta.tab}>{t("work-connection-card-fix-settings")}</SettingsLink>}
              </li>
            );
          })}
        </ul>
      )}
      <div className="flex flex-wrap items-center gap-2">
        <Tooltip label={t("work-connection-card-three-read-only-probes-code-host")}>
          <Button size="sm" variant="ghost" disabled={busy !== null} onClick={() => void runCheck()}>
            <ICON.refresh size={12} aria-hidden />
            <span className="ml-1">{busy === "check" ? t("work-connection-card-checking") : t("work-connection-card-check-connection")}</span>
          </Button>
        </Tooltip>
        {check && (
          <ul className="flex flex-col gap-0.5 text-2xs" aria-label={t("work-connection-card-check-results")}>
            {checkLines(check).map((line) => (
              <li key={line.text} className={toneClass(line.tone)}>
                {line.text}
              </li>
            ))}
            {checkLines(check).length === 0 && <li className="text-text-dim">{t("work-connection-card-nothing-probe-no-code-host-no")}</li>}
          </ul>
        )}
      </div>
    </div>
  );
}

function Row({ label, tone, children }: { label: string; tone: string; children: React.ReactNode }) {
  const color = tone === "warn" ? "text-warn" : tone === "danger" ? "text-danger" : tone === "quiet" ? "text-text-dim" : "text-text";
  return (
    <>
      <dt className="text-text-dim">{label}</dt>
      <dd className={`flex min-w-0 flex-wrap items-center gap-2 ${color}`}>{children}</dd>
    </>
  );
}
