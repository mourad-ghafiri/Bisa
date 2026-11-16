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
 * before anything is written. The list is searched, and a card is a line
 * until it is opened — its detail read then, never before — so forty-five
 * definitions are a list to scan and a token refresh re-reads only what is
 * open. Every fact is the node's and every sentence
 * `connectorsModel.mjs`'s or `connectorHealthModel.mjs`'s.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { api, openExternal } from "../../api";
import type { AccountCheck, ConnectorAccountRow, ConnectorDefinition, ConnectorParamDef, ConnectorRow, ConnectorValidation, SecretField } from "../../types";
import { Button, Card, Chip, ConfirmDialog, Dialog, EmptyState, ErrorNote, Field, ICON, MoreMenu, Pending, SecretInput, SecretTextArea, TextArea, TextInput, Tooltip, failureText, cn, useToast } from "../../ui";
import type { MenuItem } from "../../ui";
import { attempt } from "../_work/useAsync";
import { useResolvedSettings } from "../../shell/settingsStore";
import { useConnectorDetail, useConnectors } from "../_workflow/useConnectors";
import { pendingRows } from "./loadModel.mjs";
import {
  SECRET_FIELD_LABEL,
  SECRET_FIELD_WHAT,
  accountBody,
  accountDraft,
  accountRows,
  accountVerbs,
  addedWords,
  checkLine,
  connectApplies,
  connectUriWords,
  connectWords,
  connectedWords,
  connectorFacts,
  defaultWords,
  definitionOf,
  emptyDefinition,
  filterConnectors,
  flowWords,
  flowsSettled,
  forgottenWords,
  isYours,
  mayEditDefinition,
  mayPaste,
  maySaveAccount,
  missingRequired,
  needsAccount,
  oauthLine,
  oauthPort,
  openedWords,
  originWords,
  parseDefinition,
  pendingFlow,
  problemLines,
  renamedProblem,
  saveRefusedWords,
  savedWords,
  isMultilineSecret,
  secretFields,
  secretsLine,
  startsOpen,
  strayParams,
  validationFor,
} from "./connectorsModel.mjs";
import type { PendingFlow } from "./connectorsModel.mjs";
import { CHECK_ALL_AT_ONCE, checkAllLabel, checkAllReport, checkTargets, healthDetail, healthTone, healthWords } from "./connectorHealthModel.mjs";
import { checkedWords } from "./mcpHealthModel.mjs";
import { t } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";

const toneClass = (tone: string) => (tone === "ok" ? "text-text" : tone === "danger" ? "text-danger" : tone === "warn" ? "text-warn" : "text-text-dim");

type Secrets = Partial<Record<SecretField, string>>;

/**
 * The time, in unix seconds, moved every quarter minute while something on
 * screen is counted against it — a connection's minutes left, a token's
 * expiry, how long ago a check was — so the words stay true without a
 * re-render from anywhere else. Still while nothing is.
 */
function useNow(ticking: boolean): number {
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));
  useEffect(() => {
    if (!ticking) return;
    setNow(Math.floor(Date.now() / 1000));
    const timer = window.setInterval(() => setNow(Math.floor(Date.now() / 1000)), 15_000);
    return () => window.clearInterval(timer);
  }, [ticking]);
  return now;
}

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
            <SecretTextArea what={SECRET_FIELD_WHAT[f]} value={value[f] ?? ""} stored={storedFields.includes(f)} onChange={(v) => onChange({ ...value, [f]: v })} />
          ) : (
            <SecretInput what={SECRET_FIELD_WHAT[f]} value={value[f] ?? ""} stored={storedFields.includes(f)} onChange={(v) => onChange({ ...value, [f]: v })} />
          )}
        </Field>
      ))}
    </>
  );
}

/**
 * Add or edit an account: the label, the connector's parameters, the
 * secrets. The parameters are the card's reading of the definition — one
 * detail read per card, not a second one per dialog — and `null` until it
 * is read, when the form cannot ask and does not save.
 */
function AccountDialog({
  connector,
  params,
  account,
  onClose,
  onSaved,
}: {
  connector: ConnectorRow;
  /** The definition's parameters as the card read them; null while unread. */
  params: readonly ConnectorParamDef[] | null;
  /** Editing this account; adding one when null. */
  account: ConnectorAccountRow | null;
  onClose: () => void;
  onSaved: (row: ConnectorAccountRow) => void;
}) {
  const toast = useToast();
  const [label, setLabel] = useState(() => accountDraft(account, params).label);
  const [values, setValues] = useState<Record<string, string>>(() => accountDraft(account, params).values);
  const [secrets, setSecrets] = useState<Secrets>({});
  const [busy, setBusy] = useState(false);
  const fields = secretFields(connector.auth);
  // What the account still holds and the definition no longer asks for: said, and let go of by the save.
  const strays = strayParams(account, params);
  const missing = missingRequired(values, params);
  const save = async () => {
    setBusy(true);
    const body = accountBody({ label, values, secrets }, account, params);
    await attempt(
      () => api.putConnectorAccount(connector.id, body),
      toast.error,
      (row) => {
        // An edit says what moved — the label, a parameter, the secrets typed — and *Nothing changed.* only when nothing did.
        toast.ok(account ? savedWords(body, account) : addedWords(row.label));
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
          <Button
            variant="primary"
            disabled={!maySaveAccount({ label, busy, definitionRead: params !== null, missing })}
            disabledReason={missing.length > 0 ? t("settings-connectors-panel-fill-required", { names: missing.join(", "), count: missing.length }) : undefined}
            onClick={() => void save()}
          >
            {busy ? t("settings-connectors-panel-saving") : account ? t("settings-connectors-panel-save") : t("settings-connectors-panel-add-account-verb")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <Field label={t("settings-connectors-panel-label")} hint={t("settings-connectors-panel-how-account-named-here-work-personal")}>
          <TextInput value={label} autoFocus onChange={(e) => setLabel(e.target.value)} />
        </Field>
        {/* The parameters the connector asks for are its definition's: until it is read the form cannot ask, and does not save; the card says when the read failed. */}
        {params === null && <Pending what={t("settings-connectors-panel-definition")} rows={pendingRows(t("settings-connectors-panel-definition"))} />}
        {(params ?? []).map((p) => (
          // *Required.* leads the hint and stays beside what the definition says of the field — never one or the other.
          <Field key={p.name} label={p.label || p.name} hint={[p.required ? t("settings-connectors-panel-required") : "", p.doc].filter(Boolean).join(" ") || undefined}>
            <TextInput className="font-mono" value={values[p.name] ?? ""} aria-required={p.required || undefined} onChange={(e) => setValues({ ...values, [p.name]: e.target.value })} />
          </Field>
        ))}
        {strays.length > 0 && (
          <p className="max-w-measure text-2xs leading-relaxed text-text-dim">
            {t("settings-connectors-panel-strays", { count: strays.length })}{" "}
            <span className="font-mono">{strays.map((p) => `${p.name}=${p.value}`).join(" · ")}</span>
          </p>
        )}
        {fields.length > 0 && <SecretFields fields={fields} value={secrets} storedFields={account?.secrets_set ?? []} onChange={setSecrets} />}
        {account && fields.length > 0 && <p className="text-2xs text-text-dim">{t("settings-connectors-panel-field-left-blank-keeps-stored")}</p>}
      </div>
    </Dialog>
  );
}

/** A custom definition, typed as JSON, validated on the node before it is written. */
function DefinitionDialog({ initial, onClose, onSaved }: { initial: ConnectorDefinition | null; onClose: () => void; onSaved: () => void }) {
  const toast = useToast();
  const [opened] = useState(() => JSON.stringify(initial ?? emptyDefinition(), null, 2));
  const [text, setText] = useState(opened);
  // Closing over JSON nobody saved asks first: an editor of this size is minutes of typing.
  const [leaving, setLeaving] = useState(false);
  // The node's word, and the text it was about: *Valid.* goes with the first character typed since.
  const [seen, setSeen] = useState<{ text: string; validation: ConnectorValidation } | null>(null);
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
    const typed = text;
    setBusy("validate");
    await attempt(() => api.validateConnector(def), toast.error, (v) => setSeen({ text: typed, validation: v }));
    setBusy(null);
  };
  // Nothing is written before the node has read it: Save validates first,
  // shows what it found, and writes only a definition with no problem.
  const save = async () => {
    const def = parsed();
    if (!def) return;
    // A renamed id is refused by the node after the round trip; said here, before it.
    const renamedNow = renamedProblem(initial?.id ?? null, def);
    if (renamedNow) {
      toast.error(renamedNow);
      return;
    }
    const typed = text;
    setBusy("save");
    const read: { validation: ConnectorValidation | null } = { validation: null };
    await attempt(() => api.validateConnector(def), toast.error, (v) => {
      read.validation = v;
      setSeen({ text: typed, validation: v });
    });
    const found = problemLines(read.validation);
    if (!read.validation || found.length > 0) {
      if (read.validation) toast.error(saveRefusedWords(found.length));
      setBusy(null);
      return;
    }
    await attempt(() => (initial ? api.updateConnector(initial.id, def) : api.createConnector(def)), toast.error, () => {
      toast.ok(initial ? t("settings-connectors-panel-saved", { def: def.name }) : t("settings-connectors-panel-added-add-account", { def: def.name }));
      onSaved();
    });
    setBusy(null);
  };
  const validation = validationFor(seen, text);
  const problems = problemLines(validation);
  const draft = parseDefinition(text);
  const renamed = draft.ok ? renamedProblem(initial?.id ?? null, draft.value) : null;
  const close = () => {
    if (busy) return;
    if (text !== opened) setLeaving(true);
    else onClose();
  };
  return (
    <Dialog open onClose={close} title={initial ? t("settings-connectors-panel-edit-3", { initial: initial.name }) : t("settings-connectors-panel-add-connector")} width="max-w-2xl"
      footer={
        <>
          <Button variant="ghost" disabled={busy !== null} onClick={close}>{t("settings-connectors-panel-cancel")}</Button>
          <Button disabled={busy !== null} onClick={() => void validate()}>{busy === "validate" ? t("settings-connectors-panel-validating") : t("settings-connectors-panel-validate")}</Button>
          <Button variant="primary" disabled={busy !== null} onClick={() => void save()}>{busy === "save" ? t("settings-connectors-panel-saving") : initial ? t("settings-connectors-panel-save") : t("settings-connectors-panel-create")}</Button>
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
          {/* Held while the node reads it: what is validated and what is written are the same text. */}
          <TextArea
            rows={22}
            autoFocus
            className="font-mono text-2xs"
            value={text}
            spellCheck={false}
            readOnly={busy !== null}
            onChange={(e) => {
              setText(e.target.value);
              // The parser's word was about the text as it stood: it goes with the first character typed since.
              setParseError(null);
            }}
          />
        </Field>
        {/* What the node — or the parser — found, announced as it arrives. */}
        <div role="status" aria-live="polite" className="flex flex-col gap-2">
          {parseError && <p className="text-2xs text-danger">{t("settings-connectors-panel-json", { parseError })}</p>}
          {renamed && <p className="text-2xs text-danger">{renamed}</p>}
          {validation && problems.length === 0 && !renamed && <p className="text-2xs text-text">{t("settings-connectors-panel-valid")}</p>}
          {problems.length > 0 && (
            <ul className="list-disc pl-4 text-2xs text-danger">
              {problems.map((l) => (
                <li key={l}>{l}</li>
              ))}
            </ul>
          )}
        </div>
      </div>
      <ConfirmDialog
        open={leaving}
        onClose={() => setLeaving(false)}
        onConfirm={() => {
          setLeaving(false);
          onClose();
        }}
        title={t("settings-connectors-panel-discard-title")}
        body={<>{t("settings-connectors-panel-discard-body")}</>}
        confirmLabel={t("settings-connectors-panel-discard")}
        danger
      />
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
  // A card is a line until it is opened, and its detail — the definition,
  // the accounts, their health — is read then: nothing is read for a card
  // nobody looks at, on mount or on a token refresh's `connectors_changed`.
  const [open, setOpen] = useState(() => startsOpen(row));
  const detail = useConnectorDetail(open ? row.id : null);
  const accounts = useMemo(() => accountRows(detail.data?.accounts), [detail.data]);
  const [busy, setBusy] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  const [editing, setEditing] = useState<ConnectorAccountRow | null>(null);
  const [forgetting, setForgetting] = useState<ConnectorAccountRow | null>(null);
  const [editingDef, setEditingDef] = useState(false);
  const [deleting, setDeleting] = useState(false);
  // One connection open per account: a second account connected while the
  // first waits for its browser keeps both boxes.
  const [flows, setFlows] = useState<Record<string, PendingFlow>>({});
  const custom = isYours(row.origin);
  const oauth = connectApplies(row.auth);
  const hasCheck = Boolean(row.check);
  const waiting = Object.keys(flows).length > 0;
  const now = useNow(open && (waiting || oauth || accounts.some((a) => Boolean(a.health.checked_at))));
  const bodyId = `connector-${row.id}`;

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
    try {
      const start = await api.startConnectorOauth(row.id, a.id);
      // The flow remembers the row as it stood, so the landing is read as a
      // change to it — a reconnect's row already holds a token. Started
      // again for the same account, it takes the earlier one's place.
      const flow = pendingFlow(start, a);
      setFlows((all) => ({ ...all, [a.id]: flow }));
      // The toast says the browser is opening once it has; a browser that
      // cannot open leaves the URL on the row to copy, and the box stays.
      try {
        await openExternal(flow.url);
        toast.ok(openedWords());
      } catch (e) {
        toast.error(failureText("settings", "connectors-panel-failed", e));
        setFlows((all) => (all[a.id] ? { ...all, [a.id]: { ...all[a.id], browserFailed: true } } : all));
      }
    } catch (e) {
      toast.error(failureText("settings", "connectors-panel-failed", e));
    }
    setBusy(null);
  };
  const without = (all: Record<string, PendingFlow>, id: string) => Object.fromEntries(Object.entries(all).filter(([k]) => k !== id));
  const paste = async (a: ConnectorAccountRow) => {
    const flow = flows[a.id];
    if (!flow || !mayPaste({ flow, busy: busy !== null, now })) return;
    setBusy(`paste:${a.id}`);
    await attempt(() => api.completeConnectorOauth(row.id, a.id, flow.code.trim()), toast.error, () => {
      toast.ok(connectedWords(a.label));
      // What *Check* said was about the tokens held before this connection: the node forgets it.
      setFlows((all) => without(all, a.id));
      detail.reload();
    });
    setBusy(null);
  };
  /** Given up: the box goes, and the node is told — a code that arrives late completes nothing, and its listener need not wait. */
  const cancel = (a: ConnectorAccountRow) => {
    setFlows((all) => without(all, a.id));
    // The box is already gone, which is what was asked; a node that could not be told is said, and forgets the flow on its own when its minutes pass.
    void attempt(() => api.cancelConnectorOauth(row.id, a.id), toast.error, () => undefined);
  };
  const remove = async () => {
    setBusy("delete");
    await attempt(() => api.deleteConnector(row.id), toast.error, () => {
      toast.ok(t("settings-connectors-panel-removed", { row: row.name }));
      onChanged();
    });
    setBusy(null);
  };
  // A connection leaves its box when it landed through the callback — read
  // as a change to the row since the flow began, never as a token the row
  // held before it — when its minutes ran out, which is said, and when its
  // account was removed from somewhere else.
  useEffect(() => {
    if (!detail.data) return;
    const settled = flowsSettled(flows, accounts, now);
    if (settled.flows === flows) return;
    setFlows(settled.flows);
    for (const label of settled.expired) toast.info(t("settings-connectors-flow-ran-out", { label }));
  }, [accounts, detail.data, flows, now, toast]);
  // Back from the browser with a connection open: this card's rows are read
  // again, not only the list's counts — the landing is on the row.
  const reloadDetail = detail.reload;
  useEffect(() => {
    if (!waiting) return;
    const again = () => reloadDetail();
    window.addEventListener("focus", again);
    return () => window.removeEventListener("focus", again);
  }, [waiting, reloadDetail]);
  // An account being edited that was removed from somewhere else: the form
  // closes with a word, never saved into nothing.
  useEffect(() => {
    if (!editing || !detail.data) return;
    if (accounts.some((a) => a.id === editing.id)) return;
    toast.info(t("settings-connectors-panel-account-gone", { label: editing.label }));
    setEditing(null);
  }, [accounts, detail.data, editing, toast]);

  const definitionRead = detail.data !== null;
  return (
    <Card className="flex flex-col gap-2 p-3">
      <div className="flex flex-wrap items-center gap-2">
        {/* The header is the disclosure: the whole name row opens and closes the card. */}
        <button
          type="button"
          aria-expanded={open}
          aria-controls={bodyId}
          onClick={() => setOpen((o) => !o)}
          className="anim -m-1 flex min-w-0 flex-1 flex-wrap items-center gap-2 rounded-control p-1 text-left hover:bg-surface-2/50"
        >
          <ICON.collapsed size={12} aria-hidden className={cn("anim shrink-0 text-text-dim", open && "rotate-90")} />
          <ICON.connector size={14} aria-hidden className="shrink-0 text-text-dim" />
          <h3 className="text-sm font-semibold text-text">{row.name}</h3>
          <code className="font-mono text-2xs text-text-dim">{row.id}</code>
          <Chip tone="neutral">{originWords(row.origin)}</Chip>
        </button>
        {open && custom && (
          <>
            <Button size="sm" variant="ghost" disabled={!mayEditDefinition({ busy: busy !== null, definitionRead })} onClick={() => setEditingDef(true)}>{t("settings-connectors-panel-edit")}</Button>
            <Button size="sm" variant="ghost" disabled={busy !== null} onClick={() => setDeleting(true)}>{t("settings-connectors-panel-delete")}</Button>
          </>
        )}
        {open && needsAccount(row.auth) && (
          // The form asks what the definition declares: until it is read there is nothing to ask, and no door to it.
          <Button size="sm" disabled={busy !== null || !definitionRead} disabledReason={definitionRead ? undefined : t("settings-connectors-panel-add-needs-definition")} onClick={() => setAdding(true)}>
            <ICON.add size={12} aria-hidden />{t("settings-connectors-panel-add-account-verb")}</Button>
        )}
      </div>
      {/* How it signs in, where it calls, what it offers, the accounts here: four facts, set apart. */}
      <p className="flex flex-wrap gap-x-1.5 pl-5 text-2xs text-text-dim">
        {connectorFacts(row).map((fact, i) => (
          <span key={fact} className="inline-flex gap-x-1.5">
            {i > 0 && <span aria-hidden>·</span>}
            {fact}
          </span>
        ))}
      </p>
      {open && (
        <div id={bodyId} className="flex flex-col gap-2">
          <p className="max-w-measure text-2xs leading-relaxed text-text-dim">{row.description}</p>
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
                const verbs = accountVerbs(a, oauth, hasCheck);
                const isChecking = checking.has(a.id);
                const idle = busy !== null || isChecking;
                const flow = flows[a.id];
                const menu: Record<(typeof verbs.more)[number], MenuItem> = {
                  connect: { label: t("settings-connectors-panel-connect-again"), icon: ICON.open, disabled: idle || !a.secrets_set.includes("client_id"), onSelect: () => void connect(a) },
                  secrets: { label: t("settings-connectors-panel-set-secrets"), disabled: idle, onSelect: () => setEditing(a) },
                  check: { label: t("settings-connectors-panel-check"), disabled: idle, onSelect: () => onCheck(a) },
                  default: { label: t("settings-connectors-panel-make-default"), disabled: idle, onSelect: () => void setDefault(a) },
                  forget: { label: t("settings-connectors-panel-forget-verb"), danger: true, separatorBefore: true, disabled: idle, onSelect: () => setForgetting(a) },
                };
                const detailLine = healthDetail(a.health);
                const checked = checkedWords(a.health.checked_at, now);
                const left = flowWords(flow, now);
                return (
                  <li key={a.id} className="rounded-control bg-surface-2/50 p-2">
                    <div className="flex flex-wrap items-center gap-2">
                      <ICON.account size={13} aria-hidden className="shrink-0 text-text-dim" />
                      <span className="text-xs font-medium">{a.label}</span>
                      {a.default && <Chip tone="neutral">{t("settings-connectors-panel-default-chip")}</Chip>}
                      {/* The health the node holds: what the last check found; *checking…* while one runs. When it was is beside it, in words — never behind a hover. */}
                      <Chip tone={isChecking ? "quiet" : healthTone(a.health)}>{isChecking ? t("settings-connectors-panel-checking-chip") : healthWords(a.health, row.name)}</Chip>
                      {checked && !isChecking && <span className="text-2xs text-text-dim">{checked}</span>}
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
                      ) : verbs.main === "check" ? (
                        <Button size="sm" variant="ghost" disabled={idle} onClick={() => onCheck(a)}>{isChecking ? t("settings-connectors-panel-checking-verb") : t("settings-connectors-panel-check")}</Button>
                      ) : (
                        // No check named: the row's verb is its secrets.
                        <Button size="sm" variant="ghost" disabled={idle} onClick={() => setEditing(a)}>{t("settings-connectors-panel-set-secrets")}</Button>
                      )}
                      <MoreMenu label={t("settings-connectors-panel-more-account", { account: a.label })} items={verbs.more.map((verb) => menu[verb])} />
                    </div>
                    {/* Which secrets are held and where, on a line of its own: beside the label it pushed the row's verb off the row. */}
                    <p className={`mt-1 text-2xs ${toneClass(secrets.tone)}`}>{secrets.text}</p>
                    {Object.keys(a.params).length > 0 && (
                      <p className="mt-1 font-mono text-2xs text-text-dim">
                        {Object.entries(a.params).map(([k, v]) => `${k}=${typeof v === "string" ? v : JSON.stringify(v)}`).join(" · ")}
                      </p>
                    )}
                    {expiry && <p className="mt-1 text-2xs text-text-dim">{expiry}</p>}
                    {detailLine && <p className={`mt-1 text-2xs ${toneClass(healthTone(a.health))}`}>{detailLine}</p>}
                    {/* Announced: the box goes away on its own when the browser's callback lands. */}
                    <span className="sr-only" aria-live="polite">
                      {flow ? t("settings-connectors-panel-waiting-platform-s-code-browser-s") : a.secrets_set.includes("access_token") ? t("settings-connectors-panel-connected") : ""}
                    </span>
                    {flow && (
                      <form
                        className="mt-1.5 flex flex-wrap items-end gap-2"
                        onSubmit={(e) => {
                          e.preventDefault();
                          void paste(a);
                        }}
                      >
                        <Field label={t("settings-connectors-panel-paste-code")} hint={t("settings-connectors-panel-platform-cannot-send-browser-back-machine")}>
                          <TextInput className="w-72 font-mono" value={flow.code} onChange={(e) => setFlows((all) => (all[a.id] ? { ...all, [a.id]: { ...all[a.id], code: e.target.value } } : all))} />
                        </Field>
                        <Button size="sm" type="submit" disabled={!mayPaste({ flow, busy: busy !== null, now })}>{busy === `paste:${a.id}` ? t("settings-connectors-panel-connecting") : t("settings-connectors-panel-connect-code")}</Button>
                        <Button size="sm" variant="ghost" disabled={busy !== null} onClick={() => cancel(a)}>{t("settings-connectors-panel-cancel")}</Button>
                      </form>
                    )}
                    {flow?.browserFailed && (
                      <p className="mt-1 max-w-measure break-all text-2xs text-warn">
                        {t("settings-connectors-panel-open-this-link-yourself")} <code className="select-all font-mono">{flow.url}</code>
                      </p>
                    )}
                    {flow && left && <p className="mt-1 text-2xs text-text-dim">{left}</p>}
                  </li>
                );
              })}
            </ul>
          )}
          {/* The redirect URI is copied into another platform's console: the one a started flow registered while one is open, else the port the node resolves — never a guessed one. */}
          {oauth &&
            (waiting ? (
              <p className="max-w-measure text-2xs leading-relaxed text-text-dim">{connectUriWords(Object.values(flows)[0].redirectUri)}</p>
            ) : port === null ? (
              <Pending what={t("settings-connectors-panel-callback-port")} rows={pendingRows(t("settings-connectors-panel-callback-port"))} />
            ) : (
              <p className="max-w-measure text-2xs leading-relaxed text-text-dim">{connectWords(port)}</p>
            ))}
        </div>
      )}
      {adding && detail.data && <AccountDialog connector={row} params={detail.data.connector.params ?? []} account={null} onClose={() => setAdding(false)} onSaved={() => { setAdding(false); detail.reload(); onChanged(); }} />}
      {editing && <AccountDialog connector={row} params={detail.data?.connector.params ?? null} account={editing} onClose={() => setEditing(null)} onSaved={() => { setEditing(null); detail.reload(); }} />}
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
        confirmLabel={t("settings-connectors-panel-forget-confirm")}
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

/** How far *Check all* is: nowhere, reading which accounts there are, or through them. */
type Pass = { phase: "idle" | "reading" | "checking"; done: number; total: number };
const NO_PASS: Pass = { phase: "idle", done: 0, total: 0 };

/** From how many connectors the list is worth a search field. */
const SEARCH_FROM = 6;

export function ConnectorsPanel() {
  const connectors = useConnectors();
  const toast = useToast();
  const [adding, setAdding] = useState(false);
  const [query, setQuery] = useState("");
  // The accounts a check is running for: a row's own *Check*, or *Check all*'s pass over every connector.
  const [checking, setChecking] = useState<ReadonlySet<string>>(new Set());
  // *Check all* from its first read to its last check: the button is held the whole way, so a second press cannot start a second pass.
  const [pass, setPass] = useState<Pass>(NO_PASS);
  // The panel left: a pass under way dials nothing more, and says nothing to a screen that is gone.
  const here = useRef(true);
  useEffect(() => {
    here.current = true;
    return () => {
      here.current = false;
    };
  }, []);
  /** One request as the account; the node keeps the answer and the rows read it back through the bus. Answers what the platform said, or nothing when the request itself failed — which is said. */
  const dial = async (cid: string, aid: string): Promise<AccountCheck | null> => {
    setChecking((s) => new Set(s).add(aid));
    let answer: AccountCheck | null = null;
    await attempt(() => api.checkConnectorAccount(cid, aid), toast.error, (c) => {
      answer = c;
    });
    setChecking((s) => {
      const next = new Set(s);
      next.delete(aid);
      return next;
    });
    return answer;
  };
  /** A row's own check says its answer in a toast, beside the chip. */
  const check = async (cid: string, aid: string, name: string) => {
    const answer = await dial(cid, aid);
    if (!answer) return;
    const line = checkLine(answer, name);
    (line.tone === "ok" ? toast.ok : toast.error)(line.text);
  };
  /** Every account that can answer, of every connector that names a check, a few at a time; what did not answer as connected is gathered into one sentence, and a connector that could not be read is counted in it, never skipped in silence. */
  const checkAll = async () => {
    setPass({ phase: "reading", done: 0, total: 0 });
    const queue: { connector: string; account: string; name: string; label: string }[] = [];
    let unread = 0;
    for (const row of connectors.rows) {
      if (!here.current) return;
      if (row.accounts === 0 || !row.check) continue;
      const detail = await api.connector(row.id).catch(() => null);
      if (detail) {
        const labels = new Map(detail.accounts.map((a) => [a.id, a.label]));
        queue.push(...checkTargets(detail).map((target) => ({ ...target, name: row.name, label: labels.get(target.account) ?? target.account })));
      } else unread += 1;
    }
    const queued = queue.length;
    const failed: { connector: string; account: string; words: string }[] = [];
    let done = 0;
    setPass({ phase: "checking", done, total: queued });
    const workers = Array.from({ length: Math.min(CHECK_ALL_AT_ONCE, queue.length) }, async () => {
      for (let next = queue.shift(); next && here.current; next = queue.shift()) {
        const answer = await dial(next.connector, next.account);
        if (answer && answer.state !== "connected") {
          failed.push({ connector: next.name, account: next.label, words: healthWords({ state: "failing", check: answer.state, status: answer.status ?? null }, next.name) });
        }
        done += 1;
        if (here.current) setPass({ phase: "checking", done, total: queued });
      }
    });
    await Promise.all(workers);
    if (!here.current) return;
    setPass(NO_PASS);
    const report = checkAllReport({ queued, unread, failed });
    if (report) toast.error(report);
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
  const shown = filterConnectors(connectors.rows, query);
  const readAgain = (
    <Tooltip label={t("settings-connectors-panel-read-definitions-machine-s-accounts-again")}>
      <Button size="sm" variant="ghost" disabled={connectors.loading || connectors.refreshing} onClick={connectors.reload} aria-label={t("settings-connectors-panel-read-again")}>
        <ICON.refresh size={12} aria-hidden />
      </Button>
    </Tooltip>
  );
  return (
    <div className="flex flex-col gap-3">
      {/* The panel's own line above says what syncs and what stays; this row only adds where more come from. */}
      {!empty && (
        <div className="flex flex-wrap items-center gap-2">
          {connectors.rows.length >= SEARCH_FROM && (
            <>
              <TextInput
                value={query}
                placeholder={t("settings-connectors-panel-search")}
                aria-label={t("settings-connectors-panel-search-label")}
                className="h-7 max-w-64 py-0"
                onChange={(e) => setQuery(e.target.value)}
              />
              <span className="tnum text-2xs text-text-dim" aria-live="polite">
                {t("settings-connectors-panel-shown-of", { shown: shown.length, all: connectors.rows.length })}
              </span>
            </>
          )}
          <span className="flex-1" />
          <Tooltip label={t("settings-connectors-panel-check-every-account")}>
            <span>
              <Button size="sm" variant="ghost" disabled={!checkable || pass.phase !== "idle"} onClick={() => void checkAll()}>
                {checkAllLabel(pass)}
              </Button>
            </span>
          </Tooltip>
          {readAgain}
          <Button size="sm" onClick={() => setAdding(true)}>
            <ICON.add size={12} aria-hidden />{t("settings-connectors-panel-add-connector")}</Button>
        </div>
      )}
      {!empty && <p className="-mt-1 text-2xs text-text-dim">{t("settings-connectors-panel-install-more-from-library")}</p>}
      {/* A read that failed while rows stand: said above them, with the way to try again — the rows are the last read's. */}
      {connectors.error && !empty && <ErrorNote error={connectors.error} retry={connectors.reload} />}
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
      {!empty && shown.length === 0 && (
        <EmptyState
          icon={ICON.connector}
          title={t("settings-connectors-panel-search-nothing")}
          hint={t("settings-connectors-panel-search-nothing-hint")}
          action={<Button onClick={() => setQuery("")}>{t("settings-connectors-panel-search-clear")}</Button>}
        />
      )}
      {shown.map((row) => (
        <ConnectorCard key={row.id} row={row} port={port} onChanged={connectors.reload} checking={checking} onCheck={(a) => void check(row.id, a.id, row.name)} />
      ))}
      {adding && <DefinitionDialog initial={null} onClose={() => setAdding(false)} onSaved={() => { setAdding(false); connectors.reload(); }} />}
    </div>
  );
}
