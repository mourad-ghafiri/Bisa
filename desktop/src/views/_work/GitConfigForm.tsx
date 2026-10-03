/**
 * The one git-config form, drawn from the schema the node serves:
 * a control per key by its kind, and — for a project's local layer — what the
 * key inherits from the global layer and an *Inherit* action that clears the
 * local value. Settings → Git shows it for the global layer; the project
 * dialog, the ask dialog and About › Settings for a repository's local
 * layer. The rows, the write and the validation are `gitConfigModel.mjs`'s;
 * the edits are the caller's (`useConfigEdits`), so a dialog can hold them
 * beside its other fields and submit them with the rest.
 */
import { useCallback, useMemo, useState } from "react";
import type { GitConfigEntry, GitConfigKey } from "../../types";
import { useSessionDraft } from "./gitPanelStore";
import { Button, Field, Select, Switch, TextInput } from "../../ui";
import { configRows, diffWrites, isEmptyWrite, validateEdits } from "./gitConfigModel.mjs";
import type { ConfigRow, ConfigScope, ConfigWrite } from "./gitConfigModel.mjs";
import { t } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";

interface ConfigEdits {
  rows: ConfigRow[];
  edits: Record<string, string>;
  setEdit: (key: string, value: string) => void;
  reset: () => void;
  write: ConfigWrite;
  problems: Record<string, string>;
  dirty: boolean;
  valid: boolean;
}

const NO_EDITS: Record<string, string> = Object.freeze({});

/**
 * The edits behind one form: what the person typed, and what it amounts to.
 * With a `draftKey` the edits live in the git panel store and outlive the
 * form's mount (the card under About › Settings); without one they are the
 * dialog's own and go with it.
 */
export function useConfigEdits(schema: readonly GitConfigKey[], entries: readonly GitConfigEntry[], scope: ConfigScope, draftKey?: string): ConfigEdits {
  const rows = useMemo(() => configRows(schema, entries, scope), [schema, entries, scope]);
  const [local, setLocal] = useState<Record<string, string>>(NO_EDITS);
  const [kept, setKept] = useSessionDraft<Record<string, string>>(draftKey ?? "gitconfig|unkept", NO_EDITS);
  const edits = draftKey ? kept : local;
  const setEdits = draftKey ? setKept : setLocal;
  const setEdit = useCallback((key: string, value: string) => setEdits((e) => ({ ...e, [key]: value })), [setEdits]);
  const reset = useCallback(() => setEdits(NO_EDITS), [setEdits]);
  const write = useMemo(() => diffWrites(rows, edits), [rows, edits]);
  const problems = useMemo(() => validateEdits(rows, edits), [rows, edits]);
  return {
    rows,
    edits,
    setEdit,
    reset,
    write,
    problems,
    dirty: !isEmptyWrite(write),
    valid: Object.keys(problems).length === 0,
  };
}

export function GitConfigForm({
  form,
  scope,
  only,
  omit,
  autoFocus,
}: {
  form: ConfigEdits;
  scope: ConfigScope;
  /** Show these keys alone (the ask dialog's identity first); omit for every key. */
  only?: readonly string[];
  /** Hide these keys — the ones another control draws (`codehost.account` is a Select of stored logins). */
  omit?: readonly string[];
  autoFocus?: boolean;
}) {
  const rows = form.rows.filter((r) => r.editable && (!only || only.includes(r.key)) && !(omit ?? []).includes(r.key));
  return (
    <div className="flex flex-col gap-3">
      {rows.map((row, i) => {
        const value = row.key in form.edits ? form.edits[row.key] : (row.value ?? "");
        const problem = form.problems[row.key];
        const inheriting = scope === "local" && value === "" && row.inherited !== null;
        const hint = problem ? (
          <span className="text-danger">{problem}</span>
        ) : inheriting ? (
          <>{rich("work-git-config-form-inherits-from-global", { value: <span className="font-mono">{row.inherited}</span> })}</>
        ) : scope === "local" && value === "" ? (
          t("work-git-config-form-unset-everywhere", { hint: row.hint })
        ) : (
          row.hint
        );
        return (
          <Field key={row.key} label={row.label} hint={hint}>
            <div className="flex items-center gap-2">
              <ConfigControl row={row} value={value} autoFocus={autoFocus && i === 0} onChange={(v) => form.setEdit(row.key, v)} />
              {scope === "local" && value !== "" && (
                <Button size="sm" variant="ghost" onClick={() => form.setEdit(row.key, "")} title={t("work-git-config-form-clear-repository-s-value-so-inherits")}>{t("work-git-config-form-inherit")}</Button>
              )}
            </div>
          </Field>
        );
      })}
    </div>
  );
}

function ConfigControl({ row, value, autoFocus, onChange }: { row: ConfigRow; value: string; autoFocus?: boolean; onChange: (v: string) => void }) {
  switch (row.kind?.type) {
    case "bool":
      return (
        <div className="flex items-center gap-2">
          {/* The switch is named by its row; the one word beside it is git's own value — `true`, `false` — or that nothing is set. */}
          <Switch checked={value === "true"} onChange={(on) => onChange(on ? "true" : "false")} label={row.label} hideLabel />
          <span className="font-mono text-2xs text-text-dim">{value === "" ? t("work-git-config-form-unset") : value}</span>
        </div>
      );
    case "choice":
      return (
        <Select value={value} onChange={(e) => onChange(e.target.value)} className="w-40">
          <option value="">{row.inherited !== null ? t("work-git-config-form-inherit-value", { inherited: row.inherited }) : t("work-git-config-form-unset")}</option>
          {(row.kind.options ?? []).map((o) => (
            <option key={o} value={o}>
              {o}
            </option>
          ))}
        </Select>
      );
    default:
      return (
        <TextInput
          autoFocus={autoFocus}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={row.inherited ?? ""}
          className={row.key === "user.email" ? "w-64 font-mono" : "w-64"}
        />
      );
  }
}
