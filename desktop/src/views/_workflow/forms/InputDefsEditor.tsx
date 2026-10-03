/**
 * The inputs a workflow is started with: a name, a label, a kind, whether it
 * is required, and a default. Steps reach them as `{inputs.<name>}` and as
 * value references.
 *
 * Rows are keyed by an identity that follows the row through renames and
 * removals, never by its index: keyed by index, removing the second of three
 * rows hands the third row the second's input element and its focus.
 */

import { useEffect, useId, useRef, useState } from "react";
import type { InputDef } from "../../../types";
import { Button, Checkbox, ICON, Select, TextInput } from "../../../ui";
import { useConnectors } from "../useConnectors";
import { INPUT_KINDS, idProblem, idProblemWords, inputKindWords, nextInputName, optionsFrom, optionsText } from "../workflowForm.mjs";
import { t as tr } from "../../../i18n/l10n.mjs";

/** A key per row that survives edits: each new row object inherits the key of the one it replaced. */
function useRowKeys(inputs: InputDef[]) {
  const keys = useRef(new WeakMap<InputDef, string>());
  const next = useRef(0);
  const keyOf = (i: InputDef): string => {
    let k = keys.current.get(i);
    if (!k) {
      k = `row-${next.current++}`;
      keys.current.set(i, k);
    }
    return k;
  };
  const inherit = (from: InputDef, to: InputDef) => {
    keys.current.set(to, keyOf(from));
    return to;
  };
  for (const i of inputs) keyOf(i);
  return { keyOf, inherit };
}

/** A default value, typed as text and committed on blur, so a decimal can be typed. */
function DefaultField({ input, disabled, onCommit }: { input: InputDef; disabled?: boolean; onCommit: (value: unknown) => void }) {
  const shown = input.default === undefined || input.default === null ? "" : String(input.default);
  const [draft, setDraft] = useState(shown);
  useEffect(() => setDraft(shown), [shown]);
  const commit = () => {
    const t = draft.trim();
    if (t === "") return onCommit(undefined);
    if (input.kind === "number") {
      const n = Number(t);
      if (!Number.isFinite(n)) return setDraft(shown);
      return onCommit(n);
    }
    onCommit(t);
  };
  return (
    <TextInput
      className="min-w-40 flex-1 font-mono"
      value={draft}
      placeholder={tr("workflow-input-defs-editor-default")}
      aria-label={tr("workflow-input-defs-editor-default")}
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
  );
}

/**
 * An input's name, typed as a draft and committed on blur through `onRename`
 * — the whole rename, placeholders and references included — so no step
 * reads a name that is half typed, and no intermediate name is saved. A name
 * that cannot be taken says why under the row while it is typed, and a blur
 * that puts the old name back says it kept it (`errorId` is the line's id).
 */
function NameField({
  input,
  taken,
  disabled,
  errorId,
  onRename,
  onProblem,
}: {
  input: InputDef;
  taken: (name: string) => boolean;
  disabled?: boolean;
  errorId: string;
  onRename: (from: string, to: string) => void;
  /** What the row says under itself: why the typed name is refused, or that the old one was kept. */
  onProblem: (words: string | null) => void;
}) {
  const [draft, setDraft] = useState(input.name);
  useEffect(() => setDraft(input.name), [input.name]);
  const problem = idProblem(draft, input.name, taken);
  const commit = () => {
    if (problem === null && draft !== input.name) onRename(input.name, draft);
    else if (problem !== null) {
      setDraft(input.name);
      onProblem(idProblemWords(problem, "input", input.name));
    }
  };
  return (
    <TextInput
      className="font-mono"
      value={draft}
      placeholder={tr("workflow-input-defs-editor-name")}
      aria-label={tr("workflow-input-defs-editor-input-name")}
      aria-invalid={problem !== null}
      aria-describedby={errorId}
      title={tr("workflow-input-defs-editor-steps-read-inputs-name-renaming-renames")}
      disabled={disabled}
      onChange={(e) => {
        setDraft(e.target.value);
        onProblem(idProblemWords(idProblem(e.target.value, input.name, taken), "input"));
      }}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          commit();
        }
      }}
    />
  );
}

/** A choice's options, typed as a draft — commas and spaces as they are typed — and read into the list on blur or Enter. */
function OptionsField({ options, disabled, onCommit }: { options: string[]; disabled?: boolean; onCommit: (next: string[]) => void }) {
  const stored = optionsText(options);
  const [draft, setDraft] = useState(stored);
  useEffect(() => setDraft(stored), [stored]);
  const commit = () => {
    const next = optionsFrom(draft);
    setDraft(optionsText(next));
    if (optionsText(next) !== stored) onCommit(next);
  };
  return (
    <TextInput
      className="min-w-48 flex-1 font-mono"
      value={draft}
      placeholder={tr("workflow-input-defs-editor-options-comma-separated")}
      aria-label={tr("workflow-input-defs-editor-choice-options")}
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
  );
}

/** One input's row: its name, label, kind and delete; under them required, the account's connector, the options and the default. */
function InputRow({
  input: i,
  inputs,
  disabled,
  onPatch,
  onReplace,
  onRemove,
  onRename,
  connectors,
}: {
  input: InputDef;
  inputs: InputDef[];
  disabled?: boolean;
  /** The connectors installed here, read once for every row. */
  connectors: ReturnType<typeof useConnectors>;
  onPatch: (p: Partial<InputDef>) => void;
  onReplace: (next: InputDef) => void;
  onRemove: () => void;
  onRename: (from: string, to: string) => void;
}) {
  const errorId = useId();
  const [problem, setProblem] = useState<string | null>(null);
  return (
    <div className="rounded-control bg-surface-2/50 p-2">
      {/* Narrow (an inspector): name, label and its delete on one line, the kind under them across both fields; from 512px of its own width, all four on one line. */}
      <div className="grid grid-cols-[minmax(0,2fr)_minmax(0,3fr)_auto] gap-1.5 @lg:grid-cols-[minmax(0,2fr)_minmax(0,3fr)_auto_auto]">
        <NameField input={i} taken={(name) => inputs.some((x) => x.name === name)} disabled={disabled} errorId={errorId} onRename={onRename} onProblem={setProblem} />
        <TextInput value={i.label} placeholder={tr("workflow-input-defs-editor-label")} aria-label={tr("workflow-input-defs-editor-input-label")} disabled={disabled} onChange={(e) => onPatch({ label: e.target.value })} />
        <Select
          className="col-span-2 @lg:col-span-1"
          value={i.kind}
          aria-label={tr("workflow-input-defs-editor-input-kind")}
          disabled={disabled}
          onChange={(e) => {
            const kind = e.target.value as InputDef["kind"];
            const base = { name: i.name, label: i.label, required: i.required, default: undefined };
            onReplace(
              (kind === "choice"
                ? { ...base, kind, options: [] }
                : kind === "account"
                  ? { ...base, kind, connector: connectors.rows[0]?.id ?? null }
                  : { ...base, kind }) as InputDef,
            );
          }}
        >
          {INPUT_KINDS.map((k) => (
            <option key={k} value={k}>
              {inputKindWords(k)}
            </option>
          ))}
        </Select>
        <Button size="sm" variant="ghost" className="col-start-3 row-start-1 self-center @lg:col-start-4" disabled={disabled} aria-label={tr("workflow-input-defs-editor-remove-input")} onClick={onRemove}>
          <ICON.delete size={12} aria-hidden />
        </Button>
      </div>
      {/* Why a name was refused or put back, said under the row it was typed in. */}
      <p id={errorId} role="alert" className={problem ? "mt-1 text-2xs text-danger" : "sr-only"}>
        {problem}
      </p>
      <div className="mt-1.5 flex flex-wrap items-center gap-3">
        <Checkbox label={tr("workflow-input-defs-editor-required")} checked={i.required === true} disabled={disabled} onChange={(required) => onPatch({ required })} />
        {i.kind === "account" && (
          <Select
            value={i.connector ?? ""}
            aria-label={tr("workflow-input-defs-editor-account-connector")}
            disabled={disabled}
            onChange={(e) => onPatch({ connector: e.target.value || null } as Partial<InputDef>)}
          >
            <option value="">{connectors.rows.length === 0 ? tr("workflow-input-defs-editor-no-connector-installed-here") : tr("workflow-connector-step-form-pick-connector")}</option>
            {connectors.rows.map((c) => (
              <option key={c.id} value={c.id}>{tr("workflow-input-defs-editor-account", { c: c.name })}</option>
            ))}
            {i.connector && !connectors.rows.some((c) => c.id === i.connector) && <option value={i.connector}>{tr("workflow-input-defs-editor-not-installed-here", { connector: i.connector })}</option>}
          </Select>
        )}
        {i.kind === "choice" && <OptionsField options={i.options} disabled={disabled} onCommit={(options) => onPatch({ options } as Partial<InputDef>)} />}
        {(i.kind === "text" || i.kind === "number" || i.kind === "choice") && (
          <DefaultField input={i} disabled={disabled} onCommit={(value) => onPatch({ default: value } as Partial<InputDef>)} />
        )}
      </div>
    </div>
  );
}

export function InputDefsEditor({
  inputs,
  onChange,
  onRename,
  disabled,
}: {
  inputs: InputDef[];
  onChange: (next: InputDef[]) => void;
  /** The whole rename: the declaration and everything that reads it. */
  onRename: (from: string, to: string) => void;
  disabled?: boolean;
}) {
  const { keyOf, inherit } = useRowKeys(inputs);
  const connectors = useConnectors();
  const patch = (idx: number, p: Partial<InputDef>) =>
    onChange(inputs.map((x, j) => (j === idx ? inherit(x, { ...x, ...p } as InputDef) : x)));
  return (
    <div className="flex flex-col gap-2">
      {inputs.map((i, idx) => (
        <InputRow
          key={keyOf(i)}
          input={i}
          inputs={inputs}
          disabled={disabled}
          onPatch={(p) => patch(idx, p)}
          onReplace={(next) => onChange(inputs.map((x, j) => (j === idx ? inherit(x, next) : x)))}
          onRemove={() => onChange(inputs.filter((_, j) => j !== idx))}
          onRename={onRename}
          connectors={connectors}
        />
      ))}
      <div>
        <Button size="sm" disabled={disabled} onClick={() => onChange([...inputs, { name: nextInputName(inputs), label: "", kind: "text", required: false }])}>
          <ICON.add size={12} aria-hidden />{tr("workflow-input-defs-editor-add-input")}</Button>
      </div>
    </div>
  );
}
