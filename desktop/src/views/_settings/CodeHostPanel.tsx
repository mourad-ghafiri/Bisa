/**
 * Settings › Git & code hosts › GitHub · GitLab · Bitbucket (ide/08
 * §Connection is a request): one panel, three kinds. What this machine has
 * — the kind's CLI (installed, version, signed in as whom), the accounts this
 * platform holds a token for, git's own credential helper, the default
 * account, who requests go as — and a way in. **Authenticate** runs the
 * CLI's own browser sign-in in a terminal of the desktop shell (the CLI wants
 * a TTY for its one-time code; the argv is the shell's closed table, never a
 * program named here); a kind with no CLI, or a person who prefers it, adds a
 * token from the host's page. Every fact is `GET /codehost/{kind}/health`'s
 * and every sentence `codeHostAccountsModel.mjs`'s; a check is the one
 * request to the host, on a button. The panel is drawn at once with its
 * shape: the probe (`gh --version`, `gh auth status`) fills the connection
 * section when it answers, a re-read says *checking again…* beside the last
 * answer, and the token form is typed into meanwhile (ide/13 §Every panel
 * reads the same way).
 */

import { useEffect, useState } from "react";
import { api, openExternal } from "../../api";
import { useEngineEvents } from "../../bus";
import { canOpenTerminal, openTerminalIn, useTerminals } from "../../shell/useTerminals";
import type { CodeHostConnection, CodeHostKind } from "../../types";
import { Button, Card, Chip, ConfirmDialog, CopyText, ErrorNote, Field, ICON, Pending, ReadLine, SecretInput, Section, Select, TextInput, Tooltip, failureText, useToast } from "../../ui";
import { cliName, hostLabel, prNouns } from "../_work/codeHostWords.mjs";
import { attempt, useAsync } from "../_work/useAsync";
import { pendingRows, phase, readWords } from "./loadModel.mjs";
import {
  accountRows,
  addBlockedWords,
  addedWords,
  checksStillListed,
  cliLine,
  connectionLine,
  defaultLine,
  defaultWords,
  envOverrideLine,
  helpersLine,
  installLines,
  loginWords,
  resolvesLine,
  signInExitKey,
  signInPlan,
  signingInWords,
  storeHint,
  tokenWords,
} from "./codeHostAccountsModel.mjs";
import { t } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";

export function CodeHostPanel({ kind }: { kind: CodeHostKind }) {
  const toast = useToast();
  const label = hostLabel(kind);
  const health = useAsync((s) => api.codeHostHealth(kind, s), [kind]);
  const plan = useAsync((s) => api.codeHostLogin(kind, s), [kind]);
  // The two reloads are stable doors (`useAsync`), so the effects below may list them.
  const reloadHealth = health.reload;
  const reloadPlan = plan.reload;
  useEngineEvents((e) => {
    if (e.payload.type === "git_setup_changed") health.reload();
  });
  // The person comes back from the browser or the terminal: look again.
  useEffect(() => {
    const again = () => {
      reloadHealth();
      reloadPlan();
    };
    window.addEventListener("focus", again);
    return () => window.removeEventListener("focus", again);
  }, [kind, reloadHealth, reloadPlan]);
  // A sign-in terminal for this kind exited: the CLI may hold a credential now.
  const terminals = useTerminals();
  const signInExit = signInExitKey(terminals.sessions, kind);
  useEffect(() => {
    if (signInExit) {
      reloadHealth();
      reloadPlan();
    }
  }, [signInExit, reloadHealth, reloadPlan]);

  // The token stays in its box, hidden, while this panel lives; `sent` is the
  // one the host already checked, so Add waits for a change (ide/13 §Secret fields).
  const [token, setToken] = useState("");
  const [sent, setSent] = useState<string | null>(null);
  const [login, setLogin] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [checks, setChecks] = useState<Record<string, CodeHostConnection>>({});
  const [forgetting, setForgetting] = useState<string | null>(null);
  const words = tokenWords(kind);

  const add = async () => {
    if (!token.trim() || busy || !health.data || token === sent) return;
    if (words.needsLogin && !login.trim()) return;
    setBusy("add");
    await attempt(() => api.addCodeHostAccount(kind, token.trim(), words.needsLogin ? login.trim() : null), toast.error, (made) => {
      setSent(token);
      setLogin("");
      setChecks((c) => ({ ...c, [made.login]: made.connection }));
      toast.ok(addedWords(made.login, made.connection, label));
      health.reload();
    });
    setBusy(null);
  };

  const check = async (who: string) => {
    setBusy(`check:${who}`);
    await attempt(() => api.checkCodeHostAccount(kind, who), toast.error, (c) => setChecks((all) => ({ ...all, [who]: c })));
    setBusy(null);
  };

  const forget = async (who: string) => {
    setBusy(`forget:${who}`);
    await attempt(() => api.forgetCodeHostAccount(kind, who), toast.error, () => {
      toast.ok(t("settings-code-host-panel-forgotten", { who }));
      setChecks((all) => {
        const next = { ...all };
        delete next[who];
        return next;
      });
      health.reload();
    });
    setBusy(null);
  };

  const setDefault = async (who: string | null) => {
    setBusy("default");
    await attempt(() => api.setDefaultCodeHostAccount(kind, who), toast.error, (r) => {
      toast.ok(defaultWords(r.default));
      health.reload();
    });
    setBusy(null);
  };

  /** The CLI's browser sign-in, in a terminal of the shell — the closed table's argv. */
  const authenticate = () => {
    const plan = signInPlan(canOpenTerminal(), kind, label, health.data?.host);
    if (!plan.ok) {
      toast.error(plan.why);
      return;
    }
    openTerminalIn(plan.terminal);
    toast.ok(signingInWords(kind));
  };

  const open = (url: string) => void openExternal(url).catch((e: unknown) => toast.error(failureText("settings", "code-host-panel-failed", e)));

  // The panel is drawn at once with its shape; each read fills the place its
  // answer goes (ide/13 §Every panel reads the same way). `data` is null only
  // while the first probe is on its way or refused — every model line is total
  // over that, and the form's submit waits for the answer it needs.
  const data = health.data;
  const connection = phase(health);
  const rows = accountRows(data);
  // A login that left the list takes its Check line with it.
  const listed = [...rows.map((r) => r.login), ...(data?.cli?.accounts ?? []).map((a) => a.login)].join("\n");
  useEffect(() => {
    setChecks((all) => checksStillListed(all, listed ? listed.split("\n") : []));
  }, [listed]);
  const env = envOverrideLine(data);
  const helpers = helpersLine(data);
  const cli = cliLine(data);
  const resolves = resolvesLine(data);
  const way = loginWords(plan.data);
  const cliWords = cliName(kind);
  const toneClass = (tone: string) => (tone === "ok" ? "text-text-dim" : tone === "danger" ? "text-danger" : tone === "warn" ? "text-warn" : "text-text-dim");
  const probe = t("settings-code-host-panel-cli-3", { cliWords: cliWords?.label ?? t("settings-code-host-panel-code-host") });
  const status = connection === "ready" ? readWords({ what: probe, refreshing: health.refreshing, error: health.error, at: health.at, data }, Date.now() / 1000) : null;
  const blockedAdd = busy !== null ? null : addBlockedWords({ ready: Boolean(data), token, sent, needsLogin: words.needsLogin, login });

  return (
    <div className="flex flex-col gap-6">
      <Section
        title={
          <div className="flex min-w-0 flex-wrap items-center gap-2">
            <h2 className="text-sm font-semibold text-text">{label}</h2>
            {data ? (
              <Chip tone={data.resolves ? "ok" : "warn"} icon={data.resolves ? ICON.ok : ICON.warn}>
                {data.resolves ? t("settings-code-host-panel-connected") : t("settings-code-host-panel-signed")}
              </Chip>
            ) : (
              <Chip tone="quiet">{connection === "failed" ? t("settings-code-host-panel-read") : t("settings-system-permissions-checking")}</Chip>
            )}
          </div>
        }
        action={
          <Tooltip label={t("settings-code-host-panel-read-again-cli-s-status-stored")}>
            <ReadLine words={status} busy={health.loading || health.refreshing} onReload={health.reload} reloadLabel={t("settings-code-host-panel-check-again")} />
          </Tooltip>
        }
      >
        <Card className="flex flex-col gap-3">

          {connection === "pending" && <Pending what={probe} rows={pendingRows(t("settings-code-host-panel-github-cli"))} />}
          {connection === "failed" && <ErrorNote error={health.error ?? t("settings-code-host-panel-connection-check-refused")} retry={health.reload} />}

          {data && (
            <>
              <p className={`text-2xs ${resolves.tone === "ok" ? "text-text" : "text-warn"}`}>{resolves.text}</p>

              {env && (
                <p className="rounded-control border border-warn/40 bg-warn/10 px-2 py-1 text-2xs text-warn" role="status">
                  {env}
                </p>
              )}

              {/* The CLI: what the machine has, and the way in. */}
              <div className="flex flex-col gap-1.5 rounded-control bg-surface-2/50 p-2">
                <div className="flex flex-wrap items-center gap-2">
                  <ICON.shell size={13} aria-hidden className="shrink-0 text-text-dim" />
                  <span className={`min-w-0 flex-1 text-2xs ${toneClass(cli.tone)}`}>{cli.text}</span>
                  {data.cli?.path && <span className="font-mono text-2xs text-text-dim" title={data.cli.path}>{data.cli.path}</span>}
                </div>
                {data.cli?.accounts && data.cli.accounts.length > 0 && (
                  <ul className="flex flex-wrap gap-2 pl-5" aria-label={t("settings-code-host-panel-accounts", { cliWords: cliWords?.label ?? t("settings-code-host-panel-cli-2") })}>
                    {data.cli.accounts.map((a) => (
                      <li key={`${a.host}:${a.login}`} className="flex items-center gap-1">
                        <span className="font-mono text-2xs">@{a.login}</span>
                        {a.active && <Chip tone="neutral">{t("settings-code-host-panel-active")}</Chip>}
                        {data.cli && (
                          <Button size="sm" variant="ghost" disabled={busy !== null} onClick={() => void check(a.login)}>
                            {busy === `check:${a.login}` ? t("settings-code-host-panel-checking") : t("settings-code-host-panel-check")}
                          </Button>
                        )}
                        {checks[a.login] && <span className={`text-2xs ${toneClass(connectionLine(checks[a.login], label).tone)}`}>{connectionLine(checks[a.login], label).text}</span>}
                      </li>
                    ))}
                  </ul>
                )}
                {/* The way in is its own read: the button appears with its words, never before them. */}
                {cli.action !== null && phase(plan) === "pending" && <Pending what={t("settings-load-how-sign")} rows={pendingRows(t("settings-load-how-sign"))} className="pl-5" />}
                {cli.action !== null && phase(plan) === "failed" && <ErrorNote error={plan.error ?? t("settings-code-host-panel-sign-in-plan-refused")} retry={plan.reload} />}
                {cli.action === "signin" && plan.data && (
                  <div className="flex flex-wrap items-center gap-2 pl-5">
                    <Button size="sm" variant="primary" disabled={!data.cli?.installed} disabledReason={data.cli?.installed ? undefined : t("settings-code-host-panel-sign-in-needs-cli")} onClick={authenticate}>
                      <ICON.open size={12} aria-hidden />
                      <span className="ml-1">{way.button}</span>
                    </Button>
                    <span className="text-2xs text-text-dim">{way.blurb}</span>
                  </div>
                )}
                {cli.action === "install" && plan.data?.kind === "install" && (
                  <div className="flex flex-col gap-1 pl-5">
                    <p className="max-w-measure text-2xs leading-relaxed text-text-dim">{way.blurb}</p>
                    <ul className="flex flex-wrap gap-2">
                      {installLines(plan.data.hints).map((l) => (
                        <li key={l.label} className="flex items-center gap-1">
                          <span className="text-2xs text-text-dim">{l.label}</span>
                          <code className="font-mono text-2xs">{l.command}</code>
                          <CopyText value={l.command} label={t("settings-code-host-panel-copy-command", { l: l.label })} />
                        </li>
                      ))}
                    </ul>
                    <Button size="sm" variant="ghost" className="self-start" onClick={() => open(plan.data!.kind === "install" ? plan.data!.hints.url : "")}>
                      <ICON.open size={12} aria-hidden />
                      {t("settings-code-host-panel-cli-install-page", { cli: cliWords?.label ?? t("settings-code-host-panel-cli") })}
                    </Button>
                  </div>
                )}
                {cli.action === null && data.cli?.installed && data.cli.accounts.length > 0 && (
                  <div className="flex flex-wrap items-center gap-2 pl-5">
                    <Button size="sm" variant="ghost" onClick={authenticate}>{t("settings-code-host-panel-sign-another-account")}</Button>
                  </div>
                )}
              </div>

              {rows.length > 0 && (
                <ul className="flex flex-col gap-2" aria-label={t("settings-code-host-panel-stored-accounts")}>
                  {rows.map((row) => {
                    const line = connectionLine(checks[row.login], label);
                    return (
                      <li key={row.login} className="rounded-control bg-surface-2/50 p-2">
                        <div className="flex flex-wrap items-center gap-2">
                          <ICON.account size={13} aria-hidden className="shrink-0 text-text-dim" />
                          <span className="font-mono text-xs">@{row.login}</span>
                          {row.isDefault && <Chip tone="neutral">{t("settings-appearance-panel-default")}</Chip>}
                          <span className="text-2xs text-text-dim">{t("settings-code-host-panel-token", { sourceWords: row.sourceWords })}</span>
                          <span className="flex-1" />
                          <Button size="sm" variant="ghost" disabled={busy !== null} onClick={() => void check(row.login)}>
                            {busy === `check:${row.login}` ? t("settings-code-host-panel-checking") : t("settings-code-host-panel-check")}
                          </Button>
                          {!row.isDefault && (
                            <Button size="sm" variant="ghost" disabled={busy !== null} onClick={() => void setDefault(row.login)}>{t("settings-code-host-panel-make-default")}</Button>
                          )}
                          <Button size="sm" variant="ghost" disabled={busy !== null} onClick={() => setForgetting(row.login)}>{t("settings-code-host-panel-forget")}</Button>
                        </div>
                        {checks[row.login] && <p className={`mt-1 text-2xs ${toneClass(line.tone)}`}>{line.text}</p>}
                      </li>
                    );
                  })}
                </ul>
              )}

              <div className="flex flex-wrap items-end gap-2">
                <Field label={t("settings-code-host-panel-default-account")} hint={defaultLine(data)}>
                  <Select value={data.default ?? ""} disabled={busy !== null || (rows.length === 0 && !data.cli_login)} onChange={(e) => void setDefault(e.target.value || null)} className="w-56">
                    <option value="">{t("settings-code-host-panel-chain-s-own-choice")}</option>
                    {[...new Set([...rows.map((r) => r.login), ...(data.cli_login ? [data.cli_login] : [])])].map((l) => (
                      <option key={l} value={l}>
                        @{l}
                      </option>
                    ))}
                  </Select>
                </Field>
              </div>
              {helpers && <p className={`text-2xs ${helpers.tone === "ok" ? "text-text-dim" : "text-warn"}`}>{helpers.text}</p>}
            </>
          )}
        </Card>
      </Section>

      {/* Drawn at once: a token can be typed while the probe is on its way; adding waits for its answer. */}
      <Section title={t("settings-code-host-panel-add-token")}>
        <Card className="flex flex-col gap-2">
          <form
            className="flex flex-wrap items-end gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              void add();
            }}
          >
            {words.needsLogin && (
              <Field label={t("settings-code-host-panel-login")} hint={t("settings-code-host-panel-login-token-belongs", { label })}>
                <TextInput value={login} /* for the machine */ placeholder="your-login" className="w-40 font-mono" onChange={(e) => setLogin(e.target.value)} />
              </Field>
            )}
            <Field label={t("settings-code-host-panel-token-2")} hint={data ? storeHint(data) : t("settings-code-host-panel-waiting-connection-check")}>
              <SecretInput what={t("ui-secret-input-what-token")} value={token} placeholder={words.placeholder} className="w-80" onChange={setToken} />
            </Field>
            <Button size="sm" type="submit" variant="primary" disabled={!data || busy !== null || !token.trim() || token === sent || (words.needsLogin && !login.trim())} disabledReason={blockedAdd ?? undefined}>
              {busy === "add" ? t("settings-code-host-panel-checking", { label }) : t("settings-code-host-panel-add-account")}
            </Button>
          </form>
          <p className="max-w-measure text-2xs leading-relaxed text-text-dim">
            {words.scopes} {t("settings-code-host-panel-which-account-repository-uses", { prs: prNouns(kind) })}
          </p>
          {data && (
            <Button size="sm" variant="ghost" className="self-start" onClick={() => open(data.token_page)}>
              <ICON.open size={12} aria-hidden />
              {t("settings-code-host-panel-make-one", { label })}
            </Button>
          )}
        </Card>
      </Section>

      <ConfirmDialog
        open={forgetting !== null}
        onClose={() => setForgetting(null)}
        onConfirm={() => {
          const who = forgetting;
          setForgetting(null);
          if (who) void forget(who);
        }}
        title={t("settings-code-host-panel-forget-3", { forgetting: forgetting ?? "" })}
        body={
          <>{rich("settings-code-host-panel-forget-body", { login: <span className="font-mono">@{forgetting}</span> }, { label })}</>
        }
        confirmLabel={t("settings-code-host-panel-forget-2")}
        danger
      />
    </div>
  );
}
