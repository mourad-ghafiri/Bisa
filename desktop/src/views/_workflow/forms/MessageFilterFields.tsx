/**
 * A message filter's fields — where it lands, who wrote it, whom it
 * mentions, what it says — shared by a start that begins when a message
 * arrives, a wait that holds for one, and a boundary event that hears one.
 * Where and what are templates (a start's over the inputs it listens with,
 * a wait's and a boundary's over the run); who wrote it is you, any agent,
 * or someone named — one agent, a person (a person hosted from another node
 * only this way) or a team, whose members' messages all count — or an input
 * of kind assignee. A place the list does not carry — a goal's thread, an
 * input the workflow no longer declares — is shown as it is, never as the
 * first option.
 */

import { useState } from "react";
import type { InputDef, MessageFilter as WireMessageFilter, MessageFrom } from "../../../types";
import { useWorkspace } from "../../../shell/useWorkspaceData";
import { Field, Select, TextInput } from "../../../ui";
import { AssigneePicker } from "../../_work/AssigneePicker";
import { MESSAGE_FROM } from "../stepKinds.mjs";
import { ValueRefField } from "./AgentStepForm";
import { picked, valueOf, type AssigneeRef } from "./assigneeRefModel.mjs";
import { t } from "../../../i18n/l10n.mjs";

/**
 * The wire's message filter. `from` is restated: the generated type narrows
 * it to the two words, because its default is one of them, and the wire
 * takes someone named as well.
 */
export type MessageFilter = Omit<WireMessageFilter, "from"> & { from?: MessageFrom };

export function MessageFilterFields<F extends MessageFilter>({
  value,
  inputs,
  onChange,
  disabled,
  templateHint,
}: {
  value: F;
  inputs: readonly InputDef[];
  onChange: (next: F) => void;
  disabled?: boolean;
  /** What the templates read: a start's the inputs it listens with, a wait's and a boundary's the run. */
  templateHint: string;
}) {
  const ws = useWorkspace();
  const set = (patch: Partial<MessageFilter>) => onChange({ ...value, ...patch });
  const from = value.from ?? "you";
  const named = typeof from === "object";
  // Someone picked from the list before anyone is named: the choice is the form's until a name is.
  const [someone, setSomeone] = useState(named);
  const word = named || someone ? "someone" : from;
  const textInputs = inputs.filter((i) => i.kind === "text" || i.kind === "choice");
  const place = value.in ?? "";
  const listed = place === "" || ws.channels.some((c) => c.channel.id === place) || textInputs.some((i) => `{inputs.${i.name}}` === place);
  return (
    <div className="flex flex-col gap-3">
      <Field label={t("workflow-message-filter-fields-where")} hint={t("workflow-message-filter-fields-where-hint")}>
        <Select value={place} disabled={disabled} onChange={(e) => set({ in: e.target.value || null })}>
          <option value="">{t("workflow-message-filter-fields-anywhere")}</option>
          {ws.channels.map((c) => (
            <option key={c.channel.id} value={c.channel.id}>{t("workflow-notify-step-form-channel", { channel: c.channel.name })}</option>
          ))}
          {textInputs.length > 0 && (
            <optgroup label={t("workflow-notify-step-form-read-from-input-start")}>
              {textInputs.map((i) => (
                <option key={i.name} value={`{inputs.${i.name}}`}>{t("workflow-connector-step-form-input", { i: i.name })}</option>
              ))}
            </optgroup>
          )}
          {listed || <option value={place}>{place}</option>}
        </Select>
      </Field>
      <Field label={t("workflow-message-filter-fields-from")} hint={t("workflow-message-filter-fields-from-hint")}>
        <div className="flex flex-col gap-1.5">
          <Select
            value={word}
            disabled={disabled}
            onChange={(e) => {
              const v = e.target.value;
              if (v === "someone") setSomeone(true);
              else {
                setSomeone(false);
                set({ from: v as "you" | "agents" });
              }
            }}
          >
            {MESSAGE_FROM.map((m) => (
              <option key={m.from} value={m.from}>
                {m.label}
              </option>
            ))}
          </Select>
          {word === "someone" && (
            <ValueRefField
              label={t("workflow-message-filter-fields-who")}
              value={named ? valueOf(from as AssigneeRef) : null}
              inputs={[...inputs]}
              kind="assignee"
              disabled={disabled}
              onChange={(v) => {
                const ref = picked(v);
                if (ref) set({ from: ref as MessageFrom });
              }}
            >
              {(fixed, setFixed) => <AssigneePicker value={fixed ? [fixed] : []} max={1} disabled={disabled} onChange={(n) => setFixed(n[0] ?? "")} />}
            </ValueRefField>
          )}
        </div>
      </Field>
      <ValueRefField
        label={t("workflow-message-filter-fields-mentions")}
        hint={t("workflow-message-filter-fields-mentions-hint")}
        value={valueOf(value.mentions as AssigneeRef | null | undefined)}
        inputs={[...inputs]}
        kind="assignee"
        disabled={disabled}
        onChange={(v) => set({ mentions: picked(v) })}
      >
        {(fixed, setFixed) => <AssigneePicker value={fixed ? [fixed] : []} max={1} disabled={disabled} onChange={(n) => setFixed(n[0] ?? "")} />}
      </ValueRefField>
      <Field label={t("workflow-message-filter-fields-contains")} hint={t("workflow-message-filter-fields-contains-hint", { hint: templateHint })}>
        <TextInput value={value.contains ?? ""} disabled={disabled} onChange={(e) => set({ contains: e.target.value || null })} />
      </Field>
    </div>
  );
}
