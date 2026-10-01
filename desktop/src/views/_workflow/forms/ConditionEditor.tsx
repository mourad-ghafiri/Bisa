/**
 * One condition, as fields. The ten conditions are a closed set
 * (`stepKinds.mjs`'s `CONDITIONS`): six leaves, each drawing the fields its
 * variant carries and nothing else, and four that hold other conditions —
 * `all`, `any`, `one` with a list of children, `not` with one — drawn as a
 * group that nests. No leaf reads the event that began the run: a start
 * maps it onto inputs, and a rule tests those. The core bounds the nesting
 * (`MAX_CONDITION_DEPTH`); the editor stops offering a deeper group at the
 * bound rather than letting a person draw what the validator will refuse.
 */

import type { Condition, InputDef, Step } from "../../../types";
import { Button, Chip, ICON, Select, TextInput } from "../../../ui";
import { CONDITIONS, MAX_CONDITION_DEPTH } from "../stepKinds.mjs";
import { conditionOf, freshCondition, hourOf, isGroup, mayNest, offeredKinds, parseValue, valueText } from "./conditionModel.mjs";
import { t } from "../../../i18n/l10n.mjs";

type Not = Extract<Condition, { condition: "not" }>;

export function ConditionEditor({
  value,
  upstream,
  inputs,
  onChange,
  disabled,
  depth = 1,
}: {
  value: Condition;
  upstream: Step[];
  inputs: InputDef[];
  onChange: (next: Condition) => void;
  disabled?: boolean;
  /** How deep this editor sits; a leaf is 1, and the core allows `MAX_CONDITION_DEPTH`. */
  depth?: number;
}) {
  const stepSelect = (current: string, filter?: (s: Step) => boolean, set?: (id: string) => void) => (
    <Select value={current} aria-label={t("workflow-condition-editor-label-step")} disabled={disabled} onChange={(e) => set?.(e.target.value)}>
      <option value="">{t("workflow-check-step-form-pick-step")}</option>
      {upstream.filter(filter ?? (() => true)).map((s) => (
        <option key={s.id} value={s.id}>
          {s.name || s.id}
        </option>
      ))}
    </Select>
  );
  // A group one level from the bound may not hold another group; a kind that
  // names a step or an input is offered only when there is one to name.
  const canNest = mayNest(depth);
  const offered = offeredKinds(CONDITIONS, value.condition, depth, upstream, inputs);

  return (
    <div className="flex flex-col gap-1.5">
      <Select
        value={value.condition}
        aria-label={t("workflow-condition-editor-label-condition")}
        disabled={disabled}
        onChange={(e) => onChange(conditionOf(e.target.value as Condition["condition"], upstream, inputs, value))}
      >
        {offered.map((c) => (
          <option key={c.condition} value={c.condition}>
            {c.label}
          </option>
        ))}
      </Select>
      {isGroup(value) && (
        <div className="flex flex-col gap-1.5 border-l-2 border-border pl-2">
          {value.of.map((child, i) => (
            <div key={i} className="flex flex-col gap-1">
              <div className="flex items-center gap-2">
                <Chip tone="quiet">
                  {value.condition} · {i + 1}
                </Chip>
                <Button
                  size="sm"
                  variant="ghost"
                  className="ml-auto"
                  disabled={disabled}
                  aria-label={t("workflow-condition-editor-remove-condition")}
                  onClick={() => onChange({ ...value, of: value.of.filter((_, j) => j !== i) })}
                >
                  <ICON.delete size={12} aria-hidden />
                </Button>
              </div>
              <ConditionEditor
                value={child}
                upstream={upstream}
                inputs={inputs}
                disabled={disabled}
                depth={depth + 1}
                onChange={(next) => onChange({ ...value, of: value.of.map((c, j) => (j === i ? next : c)) })}
              />
            </div>
          ))}
          <div className="flex items-center gap-2">
            <Button size="sm" disabled={disabled} onClick={() => onChange({ ...value, of: [...value.of, freshCondition(upstream, inputs)] })}>
              <ICON.add size={12} aria-hidden />{t("workflow-condition-editor-add-condition")}</Button>
            {value.of.length === 0 && <span className="text-2xs text-danger">{t("workflow-condition-editor-empty-group-problem")}</span>}
            {!canNest && <span className="text-2xs text-text-dim">{t("workflow-condition-editor-conditions-nest-deep-most", { MAX_CONDITION_DEPTH })}</span>}
          </div>
        </div>
      )}
      {value.condition === "not" && (
        <div className="border-l-2 border-border pl-2">
          <ConditionEditor
            value={(value as Not).of}
            upstream={upstream}
            inputs={inputs}
            disabled={disabled}
            depth={depth + 1}
            onChange={(of) => onChange({ condition: "not", of })}
          />
        </div>
      )}
      {value.condition === "input_equals" && (
        <div className="grid gap-1.5 md:grid-cols-2">
          <Select value={value.input} aria-label={t("workflow-condition-editor-label-input")} disabled={disabled} onChange={(e) => onChange({ ...value, input: e.target.value })}>
            <option value="">{t("workflow-condition-editor-pick-input")}</option>
            {inputs.map((i) => (
              <option key={i.name} value={i.name}>
                {i.name}
              </option>
            ))}
          </Select>
          <TextInput className="font-mono" value={valueText(value.value)} placeholder={t("workflow-condition-editor-label-value")} aria-label={t("workflow-condition-editor-label-value")} disabled={disabled} onChange={(e) => onChange({ ...value, value: parseValue(e.target.value) })} />
        </div>
      )}
      {(value.condition === "output_equals" || value.condition === "output_matches") && (
        <div className="grid gap-1.5 md:grid-cols-3">
          {stepSelect(value.step, undefined, (step) => onChange({ ...value, step }))}
          <TextInput className="font-mono" value={value.path} placeholder={t("workflow-condition-editor-path-dotted")} aria-label={t("workflow-condition-editor-path-dotted")} disabled={disabled} onChange={(e) => onChange({ ...value, path: e.target.value })} />
          {value.condition === "output_equals" ? (
            <TextInput className="font-mono" value={valueText(value.value)} placeholder={t("workflow-condition-editor-label-value")} aria-label={t("workflow-condition-editor-label-value")} disabled={disabled} onChange={(e) => onChange({ ...value, value: parseValue(e.target.value) })} />
          ) : (
            <TextInput className="font-mono" value={value.contains} placeholder={t("workflow-condition-editor-label-contains")} aria-label={t("workflow-condition-editor-label-contains")} disabled={disabled} onChange={(e) => onChange({ ...value, contains: e.target.value })} />
          )}
        </div>
      )}
      {value.condition === "answered" && (
        <div className="grid gap-1.5 md:grid-cols-2">
          {stepSelect(value.step, (s) => s.kind === "human", (step) => onChange({ ...value, step }))}
          <TextInput className="font-mono" value={value.option} placeholder={t("workflow-condition-editor-option-id")} aria-label={t("workflow-condition-editor-option-id")} disabled={disabled} onChange={(e) => onChange({ ...value, option: e.target.value })} />
        </div>
      )}
      {value.condition === "outcome" && (
        <div className="grid gap-1.5 md:grid-cols-2">
          {stepSelect(value.step, (s) => s.kind === "check" || s.kind === "approval", (step) => onChange({ ...value, step }))}
          <Select value={value.passed ? "passed" : "failed"} aria-label={t("workflow-condition-editor-label-outcome")} disabled={disabled} onChange={(e) => onChange({ ...value, passed: e.target.value === "passed" })}>
            <option value="passed">{t("workflow-condition-editor-passed")}</option>
            <option value="failed">{t("workflow-condition-editor-did-not-pass")}</option>
          </Select>
        </div>
      )}
      {value.condition === "between" && (
        <div className="flex flex-wrap items-center gap-2 text-2xs text-text-dim">{t("workflow-condition-editor-between")}<TextInput className="w-16" inputMode="numeric" value={String(value.from_hour)} aria-label={t("workflow-condition-editor-from-hour")} disabled={disabled} onChange={(e) => onChange({ ...value, from_hour: hourOf(e.target.value) })} />{t("workflow-condition-editor-words")}<TextInput className="w-16" inputMode="numeric" value={String(value.to_hour)} aria-label={t("workflow-condition-editor-hour")} disabled={disabled} onChange={(e) => onChange({ ...value, to_hour: hourOf(e.target.value) })} />{t("workflow-condition-editor-o-clock-utc-range-past-midnight")}</div>
      )}
    </div>
  );
}
