/**
 * A message filter's fields — where it lands, who wrote it, whom it
 * mentions, what it says, what kind of place — shared by a start that
 * begins when a message arrives, a wait that holds for one, and a boundary
 * event that hears one. Each takes one or several: places by id (a channel,
 * a goal's thread, a conversation) or an input's; senders as words — you,
 * any agent, any person, anyone — beside named ones (an agent, a person, a
 * team whose members all count, or an input of kind assignee); mentions the
 * same way; the kinds of place as a set. Where and what are templates (a
 * start's over the inputs it listens with, a wait's and a boundary's over
 * the run). A place the workspace does not list — a goal's thread, an input
 * the workflow no longer declares — keeps its chip as it is.
 */

import type { InputDef } from "../../../types";
import { useWorkspace } from "../../../shell/useWorkspaceData";
import { Checkbox, Field, Labelled, TextInput } from "../../../ui";
import { AssigneePicker } from "../../_work/AssigneePicker";
import { MESSAGE_FROM, MESSAGE_PLACES } from "../stepKinds.mjs";
import { fixedWords, inputNames, toggleInput, withFixed, type AssigneeRef } from "./assigneeRefModel.mjs";
import type { MessageFilterEx, MessageFromWord, MessagePlace } from "./eventFilters";
import { WordChecks, WordsInput } from "./ManyFields";
import { fromList, joinWords, splitWords } from "./manyModel.mjs";
import { asList } from "./startForm.mjs";
import { t } from "../../../i18n/l10n.mjs";

/** The sender words, in the core's order; `someone` is the picker below, not a word. */
const WORDS = MESSAGE_FROM.filter((m) => m.from !== "someone").map((m) => ({ word: m.from as MessageFromWord, label: m.label }));
const WORD_NAMES = WORDS.map((w) => w.word);
const PLACES = MESSAGE_PLACES.map((p) => ({ word: p.kind as MessagePlace, label: p.label }));

/** Inputs of kind assignee as checkboxes, read beside the fixed ones a picker holds. */
function AssigneeInputs({ refs, inputs, disabled, onChange }: { refs: readonly AssigneeRef[]; inputs: readonly InputDef[]; disabled?: boolean; onChange: (next: AssigneeRef[]) => void }) {
  const candidates = inputs.filter((i) => i.kind === "assignee");
  if (candidates.length === 0) return null;
  const named = inputNames(refs);
  return (
    <div className="flex flex-wrap gap-x-4 gap-y-1">
      {candidates.map((i) => (
        <Checkbox key={i.name} label={t("workflow-message-filter-fields-from-input", { name: i.name })} checked={named.includes(i.name)} disabled={disabled} onChange={() => onChange(toggleInput(refs, i.name))} />
      ))}
    </div>
  );
}

export function MessageFilterFields<F extends object>({
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
  const filter = value as MessageFilterEx;
  const set = (patch: Partial<MessageFilterEx>) => onChange({ ...value, ...patch });
  const { words, rest } = splitWords(filter.from, WORD_NAMES);
  const named = rest as AssigneeRef[];
  const mentions = asList(filter.mentions);
  const channels = new Map(ws.channels.map((c) => [c.channel.id, c.channel.name]));
  const textInputs = inputs.filter((i) => i.kind === "text" || i.kind === "choice");
  const places = [
    ...ws.channels.map((c) => ({ word: c.channel.id, label: t("workflow-notify-step-form-channel", { channel: c.channel.name }) })),
    ...textInputs.map((i) => ({ word: `{inputs.${i.name}}`, label: t("workflow-connector-step-form-input", { i: i.name }) })),
  ];
  return (
    <div className="flex flex-col gap-3">
      <Labelled label={t("workflow-message-filter-fields-where")} hint={t("workflow-message-filter-fields-where-hint")}>
        <WordsInput
          mono
          value={filter.in}
          disabled={disabled}
          placeholder={t("workflow-message-filter-fields-anywhere")}
          suggestions={places}
          labelOf={(id) => {
            const name = channels.get(id);
            return name ? t("workflow-notify-step-form-channel", { channel: name }) : id;
          }}
          aria-label={t("workflow-message-filter-fields-where")}
          onChange={(next) => set({ in: next })}
        />
      </Labelled>
      <Labelled label={t("workflow-message-filter-fields-from")} hint={t("workflow-message-filter-fields-from-hint")}>
        <WordChecks options={WORDS} value={fromList(words) as MessageFromWord | MessageFromWord[] | null} fallback={named.length ? null : "you"} columns={2} disabled={disabled} onChange={(next) => set({ from: joinWords(next, named) })} />
      </Labelled>
      <Labelled label={t("workflow-message-filter-fields-named")} hint={t("workflow-message-filter-fields-named-hint")}>
        <div className="flex flex-col gap-1.5">
          <AssigneePicker value={fixedWords(named)} disabled={disabled} onChange={(picked) => set({ from: joinWords(words, withFixed(named, picked)) })} />
          <AssigneeInputs refs={named} inputs={inputs} disabled={disabled} onChange={(next) => set({ from: joinWords(words, next) })} />
        </div>
      </Labelled>
      <Labelled label={t("workflow-message-filter-fields-mentions")} hint={t("workflow-message-filter-fields-mentions-hint")}>
        <div className="flex flex-col gap-1.5">
          <AssigneePicker value={fixedWords(mentions)} disabled={disabled} onChange={(picked) => set({ mentions: fromList(withFixed(mentions, picked)) })} />
          <AssigneeInputs refs={mentions} inputs={inputs} disabled={disabled} onChange={(next) => set({ mentions: fromList(next) })} />
        </div>
      </Labelled>
      <Labelled label={t("workflow-message-filter-fields-kind")} hint={t("workflow-message-filter-fields-kind-hint")}>
        <WordChecks options={PLACES} value={filter.kind} fallback={null} columns={3} disabled={disabled} onChange={(kind) => set({ kind })} />
      </Labelled>
      <Field label={t("workflow-message-filter-fields-contains")} hint={t("workflow-message-filter-fields-contains-hint", { hint: templateHint })}>
        <TextInput value={filter.contains ?? ""} disabled={disabled} onChange={(e) => set({ contains: e.target.value || null })} />
      </Field>
    </div>
  );
}
