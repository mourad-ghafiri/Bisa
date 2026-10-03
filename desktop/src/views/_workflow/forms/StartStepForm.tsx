/**
 * A start step: one way a run begins, in words (03-workflows §Start events)
 * — *By hand · On a schedule · When called · When a message arrives · When a
 * signal is raised · When a project changes · When a run finishes · When the
 * platform says · When an outside platform lists something new · When a
 * check starts failing* — and that event's own fields.
 *
 * An event start then says two more things. *Inputs from the event*: one row
 * per input, the template that maps the occurrence onto it
 * (`{event.payload.…}`) — the one place the event is read; a step reads the
 * input. And its guard, in words: what an occurrence does while a run it
 * started is still going (one at a time, skipped, several at once) and the
 * debounce. A hook shows its local call — under the control-plane token —
 * and, public, whether its secret is minted, with *Rotate* to mint a new one
 * shown once. The rules and the words are `startForm.mjs`'s; this draws.
 */

import { useState } from "react";
import { api } from "../../../api";
import { useWorkspace } from "../../../shell/useWorkspaceData";
import type { HookSecret, InputDef, ListenerView, StartOn, Step } from "../../../types";
import { Button, Checkbox, Chip, ConfirmDialog, Field, ICON, Labelled, NumberInput, Select, TextInput, useToast } from "../../../ui";
import { attempt } from "../../_work/useAsync";
import { FIRE_ON, OVERLAPS, START_EVENTS } from "../stepKinds.mjs";
import { useConnectorDetail, useConnectors } from "../useConnectors";
import { HookSecretNote } from "../HookSecretNote";
import { shownFor } from "../hookSecretsModel.mjs";
import { DEFAULT_ACCOUNT, accountFromValue, accountValue, fixedValue, inputValue, strayConnector, strayOperation, withConnector, withOperation, withParam } from "./connectorStepModel.mjs";
import { MessageFilterFields } from "./MessageFilterFields";
import { ProjectFilterFields } from "./ProjectFilterFields";
import { RunFilterFields } from "./RunFilterFields";
import { ScheduleFields } from "./ScheduleFields";
import { PlatformFilterFields, SignalFilterFields } from "./SignalFilterFields";
import { ValueRefField } from "./AgentStepForm";
import { MANUAL, eventOf, guardOf, guardWords, localHookPath, mappingRows, mappingSuggestions, setGuard, setMapping, setStartEvent, strayMappings } from "./startForm.mjs";
import { t } from "../../../i18n/l10n.mjs";

type Start = Extract<Step, { kind: "start" }>;

/** Who listens with this start, when the design is stored: a library workflow, or a goal's own design. */
export interface StartHost {
  workflow?: string | null;
  goal?: string | null;
}

/** An outside platform's poll: which connector, which of its reads, as which account, with what parameters, told apart by which key. */
function ConnectorPollFields({ on, inputs, onChange, disabled }: { on: Extract<StartOn, { event: "connector" }>; inputs: InputDef[]; onChange: (next: StartOn) => void; disabled?: boolean }) {
  const connectors = useConnectors();
  const detail = useConnectorDetail(on.connector ?? null);
  const def = detail.data?.connector ?? null;
  const accounts = detail.data?.accounts ?? [];
  // A poll only reads: a writing operation is never one to poll (`bad_poll`).
  const reads = (def?.operations ?? []).filter((o) => !o.writes);
  const operation = reads.find((o) => o.id === on.operation) ?? null;
  const accountInputs = inputs.filter((i) => i.kind === "account" && on.connector != null && i.connector === on.connector);
  const params = on.params ?? {};
  // What the start names and the pickers do not offer — a connector gone from here, a write where only a read goes — is said in the picker.
  const notInstalled = strayConnector(on.connector, connectors.rows);
  const notARead = strayOperation(on.operation, reads, def ? def.operations : null);
  return (
    <div className="flex flex-col gap-3">
      <div className="grid gap-3 @xs:grid-cols-2">
        <Field label={t("workflow-step-kinds-connector")} hint={connectors.rows.length === 0 ? t("workflow-connector-step-form-nothing-installed-here-yet-settings-connectors") : t("workflow-connector-step-form-connector-installed-here-slug")}>
          <Select className="font-mono" value={on.connector ?? ""} disabled={disabled} onChange={(e) => onChange(withConnector(on, e.target.value))}>
            <option value="">{t("workflow-connector-step-form-pick-connector")}</option>
            {connectors.rows.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name} · {c.id}
              </option>
            ))}
            {on.connector != null && notInstalled !== null && <option value={on.connector}>{notInstalled}</option>}
          </Select>
        </Field>
        <Field label={t("workflow-connector-step-form-operation")} hint={operation ? operation.description : t("workflow-start-step-form-poll-reads-only")}>
          <Select className="font-mono" value={on.operation ?? ""} disabled={disabled || !def} onChange={(e) => onChange(withOperation(on, e.target.value))}>
            <option value="">{t("workflow-connector-step-form-pick-operation")}</option>
            {reads.map((o) => (
              <option key={o.id} value={o.id}>
                {o.id} — {o.name}
              </option>
            ))}
            {on.operation != null && notARead !== null && <option value={on.operation}>{notARead}</option>}
          </Select>
        </Field>
      </div>
      <Field label={t("workflow-connector-step-form-account")} hint={t("workflow-connector-step-form-connector-s-default-account-node-one")}>
        <Select value={accountValue(on.account)} disabled={disabled || !def} onChange={(e) => onChange({ ...on, account: accountFromValue(e.target.value) as typeof on.account })}>
          <option value={DEFAULT_ACCOUNT}>{t("workflow-connector-step-form-connector-s-default-account")}</option>
          {accounts.length > 0 && (
            <optgroup label={t("workflow-connector-step-form-account-machine")}>
              {accounts.map((a) => (
                <option key={a.id} value={fixedValue(a.id)}>
                  {t("workflow-connector-step-form-account-option", { label: a.label, default: a.default ? "yes" : "no" })}
                </option>
              ))}
            </optgroup>
          )}
          {accountInputs.length > 0 && (
            <optgroup label={t("workflow-connector-step-form-from-input-start")}>
              {accountInputs.map((i) => (
                <option key={i.name} value={inputValue(i.name)}>{t("workflow-connector-step-form-input", { i: i.name })}</option>
              ))}
            </optgroup>
          )}
        </Select>
      </Field>
      {(operation?.params ?? []).length > 0 && (
        <Labelled label={t("workflow-connector-step-form-parameters")} hint={t("workflow-start-step-form-poll-params-hint")}>
          <div className="flex flex-col gap-2">
            {(operation?.params ?? []).map((p) => (
              <div key={p.name} className="flex flex-col gap-0.5">
                <div className="flex items-center gap-1.5">
                  <code className="font-mono text-2xs">{p.name}</code>
                  {p.required && <Chip tone="warn">{t("workflow-connector-step-form-required")}</Chip>}
                  <span className="truncate text-2xs text-text-dim">{p.label}</span>
                </div>
                <TextInput
                  className="font-mono"
                  value={params[p.name] ?? ""}
                  aria-label={p.name}
                  disabled={disabled}
                  onChange={(e) => onChange({ ...on, params: withParam(params, p.name, e.target.value) })}
                />
              </div>
            ))}
          </div>
        </Labelled>
      )}
      <Field label={t("workflow-start-step-form-key")} hint={t("workflow-start-step-form-key-hint")}>
        <TextInput className="font-mono" value={on.key ?? ""} placeholder={t("workflow-start-step-form-id")} disabled={disabled} onChange={(e) => onChange({ ...on, key: e.target.value.trim() || null })} />
      </Field>
      <ScheduleFields value={on} inputs={inputs} disabled={disabled} onChange={onChange} />
    </div>
  );
}

/** A check start: a shell command on a cadence, in its project's tree or its own scratch folder, and which results begin a run. */
function CheckFields({ on, inputs, onChange, disabled }: { on: Extract<StartOn, { event: "check" }>; inputs: InputDef[]; onChange: (next: StartOn) => void; disabled?: boolean }) {
  return (
    <div className="flex flex-col gap-3">
      <Field label={t("workflow-check-step-form-command")} hint={t("workflow-start-step-form-check-command-hint")}>
        <TextInput className="font-mono" value={on.command} disabled={disabled} onChange={(e) => onChange({ ...on, command: e.target.value })} />
      </Field>
      <ValueRefField
        label={t("workflow-agent-step-form-project")}
        hint={t("workflow-start-step-form-check-project-hint")}
        value={on.project && typeof on.project === "object" ? { input: on.project.input } : (on.project ?? null)}
        inputs={inputs}
        kind="project"
        disabled={disabled}
        onChange={(v) => onChange({ ...on, project: v })}
      >
        {(fixed, setFixed) => <ProjectSelect value={fixed} disabled={disabled} onChange={setFixed} />}
      </ValueRefField>
      <Field label={t("workflow-start-step-form-fire-on")}>
        <Select value={on.fire_on ?? "starts_failing"} disabled={disabled} onChange={(e) => onChange({ ...on, fire_on: e.target.value as typeof on.fire_on })}>
          {FIRE_ON.map((f) => (
            <option key={f.fire_on} value={f.fire_on}>
              {f.label}
            </option>
          ))}
        </Select>
      </Field>
      <ScheduleFields value={on} inputs={inputs} disabled={disabled} onChange={onChange} />
    </div>
  );
}

/** A check's placement: one of the workspace's projects, or its own scratch folder. */
function ProjectSelect({ value, disabled, onChange }: { value: string; disabled?: boolean; onChange: (v: string) => void }) {
  const ws = useWorkspace();
  return (
    <Select value={value} disabled={disabled} onChange={(e) => onChange(e.target.value)}>
      <option value="">{t("workflow-start-step-form-own-scratch-folder")}</option>
      {ws.projects.map((p) => (
        <option key={p.project.id} value={p.project.id}>
          {p.project.name}
        </option>
      ))}
    </Select>
  );
}

/** A hook's calls: the local one, under the control-plane token; the public one, with its secret's state and *Rotate*. */
function HookFields({ step, on, host, listener, onChange, disabled }: { step: Start; on: Extract<StartOn, { event: "hook" }>; host: StartHost | null; listener: ListenerView | null; onChange: (next: StartOn) => void; disabled?: boolean }) {
  const toast = useToast();
  const [shown, setShown] = useState<HookSecret | null>(null);
  const [rotating, setRotating] = useState(false);
  // A secret callers already hold is replaced only once the person says so.
  const [asking, setAsking] = useState(false);
  // The secret this form shows is the one minted for this very hook, and for no other step.
  const mine = shownFor(shown, step.id);
  const hasSecret = Boolean(listener?.public_hook?.has_secret || mine);
  const rotate = async () => {
    const goal = host?.goal ?? null;
    const workflow = host?.workflow ?? null;
    const mint = goal !== null ? () => api.rotateGoalHookSecret(goal, step.id) : workflow !== null ? () => api.rotateWorkflowHookSecret(workflow, step.id) : null;
    if (!mint || rotating) return;
    setRotating(true);
    await attempt(mint, toast.error, (secret) => setShown(secret));
    setRotating(false);
  };
  const stored = !!host && (!!host.goal || !!host.workflow);
  return (
    <div className="flex flex-col gap-3">
      {stored ? (
        <p className="text-2xs text-text-dim">{t("workflow-start-step-form-local-call", { path: listener?.local_hook ?? localHookPath(host, step.id) })}</p>
      ) : (
        <p className="text-2xs text-text-dim">{t("workflow-start-step-form-local-call-once-saved")}</p>
      )}
      <Checkbox
        label={t("workflow-start-step-form-public")}
        hint={t("workflow-start-step-form-public-hint")}
        checked={!!on.public}
        disabled={disabled}
        onChange={(v) => onChange(v ? { ...on, public: true } : { event: "hook" })}
      />
      {on.public && listener?.public_hook && (
        <div className="flex flex-col gap-1.5 text-2xs">
          <p className="text-text-dim">{t("workflow-start-step-form-public-call", { path: listener.public_hook.path })}</p>
          <div className="flex items-center gap-2">
            <Chip tone={hasSecret ? "ok" : "warn"} icon={ICON.key}>
              {hasSecret ? t("workflow-start-step-form-secret-minted") : t("workflow-start-step-form-no-secret-yet")}
            </Chip>
            {/* A new secret is the host's, not an edit of the design: a read-only design rotates too. The first one replaces nothing, so it asks nothing. */}
            <Button size="sm" variant="ghost" disabled={rotating} onClick={() => (hasSecret ? setAsking(true) : void rotate())}>
              <ICON.restart size={12} aria-hidden />
              {t("workflow-start-step-form-rotate")}
            </Button>
          </div>
        </div>
      )}
      <ConfirmDialog
        open={asking}
        onClose={() => setAsking(false)}
        title={t("workflow-start-step-form-rotate-title")}
        body={t("workflow-start-step-form-rotate-body")}
        confirmLabel={t("workflow-start-step-form-rotate-confirm")}
        danger
        onConfirm={() => {
          setAsking(false);
          void rotate();
        }}
      />
      {on.public && !listener?.public_hook && <p className="text-2xs text-text-dim">{t("workflow-start-step-form-public-once-listening")}</p>}
      {mine && <HookSecretNote secret={mine} />}
    </div>
  );
}

/** The event's own fields, by event. */
function EventFields({ step, inputs, host, listener, onChange, disabled }: { step: Start; inputs: InputDef[]; host: StartHost | null; listener: ListenerView | null; onChange: (next: StartOn) => void; disabled?: boolean }) {
  const on = step.on;
  const listens = t("workflow-start-step-form-listening-inputs-hint");
  switch (on.event) {
    case "manual":
      return <p className="text-2xs text-text-dim">{t("workflow-start-step-form-by-hand-hint")}</p>;
    case "schedule":
      return <ScheduleFields value={on} inputs={inputs} disabled={disabled} onChange={onChange} />;
    case "hook":
      return <HookFields step={step} on={on} host={host} listener={listener} disabled={disabled} onChange={onChange} />;
    case "message":
      return <MessageFilterFields value={on} inputs={inputs} disabled={disabled} templateHint={listens} onChange={onChange} />;
    case "signal":
      return <SignalFilterFields name={on.name} fields={on.fields} disabled={disabled} templateHint={listens} onChange={(next) => onChange({ ...on, ...next })} />;
    case "project":
      return <ProjectFilterFields value={on} inputs={inputs} disabled={disabled} onChange={onChange} />;
    case "run":
      return <RunFilterFields value={on} disabled={disabled} onChange={onChange} />;
    case "platform":
      return <PlatformFilterFields topic={on.topic} fields={on.fields} disabled={disabled} onChange={(next) => onChange({ ...on, ...next })} />;
    case "connector":
      return <ConnectorPollFields on={on} inputs={inputs} disabled={disabled} onChange={onChange} />;
    case "check":
      return <CheckFields on={on} inputs={inputs} disabled={disabled} onChange={onChange} />;
    default:
      return null;
  }
}

export function StartStepForm({
  step,
  inputs,
  onChange,
  disabled,
  host = null,
  listeners = [],
}: {
  step: Start;
  inputs: InputDef[];
  onChange: (next: Step) => void;
  disabled?: boolean;
  /** Who listens with it, once the design is stored: its hook's call and its secret are the host's. */
  host?: StartHost | null;
  /** The host's listeners, while it listens: a hook's public call and its secret's state. */
  listeners?: readonly ListenerView[];
}) {
  const event = eventOf(step);
  const listener = listeners.find((l) => l.step === step.id) ?? null;
  const rows = mappingRows(step, inputs);
  const stray = strayMappings(step, inputs);
  const guard = guardOf(step);
  const suggestions = mappingSuggestions(event);
  const listId = `start-${step.id}-payload`;
  return (
    <div className="flex flex-col gap-3">
      <Field label={t("workflow-start-step-form-begins")} hint={t("workflow-start-step-form-begins-hint")}>
        <Select value={event} disabled={disabled} onChange={(e) => onChange(setStartEvent(step, e.target.value as StartOn["event"]))}>
          {START_EVENTS.map((e) => (
            <option key={e.event} value={e.event}>
              {e.label}
            </option>
          ))}
        </Select>
      </Field>
      <EventFields step={step} inputs={inputs} host={host} listener={listener} disabled={disabled} onChange={(on) => onChange({ ...step, on })} />
      {listener?.failed && (
        <p className="flex items-start gap-1.5 rounded-control border border-danger/40 bg-danger-soft px-2 py-1.5 text-2xs text-danger">
          <ICON.warn size={12} aria-hidden className="mt-0.5 shrink-0" />
          {t("workflow-start-step-form-could-not-arm", { why: listener.failed })}
        </p>
      )}
      {event !== MANUAL && (
        <>
          <Labelled label={t("workflow-start-step-form-inputs-from-event")} hint={t("workflow-start-step-form-inputs-from-event-hint")}>
            <div className="flex flex-col gap-1.5">
              {rows.length === 0 && <p className="text-2xs text-text-dim">{t("workflow-inputs-form-workflow-takes-no-inputs")}</p>}
              {rows.map((r) => (
                <div key={r.input} className="grid grid-cols-[minmax(6rem,auto)_1fr] items-center gap-2">
                  <span className="flex items-center gap-1 text-2xs">
                    <code className="font-mono">{r.input}</code>
                    {r.required && <Chip tone="warn">{t("workflow-connector-step-form-required")}</Chip>}
                  </span>
                  <TextInput
                    className="font-mono"
                    value={r.template ?? ""}
                    list={suggestions.length > 0 ? listId : undefined}
                    placeholder={r.required ? t("workflow-start-step-form-from-listening-inputs") : t("workflow-start-step-form-left-default")}
                    aria-label={t("workflow-start-step-form-maps-onto", { input: r.label })}
                    disabled={disabled}
                    onChange={(e) => onChange(setMapping(step, r.input, e.target.value))}
                  />
                </div>
              ))}
              {suggestions.length > 0 && (
                <datalist id={listId}>
                  {suggestions.map((s) => (
                    <option key={s} value={s} />
                  ))}
                </datalist>
              )}
              {stray.map((name) => (
                <div key={name} className="flex items-center gap-1.5 text-2xs">
                  <code className="font-mono text-danger">{name}</code>
                  <span className="text-text-dim">{t("workflow-start-step-form-maps-undeclared-input")}</span>
                  <Button size="sm" variant="ghost" disabled={disabled} onClick={() => onChange(setMapping(step, name, null))}>
                    {t("workflow-connector-step-form-remove")}
                  </Button>
                </div>
              ))}
            </div>
          </Labelled>
          <Labelled label={t("workflow-start-step-form-guard")} hint={guardWords(step)}>
            <div className="flex flex-wrap items-center gap-2">
              <Select value={guard.overlap} disabled={disabled} aria-label={t("workflow-start-step-form-overlap")} onChange={(e) => onChange(setGuard(step, { ...guard, overlap: e.target.value as typeof guard.overlap }))}>
                {OVERLAPS.map((o) => (
                  <option key={o.overlap} value={o.overlap}>
                    {o.label}
                  </option>
                ))}
              </Select>
              {guard.overlap === "parallel" && (
                <NumberInput className="w-20" value={guard.max} min={1} disabled={disabled} aria-label={t("workflow-start-step-form-at-most-at-once")} onCommit={(max) => onChange(setGuard(step, { ...guard, max }))} />
              )}
              <span className="text-2xs text-text-dim">{t("workflow-start-step-form-debounce")}</span>
              <NumberInput className="w-20" value={guard.debounce} min={0} disabled={disabled} aria-label={t("workflow-start-step-form-debounce-seconds")} onCommit={(debounce) => onChange(setGuard(step, { ...guard, debounce }))} />
              <span className="text-2xs text-text-dim">{t("workflow-start-step-form-seconds")}</span>
            </div>
          </Labelled>
        </>
      )}
    </div>
  );
}
