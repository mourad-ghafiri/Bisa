/**
 * Settings › Capabilities › Connectors: every connector installed here — the
 * catalog's and yours — with this machine's accounts for each. An account is
 * a label, the connector's parameters, and secret fields typed once into
 * write-only inputs; the rows say which fields are set and where they live,
 * never a value. Every account row wears the health its last check found —
 * the node's, kept for the engine's lifetime and forgotten when the way in
 * changes — *Check* is one request to the platform as that account, *Check
 * all* every account of every connector, a few at a time, and a check
 * finished anywhere lands on the rows through the bus. *Connect* runs the
 * OAuth2 flow through the browser and the node's loopback callback, with a
 * field to paste the code for a platform that cannot send the browser back.
 * A custom definition is added and edited as JSON and validated on the node
 * before anything is written. Every fact is the node's and every sentence
 * `connectorsModel.mjs`'s or `connectorHealthModel.mjs`'s.
 */

import { useEffect, useMemo, useState } from "react";
import { api, openExternal } from "../../api";
import type { ConnectorAccountRow, ConnectorDefinition, ConnectorRow, ConnectorValidation, SecretField } from "../../types";
import { Button, Card, Chip, ConfirmDialog, Dialog, EmptyState, ErrorNote, Field, ICON, MoreMenu, Pending, SecretInput, SecretTextArea, TextArea, TextInput, Tooltip, failureText, cn, useToast } from "../../ui";
import type { MenuItem } from "../../ui";
import { attempt } from "../_work/useAsync";
import { useResolvedSettings } from "../../shell/settingsStore";
import { useConnectorDetail, useConnectors } from "../_workflow/useConnectors";
import { pendingRows } from "./loadModel.mjs";
import {
  SECRET_FIELD_LABEL,
  accountBody,
  accountDraft,
  accountRows,
  accountVerbs,
  addedWords,
  checkLine,
  connectApplies,
  connectWords,
  connectedWords,
  connectorLine,
  defaultWords,
  definitionOf,
  emptyDefinition,
  forgottenWords,
  isYours,
  maySaveAccount,
  needsAccount,
  oauthLine,
  oauthPort,
  openedWords,
  originWords,
  parseDefinition,
  problemLines,
  saveRefusedWords,
  isMultilineSecret,
  secretFields,
  secretsLine,
  secretsSetWords,
  typedSecrets,
} from "./connectorsModel.mjs";
import { CHECK_ALL_AT_ONCE, checkTargets, healthDetail, healthTone, healthWords } from "./connectorHealthModel.mjs";
import { checkedWords } from "./mcpHealthModel.mjs";
import { t } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";

const toneClass = (tone: string) => (tone === "ok" ? "text-text" : tone === "danger" ? "text-danger" : tone === "warn" ? "text-warn" : "text-text-dim");

type Secrets = Partial<Record<SecretField, string>>;

/**
 * The secret fields of a scheme, each the kit's secret field — hidden as
 * typed, the eye to show it, the mask for one the account already holds
 * (`secrets_set`); a PEM block gets the folded text area.
 */
function SecretFields({ fields, value, storedFields, onChange }: { fields: SecretField[]; value: Secrets; storedFields: readonly SecretField[]; onChange: (next: Secrets) => void }) {
  return (
    <>
      {fields.map((f) => (
        <Field key={f} label={SECRET_FIELD_LABEL[f]} hint={isMultilineSecret(f) ? t("settings-connectors-panel-paste-whole-pem-block-begin-end") : t("settings-connectors-panel-stored-keystore-machine-never-shown-back")}>
          {isMultilineSecret(f) ? (
            <SecretTextArea what={SECRET_FIELD_LABEL[f].toLowerCase()} value={value[f] ?? ""} stored={storedFields.includes(f)} onChange={(v) => onChange({ ...value, [f]: v })} />
          ) : (
            <SecretInput what={SECRET_FIELD_LABEL[f].toLowerCase()} value={value[f] ?? ""} stored={storedFields.includes(f)} onChange={(v) => onChange({ ...value, [f]: v })} />
          )}
        </Field>
      ))}
    </>
  );
}

/** Add or edit an account: the label, the connector's parameters, the secrets. */
function AccountDialog({
  connector,
  account,
  onClose,
  onSaved,
}: {
  connector: ConnectorRow;
  /** Editing this account; adding one when null. */
  account: ConnectorAccountRow | null;
  onClose: () => void;
  onSaved: (row: ConnectorAccountRow) => void;
}) {
  const toast = useToast();
  const detail = useConnectorDetail(connector.id);
  const params = detail.data?.connector.params ?? [];
  const [label, setLabel] = useState(() => accountDraft(account).label);
  const [values, setValues] = useState<Record<string, string>>(() => accountDraft(account).values);
  const [secrets, setSecrets] = useState<Secrets>({});
  const [busy, setBusy] = useState(false);
  const fields = secretFields(connector.auth);
  const save = async () => {
    setBusy(true);
    const body = accountBody({ label, values, secrets }, account);
    await attempt(
      () => api.putConnectorAccount(connector.id, body),
      toast.error,
      (row) => {
        toast.ok(account ? secretsSetWords(Object.keys(typedSecrets(secrets))) : addedWords(row.label));
        onSaved(row);
      },
    );
    setBusy(false);
  };
  return (
    <Dialog open onClose={() => !busy && onClose()} title={account ? t("settings-connectors-panel-edit-2", { account: account.label }) : t("settings-connectors-panel-add-account", { connector: connector.name })} width="max-w-md"
      footer={
        <>
          <Button variant="ghost" disabled={busy} onClick={onClose}>{t("settings-connectors-panel-cancel")}</Button>
          <Button variant="primary" disabled={!maySaveAccount({ label, busy, definitionRead: detail.data !== null })} onClick={() => void save()}>{busy ? t("settings-connectors-panel-saving") : account ? t("settings-decisions-panel-save") : t("settings-code-host-panel-add-account")}</Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <Field label={t("settings-connectors-panel-label")} hint={t("settings-connectors-panel-how-account-named-here-work-personal")}>
          <TextInput value={label} autoFocus onChange={(e) => setLabel(e.target.value)} />
        </Field>
        {/* The parameters the connector asks for are its definition's: until it is read the form cannot ask, and does not save. */}
        {detail.error && <ErrorNote error={detail.error} retry={detail.reload} />}
        {!detail.data && !detail.error && <Pending what={t("settings-connectors-panel-definition")} rows={pendingRows(t("settings-connectors-panel-definition"))} />}
        {params.map((p) => (
          <Field key={p.name} label={p.label || p.name} hint={p.doc || (p.required ? t("settings-connectors-panel-required") : undefined)}>
            <TextInput className="font-mono" value={values[p.name] ?? ""} onChange={(e) => setValues({ ...values, [p.name]: e.target.value })} />
          </Field>
        ))}
        {fields.length > 0 && <SecretFields fields={fields} value={secrets} storedFields={account?.secrets_set ?? []} onChange={setSecrets} />}
        {account && fields.length > 0 && <p className="text-2xs text-text-dim">{t("settings-connectors-panel-field-left-blank-keeps-stored")}</p>}
      </div>
    </Dialog>
  );
}

/** A custom definition, typed as JSON, validated on the node before it is written. */
function DefinitionDialog({ initial, onClose, onSaved }: { initial: ConnectorDefinition | null; onClose: () => void; onSaved: () => void }) {
  const toast = useToast();
  const [text, setText] = useState(() => JSON.stringify(initial ?? emptyDefinition(), null, 2));
  const [validation, setValidation] = useState<ConnectorValidation | null>(null);
  const [parseError, setParseError] = useState<string | null>(null);
  const [busy, setBusy] = useState<"validate" | "save" | null>(null);
  // The rules a definition is held to are reference for the one writing it: folded under one line, never a wall over the editor.
  const [rulesOpen, setRulesOpen] = useState(false);
  const parsed = () => {
    const p = parseDefinition(text);
    if (!p.ok) {
      setParseError(p.error);
      return null;
    }
    setParseError(null);
    return p.value as unknown as ConnectorDefinition;
  };
  const validate = async () => {
    const def = parsed();
    if (!def) return;
    setBusy("validate");
    await attempt(() => api.validateConnector(def), toast.error, setValidation);
    setBusy(null);
  };
  // Nothing is written before the node has read it: Save validates first,
  // shows what it found, and writes only a definition with no problem.
  const save = async () => {
    const def = parsed();
    if (!def) return;
    setBusy("save");
    const seen: { validation: ConnectorValidation | null } = { validation: null };
    await attempt(() => api.validateConnector(def), toast.error, (v) => {
      seen.validation = v;
      setValidation(v);
    });
    const found = problemLines(seen.validation);
    if (!seen.validation || found.length > 0) {
      if (seen.validation) toast.error(saveRefusedWords(found.length));
      setBusy(null);
      return;
    }
    await attempt(() => (initial ? api.updateConnector(initial.id, def) : api.createConnector(def)), toast.error, () => {
      toast.ok(initial ? t("settings-connectors-panel-saved", { def: def.name }) : t("settings-connectors-panel-added-add-account", { def: def.name }));
      onSaved();
    });
    setBusy(null);
  };
  const problems = problemLines(validation);
  return (
    <Dialog open onClose={() => !busy && onClose()} title={initial ? t("settings-connectors-panel-edit-3", { initial: initial.name }) : t("settings-connectors-panel-add-connector")} width="max-w-2xl"
      footer={
        <>
          <Button variant="ghost" disabled={busy !== null} onClick={onClose}>{t("settings-connectors-panel-cancel")}</Button>
          <Button disabled={busy !== null} onClick={() => void validate()}>{busy === "validate" ? t("settings-connectors-panel-validating") : t("settings-connectors-panel-validate")}</Button>
          <Button variant="primary" disabled={busy !== null} onClick={() => void save()}>{busy === "save" ? t("settings-connectors-panel-saving") : initial ? t("settings-decisions-panel-save") : t("settings-mobile-development-panel-create")}</Button>
        </>
      }
    >
      <div className="flex flex-col gap-2">
        <p className="text-2xs text-text-dim">{t("settings-connectors-panel-shape-lead")}</p>
        <button type="button" aria-expanded={rulesOpen} onClick={() => setRulesOpen((o) => !o)} className="anim flex w-fit items-center gap-1 rounded-control text-2xs font-medium text-text-dim hover:text-text">
          <ICON.collapsed size={11} aria-hidden className={cn("anim shrink-0", rulesOpen && "rotate-90")} />
          {t("settings-connectors-panel-shape-rules")}
        </button>
        {rulesOpen && <p className="max-w-measure pl-4 text-2xs leading-relaxed text-text-dim">
          {/* for the machine: the two placeholders a definition's templates take, as they are typed */}
          {rich("settings-connectors-panel-shape-blurb", { code: (inner) => <code className="font-mono">{inner}</code>, params: <code className="font-mono">{"{params.<name>}"}</code>, account: <code className="font-mono">{"{account.<name>}"}</code> })}
        </p>}
        <Field label={t("settings-connectors-panel-definition-json")}>
          <TextArea rows={22} className="font-mono text-2xs" value={text} spellCheck={false} onChange={(e) => setText(e.target.value)} />
        </Field>
        {parseError && <p className="text-2xs text-danger">{t("settings-connectors-panel-json", { parseError })}</p>}
        {validation && problems.length === 0 && <p className="text-2xs text-text">{t("settings-connectors-panel-valid")}</p>}
        {problems.length > 0 && (
          <ul className="list-disc pl-4 text-2xs text-danger">
            {problems.map((l) => (
              <li key={l}>{l}</li>
            ))}
          </ul>
        )}
      </div>
    </Dialog>
  );
}

function ConnectorCard({
  row,
  port,
  onChanged,
  checking,
  onCheck,
}: {
  row: ConnectorRow;
  port: number | null;
  onChanged: () => void;
  /** The accounts a check is running for right now — this card's or *Check all*'s. */
  checking: ReadonlySet<string>;
  onCheck: (account: ConnectorAccountRow) => void;
}) {
  const toast = useToast();
  const detail = useConnectorDetail(row.id);
  const accounts = useMemo(() => accountRows(detail.data?.accounts), [detail.data]);
  const [busy, setBusy] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  const [editing, setEditing] = useState<ConnectorAccountRow | null>(null);
  const [forgetting, setForgetting] = useState<ConnectorAccountRow | null>(null);
  const [editingDef, setEditingDef] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [pending, setPending] = useState<{ account: string; code: string } | null>(null);
  const now = Math.floor(Date.now() / 1000);
  const custom = isYours(row.origin);
  const oauth = connectApplies(row.auth);

  const setDefault = async (a: ConnectorAccountRow) => {
    setBusy(`default:${a.id}`);
    await attempt(() => api.setDefaultConnectorAccount(row.id, a.id), toast.error, () => {
      toast.ok(defaultWords(a.label));
      detail.reload();
      onChanged();
    });
    setBusy(null);
  };
  const forget = async (a: ConnectorAccountRow) => {
    setBusy(`forget:${a.id}`);
    await attempt(() => api.deleteConnectorAccount(row.id, a.id), toast.error, () => {
      toast.ok(forgottenWords(a.label));
      detail.reload();
      onChanged();
    });
    setBusy(null);
  };
  const connect = async (a: ConnectorAccountRow) => {
    setBusy(`connect:${a.id}`);
    await attempt(() => api.startConnectorOauth(row.id, a.id), toast.error, (start) => {
      setPending({ account: a.id, code: "" });
      toast.ok(openedWords());
      void openExternal(start.url).catch((e: unknown) => toast.error(failureText("settings", "connectors-panel-failed", e)));
    });
    setBusy(null);
  };
  const paste = async () => {
    if (!pending || !pending.code.trim()) return;
    setBusy(`paste:${pending.account}`);
    const label = accounts.find((a) => a.id === pending.account)?.label ?? t("settings-connectors-panel-account");
    await attempt(() => api.completeConnectorOauth(row.id, pending.account, pending.code.trim()), toast.error, () => {
      toast.ok(connectedWords(label));
      // What *Check* said was about the tokens held before this connection: the node forgets it.
      setPending(null);
      detail.reload();
    });
    setBusy(null);
  };
  const remove = async () => {
    setBusy("delete");
    await attempt(() => api.deleteConnector(row.id), toast.error, () => {
      toast.ok(t("settings-connectors-panel-removed", { row: row.name }));
      onChanged();
    });
    setBusy(null);
  };
  // A connection that landed through the callback clears the paste field.
  useEffect(() => {
    if (!pending) return;
    const a = accounts.find((x) => x.id === pending.account);
    if (a?.secrets_set.includes("access_token")) setPending(null);
  }, [accounts, pending]);

  return (
    <Card className="flex flex-col gap-2 p-3">
      <div className="flex flex-wrap items-center gap-2">
        <ICON.connector size={14} aria-hidden className="shrink-0 text-text-dim" />
        <h3 className="text-sm font-semibold text-text">{row.name}</h3>
        <code className="font-mono text-2xs text-text-dim">{row.id}</code>
        <Chip tone="neutral">{originWords(row.origin)}</Chip>
        <span className="flex-1" />
        {custom && (
          <>
            <Button size="sm" variant="ghost" disabled={busy !== null} onClick={() => setEditingDef(true)}>{t("settings-connectors-panel-edit")}</Button>
            <Button size="sm" variant="ghost" disabled={busy !== null} onClick={() => setDeleting(true)}>{t("settings-connectors-panel-delete")}</Button>
          </>
        )}
        {needsAccount(row.auth) && (
          <Button size="sm" disabled={busy !== null} onClick={() => setAdding(true)}>
            <ICON.add size={12} aria-hidden />{t("settings-code-host-panel-add-account")}</Button>
        )}
      </div>
      <p className="max-w-measure text-2xs leading-relaxed text-text-dim">{row.description}</p>
      <p className="text-2xs text-text-dim">{connectorLine(row)}</p>
      {detail.error && <ErrorNote error={detail.error} retry={detail.reload} />}
      {!needsAccount(row.auth) && <p className="text-2xs text-text-dim">{t("settings-connectors-panel-connector-needs-account-operations-called-they")}</p>}
      {needsAccount(row.auth) && !detail.data && !detail.error && <Pending what={t("settings-connectors-panel-accounts-2")} rows={pendingRows(t("settings-connectors-panel-accounts-2"))} />}
      {needsAccount(row.auth) && accounts.length === 0 && detail.data && <p className="text-2xs text-warn">{t("settings-connectors-panel-account-here-yet-step-calling-cannot")}</p>}
      {accounts.length > 0 && (
        <ul className="flex flex-col gap-2" aria-label={t("settings-connectors-panel-accounts", { row: row.name })}>
          {accounts.map((a) => {
            const secrets = secretsLine(a, row.auth);
            const expiry = oauthLine(a, now);
            // One verb on the row, the rest behind its menu (`accountVerbs`); Forget still asks first.
            const verbs = accountVerbs(a, oauth);
            const isChecking = checking.has(a.id);
            const idle = busy !== null || isChecking;
            const menu: Record<(typeof verbs.more)[number], MenuItem> = {
              connect: { label: t("settings-connectors-panel-connect-again"), icon: ICON.open, disabled: idle || !a.secrets_set.includes("client_id"), onSelect: () => void connect(a) },
              secrets: { label: t("settings-connectors-panel-set-secrets"), disabled: idle, onSelect: () => setEditing(a) },
              check: { label: t("settings-code-host-panel-check"), disabled: idle, onSelect: () => onCheck(a) },
              default: { label: t("settings-code-host-panel-make-default"), disabled: idle, onSelect: () => void setDefault(a) },
              forget: { label: t("settings-code-host-panel-forget"), danger: true, separatorBefore: true, disabled: idle, onSelect: () => setForgetting(a) },
            };
            const detailLine = healthDetail(a.health);
            return (
              <li key={a.id} className="rounded-control bg-surface-2/50 p-2">
                <div className="flex flex-wrap items-center gap-2">
                  <ICON.account size={13} aria-hidden className="shrink-0 text-text-dim" />
                  <span className="text-xs font-medium">{a.label}</span>
                  {a.default && <Chip tone="neutral">{t("settings-appearance-panel-default")}</Chip>}
                  {/* The health the node holds: what the last check found and when; *checking…* while one runs. */}
                  <Tooltip label={[healthWords(a.health, row.name), checkedWords(a.health.checked_at)].filter(Boolean).join(" · ")}>
                    <span>
                      <Chip tone={isChecking ? "quiet" : healthTone(a.health)}>{isChecking ? t("settings-system-permissions-checking") : healthWords(a.health, row.name)}</Chip>
                    </span>
                  </Tooltip>
                  <span className={`text-2xs ${toneClass(secrets.tone)}`}>{secrets.text}</span>
                  <span className="flex-1" />
                  {verbs.main === "connect" ? (
                    <Button
                      size="sm"
                      disabled={busy !== null || !a.secrets_set.includes("client_id")}
                      disabledReason={a.secrets_set.includes("client_id") ? undefined : t("settings-connectors-panel-set-oauth-client-id-first")}
                      onClick={() => void connect(a)}
                    >
                      <ICON.open size={12} aria-hidden />
                      {busy === `connect:${a.id}` ? t("settings-connectors-panel-opening") : t("settings-connectors-panel-connect")}
                    </Button>
                  ) : (
                    <Button size="sm" variant="ghost" disabled={idle} onClick={() => onCheck(a)}>{isChecking ? t("settings-mcp-panel-checking-2") : t("settings-code-host-panel-check")}</Button>
                  )}
                  <MoreMenu label={t("settings-connectors-panel-more-account", { account: a.label })} items={verbs.more.map((verb) => menu[verb])} />
                </div>
                {Object.keys(a.params).length > 0 && (
                  <p className="mt-1 font-mono text-2xs text-text-dim">
                    {Object.entries(a.params).map(([k, v]) => `${k}=${typeof v === "string" ? v : JSON.stringify(v)}`).join(" · ")}
                  </p>
                )}
                {expiry && <p className="mt-1 text-2xs text-text-dim">{expiry}</p>}
                {detailLine && <p className={`mt-1 text-2xs ${toneClass(healthTone(a.health))}`}>{detailLine}</p>}
                {/* Announced: the box goes away on its own when the browser's callback lands. */}
                <span className="sr-only" aria-live="polite">
                  {pending?.account === a.id ? t("settings-connectors-panel-waiting-platform-s-code-browser-s") : a.secrets_set.includes("access_token") ? t("settings-connectors-panel-connected") : ""}
                </span>
                {pending?.account === a.id && (
                  <form
                    className="mt-1.5 flex flex-wrap items-end gap-2"
                    onSubmit={(e) => {
                      e.preventDefault();
                      void paste();
                    }}
                  >
                    <Field label={t("settings-connectors-panel-paste-code")} hint={t("settings-connectors-panel-platform-cannot-send-browser-back-machine")}>
                      <TextInput className="w-72 font-mono" value={pending.code} onChange={(e) => setPending({ ...pending, code: e.target.value })} />
                    </Field>
                    <Button size="sm" type="submit" disabled={busy !== null || !pending.code.trim()}>{busy === `paste:${a.id}` ? t("settings-connectors-panel-connecting") : t("settings-connectors-panel-connect-code")}</Button>
                    <Button size="sm" variant="ghost" disabled={busy !== null} onClick={() => setPending(null)}>{t("settings-connectors-panel-cancel")}</Button>
                  </form>
                )}
              </li>
            );
          })}
        </ul>
      )}
      {/* The redirect URI is copied into another platform's console: said with the port the node resolves, never a guessed one. */}
      {oauth && (port === null ? <Pending what={t("settings-connectors-panel-callback-port")} rows={pendingRows(t("settings-connectors-panel-callback-port"))} /> : <p className="max-w-measure text-2xs leading-relaxed text-text-dim">{connectWords(port)}</p>)}
      {adding && <AccountDialog connector={row} account={null} onClose={() => setAdding(false)} onSaved={() => { setAdding(false); detail.reload(); onChanged(); }} />}
      {editing && <AccountDialog connector={row} account={editing} onClose={() => setEditing(null)} onSaved={() => { setEditing(null); detail.reload(); }} />}
      {editingDef && detail.data && (
        <DefinitionDialog
          initial={definitionOf(detail.data.connector)}
          onClose={() => setEditingDef(false)}
          onSaved={() => { setEditingDef(false); detail.reload(); onChanged(); }}
        />
      )}
      <ConfirmDialog
        open={forgetting !== null}
        onClose={() => setForgetting(null)}
        onConfirm={() => {
          const a = forgetting;
          setForgetting(null);
          if (a) void forget(a);
        }}
        title={t("settings-connectors-panel-forget", { forgetting: forgetting?.label ?? "" })}
        body={<>{t("settings-connectors-panel-forget-body", { name: row.name })}</>}
        confirmLabel={t("settings-code-host-panel-forget-2")}
        danger
      />
      <ConfirmDialog
        open={deleting}
        onClose={() => setDeleting(false)}
        onConfirm={() => {
          setDeleting(false);
          void remove();
        }}
        title={t("settings-connectors-panel-delete-2", { row: row.name })}
        body={<>{t("settings-connectors-panel-delete-body")}</>}
        confirmLabel={t("settings-connectors-panel-delete-3")}
        danger
      />
    </Card>
  );
}

export function ConnectorsPanel() {
  const connectors = useConnectors();
  const toast = useToast();
  const [adding, setAdding] = useState(false);
  // The accounts a check is running for: a row's own *Check*, or *Check all*'s pass over every connector.
  const [checking, setChecking] = useState<ReadonlySet<string>>(new Set());
  /**
   * One request as the account; the node keeps the answer and the rows read
   * it back through the bus. A row's own check says its answer in a toast
   * too; *Check all* stays quiet and lets the chips speak.
   */
  const check = async (cid: string, aid: string, name: string, say: boolean) => {
    setChecking((s) => new Set(s).add(aid));
    await attempt(
      () => api.checkConnectorAccount(cid, aid),
      toast.error,
      (c) => {
        if (!say) return;
        const line = checkLine(c, name);
        (line.tone === "ok" ? toast.ok : toast.error)(line.text);
      },
    );
    setChecking((s) => {
      const next = new Set(s);
      next.delete(aid);
      return next;
    });
  };
  /** Every account that can answer, of every connector that names a check, a few at a time. */
  const checkAll = async () => {
    const queue: { connector: string; account: string; name: string }[] = [];
    for (const row of connectors.rows) {
      if (row.accounts === 0 || !row.check) continue;
      const detail = await api.connector(row.id).catch(() => null);
      if (detail) queue.push(...checkTargets(detail).map((target) => ({ ...target, name: row.name })));
    }
    const workers = Array.from({ length: Math.min(CHECK_ALL_AT_ONCE, queue.length) }, async () => {
      for (let next = queue.shift(); next; next = queue.shift()) await check(next.connector, next.account, next.name, false);
    });
    await Promise.all(workers);
  };
  const checkable = connectors.rows.some((row) => row.accounts > 0 && Boolean(row.check));
  // The callback port is a setting: the resolved value the settings store
  // keeps — read again on every `settings_changed` and when the node comes
  // back — so the redirect URI a person copies names the port in force, and
  // nothing names one before it is read.
  const port = oauthPort(useResolvedSettings(null).resolved);
  // Back from the browser: look again. The reload is a stable door (`useAsync`).
  const reloadConnectors = connectors.reload;
  useEffect(() => {
    const again = () => reloadConnectors();
    window.addEventListener("focus", again);
    return () => window.removeEventListener("focus", again);
  }, [reloadConnectors]);

  if (connectors.loading && connectors.rows.length === 0) return <Pending what={t("settings-connectors-panel-connectors")} rows={pendingRows(t("settings-connectors-panel-connectors"))} />;
  if (connectors.error && connectors.rows.length === 0) return <ErrorNote error={connectors.error} retry={connectors.reload} />;

  const empty = connectors.rows.length === 0;
  const readAgain = (
    <Tooltip label={t("settings-connectors-panel-read-definitions-machine-s-accounts-again")}>
      <Button size="sm" variant="ghost" disabled={connectors.loading} onClick={connectors.reload} aria-label={t("settings-connectors-panel-read-again")}>
        <ICON.refresh size={12} aria-hidden />
      </Button>
    </Tooltip>
  );
  return (
    <div className="flex flex-col gap-3">
      {/* The panel's own line above says what syncs and what stays; this row only adds where more come from. */}
      {!empty && (
        <div className="flex flex-wrap items-center gap-2">
          <p className="text-2xs text-text-dim">{t("settings-connectors-panel-install-more-from-library")}</p>
          <span className="flex-1" />
          <Tooltip label={t("settings-connectors-panel-check-every-account")}>
            <span>
              <Button size="sm" variant="ghost" disabled={!checkable || checking.size > 0} onClick={() => void checkAll()}>
                {checking.size > 0 ? t("settings-connectors-panel-checking", { checking: checking.size }) : t("settings-connectors-panel-check-all")}
              </Button>
            </span>
          </Tooltip>
          {readAgain}
          <Button size="sm" onClick={() => setAdding(true)}>
            <ICON.add size={12} aria-hidden />{t("settings-connectors-panel-add-connector")}</Button>
        </div>
      )}
      {empty && (
        <EmptyState
          icon={ICON.connector}
          title={t("settings-connectors-panel-no-connectors")}
          hint={t("settings-connectors-panel-nothing-installed-here-yet")}
          action={
            // Read again sits beside the door: one installed from the Library shows up without leaving.
            <span className="inline-flex items-center gap-1">
              <Button variant="primary" onClick={() => setAdding(true)}>
                <ICON.add size={12} aria-hidden />{t("settings-connectors-panel-add-connector")}</Button>
              <Button variant="ghost" disabled={connectors.loading} onClick={connectors.reload}>
                <ICON.refresh size={12} aria-hidden />{t("settings-connectors-panel-read-again")}</Button>
            </span>
          }
        />
      )}
      {connectors.rows.map((row) => (
        <ConnectorCard key={row.id} row={row} port={port} onChanged={connectors.reload} checking={checking} onCheck={(a) => void check(row.id, a.id, row.name, true)} />
      ))}
      {adding && <DefinitionDialog initial={null} onClose={() => setAdding(false)} onSaved={() => { setAdding(false); connectors.reload(); }} />}
    </div>
  );
}
