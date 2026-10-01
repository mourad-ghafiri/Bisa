/**
 * A step's boundary events (03-workflows §Boundary events), for the kinds
 * whose work can be stopped while it is live — an agent, a human, an
 * approval, a wait, a spawn that waits. *＋ Timeout · Reminder · Message ·
 * Signal*; each either **diverts** the step — it stops, and the run takes
 * the flows labelled with the boundary's name, drawn from its chip on the
 * card — or acts **beside** it: *Post* into a conversation, *Emit* a named
 * signal. A reminder never diverts. A rename carries the divert's flows
 * along, committed on blur; a removal takes them away. The rules are
 * `boundaryModel.mjs`'s; this draws.
 */

import { useEffect, useState } from "react";
import type { Boundary, InputDef, Step } from "../../../types";
import { AssigneePicker } from "../../_work/AssigneePicker";
import { BOUNDARY_ON_ICON, Button, Field, ICON, NumberInput, Select, TextArea, TextInput } from "../../../ui";
import { BOUNDARY_ACTS, BOUNDARY_EVENTS, TEMPLATE_HINT, mayCarryBoundaries } from "../stepKinds.mjs";
import { DEFAULT_REMINDERS, actsFor, addBoundary, consequence, removeBoundaryOn, renameBoundaryOn, replaceBoundary, setAct, setEvent, type BoundaryActName, type BoundaryEvent } from "./boundaryModel.mjs";
import { ExactFieldsEditor } from "./ExactFieldsEditor";
import { MessageFilterFields } from "./MessageFilterFields";
import { RefSource, isInputRef } from "./RefSource";
import { SignalFilterFields } from "./SignalFilterFields";
import { fixedWords, withFixed } from "./assigneeRefModel.mjs";
import { t } from "../../../i18n/l10n.mjs";

/** A boundary's name, committed on blur; a refused name reverts and says why. */
function BoundaryName({ step, name, disabled, onChange }: { step: Step; name: string; disabled?: boolean; onChange: (next: Step) => void }) {
  const [draft, setDraft] = useState(name);
  const [why, setWhy] = useState<string | null>(null);
  useEffect(() => setDraft(name), [name]);
  const commit = () => {
    const r = renameBoundaryOn(step, name, draft);
    if (r.ok) {
      setWhy(null);
      if (r.step !== step) onChange(r.step);
    } else {
      setWhy(r.reason);
      setDraft(name);
    }
  };
  return (
    <span className="flex min-w-0 flex-1 flex-col gap-0.5">
      <TextInput
        className="font-mono"
        value={draft}
        aria-label={t("workflow-boundary-events-editor-name")}
        disabled={disabled}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            commit();
          }
        }}
      />
      {why && <span className="text-2xs text-danger">{why}</span>}
    </span>
  );
}

/** The seconds a timeout or a reminder counts: fixed, or read from a number input. */
function Seconds({ secs, inputs, disabled, onChange }: { secs: { input: string } | number; inputs: InputDef[]; disabled?: boolean; onChange: (secs: { input: string } | number) => void }) {
  return (
    <div className="flex flex-col gap-1.5">
      <RefSource value={secs} inputs={inputs} kind="number" disabled={disabled} onChange={(r) => onChange(r ?? 86400)} />
      {!isInputRef(secs) && <NumberInput className="w-32" value={typeof secs === "number" ? secs : 86400} min={1} disabled={disabled} onCommit={onChange} aria-label={t("workflow-wait-step-form-seconds")} />}
    </div>
  );
}

/** One boundary event: its name, what it listens for, what it does, and that event's and that act's fields. */
function BoundaryRow({ step, boundary, inputs, disabled, onChange }: { step: Step; boundary: Boundary; inputs: InputDef[]; disabled?: boolean; onChange: (next: Step) => void }) {
  const on = boundary.on;
  const event = on.event as BoundaryEvent;
  const Icon = BOUNDARY_ON_ICON[event] ?? BOUNDARY_ON_ICON.after;
  const put = (next: Boundary) => onChange(replaceBoundary(step, boundary.name, next));
  const acts = actsFor(event);
  return (
    <div className="flex flex-col gap-2 rounded-control border border-border p-2">
      <div className="flex items-center gap-2">
        <Icon size={13} aria-hidden className="shrink-0 text-text-dim" />
        <BoundaryName step={step} name={boundary.name} disabled={disabled} onChange={onChange} />
        <Button size="sm" variant="ghost" disabled={disabled} aria-label={t("workflow-boundary-events-editor-remove")} onClick={() => onChange(removeBoundaryOn(step, boundary.name))}>
          <ICON.delete size={12} aria-hidden />
        </Button>
      </div>
      <div className="grid gap-2 md:grid-cols-2">
        <Select value={event} disabled={disabled} aria-label={t("workflow-boundary-events-editor-listens-for")} onChange={(e) => onChange(setEvent(step, boundary.name, e.target.value as BoundaryEvent))}>
          {BOUNDARY_EVENTS.map((b) => (
            <option key={b.event} value={b.event}>
              {b.label}
            </option>
          ))}
        </Select>
        <Select value={boundary.act} disabled={disabled} aria-label={t("workflow-boundary-events-editor-does")} onChange={(e) => onChange(setAct(step, boundary.name, e.target.value as BoundaryActName))}>
          {BOUNDARY_ACTS.filter((a) => acts.includes(a.act)).map((a) => (
            <option key={a.act} value={a.act}>
              {a.label}
            </option>
          ))}
        </Select>
      </div>
      {on.event === "after" && (
        <Field label={t("workflow-boundary-events-editor-after")} hint={t("workflow-boundary-events-editor-after-hint")}>
          <Seconds secs={on.secs} inputs={inputs} disabled={disabled} onChange={(secs) => put({ ...boundary, on: { ...on, secs } })} />
        </Field>
      )}
      {on.event === "every" && (
        <div className="grid gap-2 md:grid-cols-2">
          <Field label={t("workflow-boundary-events-editor-every")} hint={t("workflow-boundary-events-editor-every-hint")}>
            <Seconds secs={on.secs} inputs={inputs} disabled={disabled} onChange={(secs) => put({ ...boundary, on: { ...on, secs } })} />
          </Field>
          <Field label={t("workflow-boundary-events-editor-at-most")} hint={t("workflow-boundary-events-editor-at-most-hint", { n: DEFAULT_REMINDERS })}>
            <NumberInput className="w-24" value={on.max ?? DEFAULT_REMINDERS} min={1} max={1000} disabled={disabled} onCommit={(max) => put({ ...boundary, on: { ...on, max } })} aria-label={t("workflow-boundary-events-editor-at-most")} />
          </Field>
        </div>
      )}
      {on.event === "message" && <MessageFilterFields value={on} inputs={inputs} disabled={disabled} templateHint={TEMPLATE_HINT} onChange={(next) => put({ ...boundary, on: next })} />}
      {on.event === "signal" && <SignalFilterFields name={on.name} fields={on.fields} disabled={disabled} templateHint={TEMPLATE_HINT} onChange={(next) => put({ ...boundary, on: { ...on, ...next } })} />}
      {boundary.act === "notify" && (
        <>
          <Field label={t("workflow-notify-step-form-message")} hint={t("workflow-boundary-events-editor-post-hint", { TEMPLATE_HINT })}>
            <TextArea rows={2} value={boundary.template} disabled={disabled} onChange={(e) => put({ ...boundary, template: e.target.value })} />
          </Field>
          <Field label={t("workflow-notify-step-form-mention")} hint={t("workflow-notify-step-form-who-woken-none-wakes-nobody-notice")}>
            <AssigneePicker
              value={fixedWords(boundary.mentions)}
              disabled={disabled}
              onChange={(next) => put({ ...boundary, mentions: withFixed(boundary.mentions, next) })}
            />
          </Field>
        </>
      )}
      {boundary.act === "emit" && (
        <>
          <Field label={t("workflow-signal-filter-fields-name")} hint={t("workflow-boundary-events-editor-emit-hint")}>
            <TextInput className="font-mono" value={boundary.signal} /* for the machine */ placeholder="review.waiting" disabled={disabled} onChange={(e) => put({ ...boundary, signal: e.target.value })} />
          </Field>
          <Field label={t("workflow-emit-step-form-payload")} hint={t("workflow-emit-step-form-payload-hint", { TEMPLATE_HINT })}>
            <ExactFieldsEditor value={boundary.payload} disabled={disabled} onChange={(payload) => put({ ...boundary, payload })} />
          </Field>
        </>
      )}
      <p className="text-2xs text-text-dim">{consequence(step, boundary)}</p>
    </div>
  );
}

export function BoundaryEventsEditor({ step, inputs, onChange, disabled }: { step: Step; inputs: InputDef[]; onChange: (next: Step) => void; disabled?: boolean }) {
  const boundaries = step.boundaries ?? [];
  const carries = mayCarryBoundaries(step);
  // A step that no longer carries them — a spawn that stopped waiting — still shows them, to remove.
  if (!carries && boundaries.length === 0) return null;
  return (
    <Field label={t("workflow-boundary-events-editor-boundary-events")} hint={carries ? t("workflow-boundary-events-editor-hint") : t("workflow-boundary-events-editor-cannot-carry")}>
      <div className="flex flex-col gap-2">
        {boundaries.map((b) => (
          <BoundaryRow key={b.name} step={step} boundary={b} inputs={inputs} disabled={disabled} onChange={onChange} />
        ))}
        {carries && (
          <div className="flex flex-wrap items-center gap-1.5">
            {BOUNDARY_EVENTS.map((b) => {
              const Icon = BOUNDARY_ON_ICON[b.event as BoundaryEvent];
              return (
                <Button key={b.event} size="sm" disabled={disabled} onClick={() => onChange(addBoundary(step, b.event as BoundaryEvent))}>
                  <ICON.add size={12} aria-hidden />
                  <Icon size={12} aria-hidden />
                  {b.label}
                </Button>
              );
            })}
          </div>
        )}
      </div>
    </Field>
  );
}
