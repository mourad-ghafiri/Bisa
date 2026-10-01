/**
 * Exact matches on an event's fields — `path = value`, every one must hold —
 * as rows of two text fields. A value is a template (a start's over the
 * inputs it listens with, a wait's and a boundary's over the run); a path is
 * dotted. Shared by the signal and the platform filters, and by a payload
 * an emit raises.
 *
 * The wire's shape is a map, so a path is a key: it is committed when the
 * field is left — a half-typed path would otherwise pass through another
 * row's and take its value — and a path another row has is refused, with
 * the reason under the field. The rules are `exactFieldsModel.mjs`'s; this
 * draws.
 */

import { useEffect, useState } from "react";
import { Button, ICON, TextInput } from "../../../ui";
import { addField, fieldRows, removeField, renameField, setFieldValue, type ExactFields } from "./exactFieldsModel.mjs";
import { t } from "../../../i18n/l10n.mjs";

/** One row's path, committed on blur; a refused path reverts and says why. */
function FieldPath({ fields, path, disabled, onChange }: { fields: ExactFields; path: string; disabled?: boolean; onChange: (next: ExactFields) => void }) {
  const [draft, setDraft] = useState(path);
  const [why, setWhy] = useState<string | null>(null);
  useEffect(() => setDraft(path), [path]);
  const commit = () => {
    const r = renameField(fields, path, draft);
    if (r.ok) {
      setWhy(null);
      if (r.fields !== fields) onChange(r.fields);
    } else {
      setWhy(r.reason);
      setDraft(path);
    }
  };
  return (
    <span className="flex min-w-0 flex-1 flex-col gap-0.5">
      <TextInput
        className="font-mono"
        value={draft}
        aria-label={t("workflow-exact-fields-editor-path")}
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

export function ExactFieldsEditor({
  value,
  onChange,
  disabled,
}: {
  value: ExactFields | null | undefined;
  onChange: (next: ExactFields) => void;
  disabled?: boolean;
}) {
  const fields = value ?? {};
  return (
    <div className="flex flex-col gap-1.5">
      {fieldRows(fields).map((row) => (
        <div key={row.path} className="flex items-start gap-2">
          <FieldPath fields={fields} path={row.path} disabled={disabled} onChange={onChange} />
          <span className="pt-1.5 text-2xs text-text-dim">=</span>
          <TextInput className="min-w-0 flex-1 font-mono" value={row.value} aria-label={t("workflow-exact-fields-editor-value")} disabled={disabled} onChange={(e) => onChange(setFieldValue(fields, row.path, e.target.value))} />
          <Button size="sm" variant="ghost" disabled={disabled} aria-label={t("workflow-wait-step-form-remove-field")} onClick={() => onChange(removeField(fields, row.path))}>
            <ICON.delete size={12} aria-hidden />
          </Button>
        </div>
      ))}
      <div>
        <Button size="sm" disabled={disabled} onClick={() => onChange(addField(fields))}>
          <ICON.add size={12} aria-hidden />
          {t("workflow-wait-step-form-add-field")}
        </Button>
      </div>
    </div>
  );
}
