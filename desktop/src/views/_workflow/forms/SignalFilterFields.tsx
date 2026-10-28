/**
 * A named signal's filter — its name, dotted lowercase words, and exact
 * fields on its payload — and a platform topic's — the bus events' topics,
 * one or several, and exact fields. Shared by the starts that begin on one,
 * the waits that hold for one and, for a signal, the boundary events that
 * hear one.
 */

import { Field, Labelled, TextInput } from "../../../ui";
import type { OneOrMany } from "./eventFilters";
import { ExactFieldsEditor } from "./ExactFieldsEditor";
import { PLATFORM_TOPICS } from "../platformTopics.mjs";
import { WordsInput } from "./ManyFields";
import { validSignalName } from "./startForm.mjs";
import { t } from "../../../i18n/l10n.mjs";

type Fields = Record<string, string> | undefined;

export function SignalFilterFields({
  name,
  fields,
  onChange,
  disabled,
  templateHint,
}: {
  name: string;
  fields: Fields;
  onChange: (next: { name: string; fields: Record<string, string> }) => void;
  disabled?: boolean;
  /** What the templates read: a start's the inputs it listens with, a wait's and a boundary's the run. */
  templateHint: string;
}) {
  // A name may be a template; only a literal one is held to the words here — the validator judges the rest.
  const literal = !name.includes("{");
  const bad = literal && name !== "" && !validSignalName(name);
  return (
    <div className="flex flex-col gap-3">
      <Field label={t("workflow-signal-filter-fields-name")} hint={bad ? t("workflow-signal-filter-fields-name-not-dotted-words") : t("workflow-signal-filter-fields-name-hint", { hint: templateHint })}>
        <TextInput className="font-mono" value={name} disabled={disabled} /* for the machine */ placeholder="report.ready" onChange={(e) => onChange({ name: e.target.value, fields: fields ?? {} })} />
      </Field>
      <Labelled label={t("workflow-wait-step-form-fields")} hint={t("workflow-signal-filter-fields-exact-matches")}>
        <ExactFieldsEditor value={fields} disabled={disabled} onChange={(next) => onChange({ name, fields: next })} />
      </Labelled>
    </div>
  );
}

export function PlatformFilterFields({
  topic,
  fields,
  onChange,
  disabled,
}: {
  topic: OneOrMany<string> | null | undefined;
  fields: Fields;
  /** A topic is required on the wire: none chosen is written as `""`, which the validator names. */
  onChange: (next: { topic: OneOrMany<string>; fields: Record<string, string> }) => void;
  disabled?: boolean;
}) {
  return (
    <div className="flex flex-col gap-3">
      <Labelled label={t("workflow-wait-step-form-topic")} hint={t("workflow-signal-filter-fields-topic-hint")}>
        <WordsInput mono value={topic} disabled={disabled} /* for the machine */ placeholder="goal.closed" aria-label={t("workflow-wait-step-form-topic")} suggestions={PLATFORM_TOPICS} suggest="typed" onChange={(next) => onChange({ topic: next ?? "", fields: fields ?? {} })} />
      </Labelled>
      <Labelled label={t("workflow-wait-step-form-fields")} hint={t("workflow-signal-filter-fields-exact-matches")}>
        <ExactFieldsEditor value={fields} disabled={disabled} onChange={(next) => onChange({ topic: topic ?? "", fields: next })} />
      </Labelled>
    </div>
  );
}
