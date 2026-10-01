/**
 * Settings › Decision Making: the Decision-Making Agent — the third core
 * agent, the one that judges, at the platform's eleven decision points. It
 * holds no record of its own: a decision point asks it a typed question and
 * gets an answer with how sure it is, and this panel is where who answers
 * for it, and whether it is on, is set.
 *
 * `GET /decisions/status` is the one read that draws the header, the
 * readiness line, the calibration note and every point's standing; the
 * `decisions.*` settings are edited through the same registry-generated
 * controls the Security panels use (`RegistryPanel`), so a switch here and a
 * switch in Settings › Security draw and save the same way. An API key is
 * never read back — `PUT`/`DELETE /decisions/key/{provider}` answer only
 * whether one is now stored — and the box that holds what was typed is its
 * provider's own: moving to another provider empties it.
 *
 * The words and the rules are `decisionsModel.mjs`'s; this draws.
 */

import { useEffect, useMemo, useState } from "react";
import { api } from "../../api";
import { useEngineEvents } from "../../bus";
import { useResolvedSettingsRead } from "../../shell/settingsStore";
import type { DeciderStatus, DecisionProviderKind, DecisionResponse, JudgementRecord } from "../../types";
import { Button, Card, Chip, ErrorNote, Field, Pending, SecretInput, Section, Select, Switch, TextArea, TextInput, useToast } from "../../ui";
import { useReloadOnReconnect } from "../../ui/useReloadOnReconnect";
import { attempt, useAsync, type Async } from "../_work/useAsync";
import {
  answerLines,
  blankKey,
  blankTryForm,
  calibratedNote,
  fieldsFor,
  judgementLine,
  keyCleared,
  keyFor,
  keySaved,
  keyTyped,
  KEYS,
  mayClearKey,
  maySaveKey,
  movesJudgements,
  movesStatus,
  offList,
  pointRows,
  pointsOffAfter,
  readyLine,
  SCALARS,
  takesKey,
  tryProblem,
  tryRequest,
} from "./decisionsModel.mjs";
import { pendingRows, phase } from "./loadModel.mjs";
import { RegistryPanel } from "./RegistryPanel";
import { t } from "../../i18n/l10n.mjs";

/** The status, re-read whenever a `decisions.*` key changes or a judgement lands — and when the node comes back. */
function useDeciderStatus() {
  const status = useAsync((s) => api.decisionsStatus(s), []);
  const { reload } = status;
  useEngineEvents((e) => {
    if (movesStatus(e.payload)) reload();
  });
  useReloadOnReconnect(reload);
  return status;
}

function Header({ status }: { status: Async<DeciderStatus> }) {
  const line = status.data ? readyLine(status.data) : readyLine(null);
  const note = status.data ? calibratedNote(status.data) : null;
  return (
    <Card>
      {phase(status) === "pending" ? (
        <Pending what={t("settings-decisions-panel-the-decision-making-agent")} rows={pendingRows(t("settings-decisions-panel-security-status"))} />
      ) : (
        <>
          {status.error && <ErrorNote error={status.error} retry={status.reload} />}
          {status.data && (
            <>
              <p className="text-xs font-medium">{status.data.agent.name}</p>
              <p className="mt-0.5 text-2xs text-text-dim">{status.data.agent.description}</p>
              <div className="mt-1.5 flex items-center gap-2">
                <Chip tone={line.tone}>{line.text}</Chip>
              </div>
              {note && <p className="mt-1.5 text-2xs text-text-dim">{note}</p>}
            </>
          )}
        </>
      )}
    </Card>
  );
}

/** The provider picker, then only the fields the chosen provider takes — and its API key when it takes one. */
function ProviderCard({ status }: { status: Async<DeciderStatus> }) {
  const toast = useToast();
  const resolved = useResolvedSettingsRead(null);
  const values = useMemo(() => new Map((resolved.data?.settings ?? []).map((r) => [r.key, r.value])), [resolved.data]);
  const provider = (values.get(KEYS.provider) as DecisionProviderKind | undefined) ?? "harness";
  const rlcdAuth = (values.get(KEYS.rlcd.auth) as string | undefined) ?? "bearer";
  const fields = fieldsFor(provider, rlcdAuth);
  const needsKey = takesKey(provider, rlcdAuth);

  // The key stays in the box, hidden, for as long as this window lives —
  // never in storage; what the node already holds is kept beside it, so Save
  // waits for a change (ide/13 §Secret fields). The box is its provider's
  // own (`keyFor`): a key typed for one is never drawn or sent under another.
  const [box, setBox] = useState(() => blankKey(provider));
  const own = keyFor(box, provider);
  const [busy, setBusy] = useState(false);
  const save = async () => {
    if (!maySaveKey(box, provider)) return;
    setBusy(true);
    const ok = await attempt(() => api.setDecisionKey(provider, own.typed), toast.error);
    setBusy(false);
    if (ok) {
      setBox(keySaved(box, provider));
      toast.ok(t("settings-decisions-panel-key-set"));
      status.reload();
    }
  };
  const clear = async () => {
    setBusy(true);
    const ok = await attempt(() => api.clearDecisionKey(provider), toast.error);
    setBusy(false);
    if (ok) {
      setBox(keyCleared(provider));
      toast.ok(t("settings-decisions-panel-key-cleared"));
      status.reload();
    }
  };

  return (
    <Section title={t("settings-decisions-panel-who-answers")}>
      <RegistryPanel group="decisions" only={[KEYS.provider]} />
      {fields.length > 0 && <RegistryPanel group="decisions" only={fields} />}
      {needsKey && (
        <Field
          label={t("settings-decisions-panel-api-key")}
          hint={
            status.data?.key_stored
              ? t("settings-decisions-panel-key-stored-kept-machine-never-shown")
              : t("settings-decisions-panel-key-stored-provider-will-refuse-without")
          }
        >
          <div className="flex items-center gap-2">
            <SecretInput className="min-w-0 flex-1" what={t("ui-secret-input-what-key")} value={own.typed} stored={status.data?.key_stored} disabled={busy} onChange={(typed) => setBox(keyTyped(box, provider, typed))} />
            <Button size="sm" disabled={busy || !maySaveKey(box, provider)} onClick={() => void save()}>{t("settings-decisions-panel-save")}</Button>
            <Button size="sm" variant="ghost" disabled={busy || !mayClearKey(status.data)} onClick={() => void clear()}>{t("settings-decisions-panel-clear")}</Button>
          </div>
        </Field>
      )}
    </Section>
  );
}

/** `decisions.enabled`, then every decision point with its own switch. */
function PointsCard({ status }: { status: Async<DeciderStatus> }) {
  const toast = useToast();
  const resolved = useResolvedSettingsRead(null);
  const [busy, setBusy] = useState<string | null>(null);
  const read = resolved.data?.settings.find((r) => r.key === KEYS.pointsOff)?.value;
  // What this window wrote and the node's read has yet to say: a second
  // switch is built on it, never on the list as it was read (`offList`).
  const [written, setWritten] = useState<string[] | null>(null);
  const readKey = JSON.stringify(read ?? null);
  useEffect(() => setWritten(null), [readKey]);

  const flip = async (point: string, on: boolean) => {
    if (busy !== null) return;
    const next = pointsOffAfter(offList(read, written), point, on);
    setBusy(point);
    const ok = await attempt(() => api.setSettings("workspace", { [KEYS.pointsOff]: next }), toast.error);
    setBusy(null);
    if (ok) {
      setWritten(next);
      resolved.reload();
      status.reload();
    }
  };

  return (
    <Section title={t("settings-decisions-panel-where")}>
      <RegistryPanel group="decisions" only={[KEYS.enabled]} />
      {phase(status) === "pending" ? (
        <Pending what={t("settings-decisions-panel-decision-points")} rows={pendingRows(t("settings-decisions-panel-security-status"))} />
      ) : (
        <div className="mt-2 flex flex-col gap-1">
          {pointRows(status.data, busy).map((row) => (
            <div key={row.id} className="rounded-control border border-border px-2 py-1.5">
              <Switch checked={row.on} disabled={row.held} onChange={(v) => void flip(row.id, v)} label={row.label} hint={row.hint} />
            </div>
          ))}
        </div>
      )}
    </Section>
  );
}

/** A state, a question, and the provider's own answer — nothing decided, nothing recorded. */
function TryIt() {
  const toast = useToast();
  const [form, setForm] = useState(blankTryForm());
  const [result, setResult] = useState<DecisionResponse | null>(null);
  const [busy, setBusy] = useState(false);
  const problem = tryProblem(form);
  // An answer is the answer to the question as it was asked: an edit takes it off the screen.
  const set = (patch: Partial<typeof form>) => {
    setForm((f) => ({ ...f, ...patch }));
    setResult(null);
  };

  const run = async () => {
    setBusy(true);
    setResult(null);
    const ok = await attempt(() => api.decisionsTry(tryRequest(form)), toast.error, setResult);
    setBusy(false);
    if (!ok) return;
  };

  return (
    <Section title={t("settings-decisions-panel-try")}>
      <p className="mb-1 text-2xs text-text-dim">{t("settings-decisions-panel-nothing-decided-nothing-recorded-request-provider")}</p>
      <div className="flex flex-col gap-2">
        <Field label={t("settings-decisions-panel-state")} hint={t("settings-decisions-panel-what-model-reads-free-text")}>
          <TextArea rows={3} className="font-mono text-2xs" value={form.state} onChange={(e) => set({ state: e.target.value })} placeholder={t("settings-decisions-panel-customer-asked-refund-order-4821")} />
        </Field>
        <Field label={t("settings-decisions-panel-question")}>
          <Select value={form.kind} onChange={(e) => set({ kind: e.target.value as typeof form.kind })}>
            <option value="noul">{t("settings-decisions-panel-yes")}</option>
            <option value="choice">{t("settings-decisions-panel-choice-among-options")}</option>
          </Select>
        </Field>
        <Field label={t("settings-decisions-panel-instructions")}>
          <TextInput value={form.instructions} onChange={(e) => set({ instructions: e.target.value })} placeholder={t("settings-decisions-panel-urgent")} />
        </Field>
        {form.kind === "choice" && (
          <Field label={t("settings-decisions-panel-options")} hint={t("settings-decisions-panel-least-two-id-what-choosing-means")}>
            <div className="flex flex-col gap-1.5">
              {form.options.map((o, i) => (
                <div key={i} className="flex items-center gap-1.5">
                  <TextInput
                    className="w-28 font-mono"
                    value={o.id}
                    placeholder={t("settings-decisions-panel-id")}
                    aria-label={t("settings-decisions-panel-option-id")}
                    onChange={(e) => set({ options: form.options.map((x, j) => (j === i ? { ...x, id: e.target.value } : x)) })}
                  />
                  <TextInput
                    className="min-w-0 flex-1"
                    value={o.meaning}
                    placeholder={t("settings-decisions-panel-what-choosing-means")}
                    aria-label={t("settings-decisions-panel-option-meaning")}
                    onChange={(e) => set({ options: form.options.map((x, j) => (j === i ? { ...x, meaning: e.target.value } : x)) })}
                  />
                  <Button size="sm" variant="ghost" aria-label={t("settings-decisions-panel-remove-option")} onClick={() => set({ options: form.options.filter((_, j) => j !== i) })}>
                    ×
                  </Button>
                </div>
              ))}
              <div>
                <Button size="sm" onClick={() => set({ options: [...form.options, { id: "", meaning: "" }] })}>{t("settings-decisions-panel-add-option")}</Button>
              </div>
            </div>
          </Field>
        )}
        <div>
          <Button size="sm" disabled={busy || Boolean(problem)} onClick={() => void run()}>
            {busy ? t("settings-decisions-panel-asking") : t("settings-ssh-panel-ask")}
          </Button>
          {problem && <span className="ml-2 text-2xs text-danger">{problem}</span>}
        </div>
        {result && (
          <div className="rounded-control border border-border bg-surface-2 p-2 text-2xs">
            <p className="font-mono text-text-dim">{result.model}</p>
            <ul className="mt-1 flex flex-col gap-0.5">
              {answerLines(result).map((l) => (
                <li key={l}>{l}</li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </Section>
  );
}

/** The newest judgements this node has recorded — never a person's own approval. */
function Recent() {
  const recent = useAsync((s) => api.decisionsRecent(50, s), []);
  const { reload } = recent;
  useEngineEvents((e) => {
    if (movesJudgements(e.payload)) reload();
  });
  useReloadOnReconnect(reload);
  const rows: JudgementRecord[] = recent.data ?? [];
  return (
    <Section title={t("settings-decisions-panel-recent-judgements")}>
      {phase(recent) === "pending" ? (
        <Pending what={t("settings-decisions-panel-decisions")} rows={3} />
      ) : recent.error ? (
        <ErrorNote error={recent.error} retry={recent.reload} />
      ) : rows.length === 0 ? (
        <p className="text-2xs text-text-dim">{t("settings-decisions-panel-nothing-judged-since-node-started")}</p>
      ) : (
        <ul className="flex flex-col gap-1">
          {rows.map((r) => (
            <li key={r.seq} className="text-2xs text-text-dim">
              {judgementLine(r)}
            </li>
          ))}
        </ul>
      )}
    </Section>
  );
}

export function DecisionsPanel() {
  // One read, shared by every card below — three cards over the same status
  // would otherwise ask the node three times on every mount.
  const status = useDeciderStatus();
  return (
    <div className="mb-4 flex max-w-3xl flex-col gap-4">
      <Header status={status} />
      <ProviderCard status={status} />
      <PointsCard status={status} />
      <RegistryPanel group="decisions" only={SCALARS} />
      <TryIt />
      <Recent />
    </div>
  );
}
