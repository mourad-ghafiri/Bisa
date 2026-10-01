/**
 * The inputs a workflow is started with: a name, a label, a kind, whether it
 * is required, and a default. Steps reach them as `{inputs.<name>}` and as
 * value references.
 *
 * Rows are keyed by an identity that follows the row through renames and
 * removals, never by its index: keyed by index, removing the second of three
 * rows hands the third row the second's input element and its focus.
 */

import { useEffect, useRef, useState } from "react";
import type { InputDef } from "../../../types";
import { Button, Checkbox, ICON, Select, TextInput } from "../../../ui";
import { useConnectors } from "../useConnectors";
import { nextInputName } from "../workflowForm.mjs";
import { t as tr } from "../../../i18n/l10n.mjs";

const KINDS: InputDef["kind"][] = ["text", "number", "bool", "choice", "assignee", "project", "account"];

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

const NAME_RE = /^[a-z][a-z0-9_-]{0,31}$/;

/**
 * An input's name, typed as a draft and committed on blur through `onRename`
 * — the whole rename, placeholders and references included — so no step
 * reads a name that is half typed, and no intermediate name is saved.
 */
function NameField({
  input,
  taken,
  disabled,
  onRename,
}: {
  input: InputDef;
  taken: (name: string) => boolean;
  disabled?: boolean;
  onRename: (from: string, to: string) => void;
}) {
  const [draft, setDraft] = useState(input.name);
  useEffect(() => setDraft(input.name), [input.name]);
  const ok = NAME_RE.test(draft) && !(draft !== input.name && taken(draft));
  const commit = () => {
    if (ok && draft !== input.name) onRename(input.name, draft);
    else setDraft(input.name);
  };
  return (
    <TextInput
      className="font-mono"
      value={draft}
      placeholder={tr("workflow-input-defs-editor-name")}
      aria-label={tr("workflow-input-defs-editor-input-name")}
      aria-invalid={!ok}
      title={ok ? tr("workflow-input-defs-editor-steps-read-inputs-name-renaming-renames") : tr("workflow-input-defs-editor-z-0-9-starts-letter-32")}
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
        <div key={keyOf(i)} className="rounded-control border border-border p-2">
          <div className="grid gap-1.5 md:grid-cols-[1fr_1fr_auto_auto]">
            <NameField input={i} taken={(name) => inputs.some((x) => x.name === name)} disabled={disabled} onRename={onRename} />
            <TextInput value={i.label} placeholder={tr("workflow-input-defs-editor-label")} aria-label={tr("workflow-input-defs-editor-input-label")} disabled={disabled} onChange={(e) => patch(idx, { label: e.target.value })} />
            <Select
              value={i.kind}
              aria-label={tr("workflow-input-defs-editor-input-kind")}
              disabled={disabled}
              onChange={(e) => {
                const kind = e.target.value as InputDef["kind"];
                const base = { name: i.name, label: i.label, required: i.required, default: undefined };
                const next = (
                  kind === "choice"
                    ? { ...base, kind, options: [] }
                    : kind === "account"
                      ? { ...base, kind, connector: connectors.rows[0]?.id ?? null }
                      : { ...base, kind }
                ) as InputDef;
                onChange(inputs.map((x, j) => (j === idx ? inherit(x, next) : x)));
              }}
            >
              {KINDS.map((k) => (
                <option key={k} value={k}>
                  {k}
                </option>
              ))}
            </Select>
            <Button size="sm" variant="ghost" disabled={disabled} aria-label={tr("workflow-input-defs-editor-remove-input")} onClick={() => onChange(inputs.filter((_, j) => j !== idx))}>
              <ICON.delete size={12} aria-hidden />
            </Button>
          </div>
          <div className="mt-1.5 flex flex-wrap items-center gap-3">
            <Checkbox label={tr("workflow-input-defs-editor-required")} checked={i.required === true} disabled={disabled} onChange={(required) => patch(idx, { required })} />
            {i.kind === "account" && (
              <Select
                value={i.connector ?? ""}
                aria-label={tr("workflow-input-defs-editor-account-connector")}
                disabled={disabled}
                onChange={(e) => patch(idx, { connector: e.target.value || null } as Partial<InputDef>)}
              >
                <option value="">{connectors.rows.length === 0 ? tr("workflow-input-defs-editor-no-connector-installed-here") : tr("workflow-connector-step-form-pick-connector")}</option>
                {connectors.rows.map((c) => (
                  <option key={c.id} value={c.id}>{tr("workflow-input-defs-editor-account", { c: c.name })}</option>
                ))}
                {i.connector && !connectors.rows.some((c) => c.id === i.connector) && <option value={i.connector}>{tr("workflow-input-defs-editor-not-installed-here", { connector: i.connector })}</option>}
              </Select>
            )}
            {i.kind === "choice" && (
              <TextInput
                className="min-w-48 flex-1 font-mono"
                value={i.options.join(", ")}
                placeholder={tr("workflow-input-defs-editor-options-comma-separated")}
                aria-label={tr("workflow-input-defs-editor-choice-options")}
                disabled={disabled}
                onChange={(e) => patch(idx, { options: e.target.value.split(",").map((s) => s.trim()).filter(Boolean) } as Partial<InputDef>)}
              />
            )}
            {(i.kind === "text" || i.kind === "number" || i.kind === "choice") && (
              <DefaultField input={i} disabled={disabled} onCommit={(value) => patch(idx, { default: value } as Partial<InputDef>)} />
            )}
          </div>
        </div>
      ))}
      <div>
        <Button size="sm" disabled={disabled} onClick={() => onChange([...inputs, { name: nextInputName(inputs), label: "", kind: "text", required: false }])}>
          <ICON.add size={12} aria-hidden />{tr("workflow-input-defs-editor-add-input")}</Button>
      </div>
    </div>
  );
}
