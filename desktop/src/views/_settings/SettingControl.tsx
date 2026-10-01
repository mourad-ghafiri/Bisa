/**
 * One control per settings kind (ide/13): a switch for a bool, a select for a
 * choice, a number box, a text box, JSON in a box for a structured value.
 * Shared by the registry panel in Settings (workspace and machine scope) and
 * the project cards in the Project IDE (project scope), so one setting is
 * drawn one way wherever it is edited.
 */

import { useState } from "react";
import type { SettingDef } from "../../types";
import { SecretInput, Select, Switch, TextArea, TextInput } from "../../ui";
import { isSecretSetting } from "./networkModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export function SettingControl({
  def,
  value,
  onChange,
  disabled,
}: {
  def: SettingDef;
  value: unknown;
  onChange: (v: unknown) => void;
  disabled: boolean;
}) {
  const k = def.kind;
  switch (k.type) {
    case "bool":
      return <Switch checked={Boolean(value)} onChange={onChange} label={def.label} disabled={disabled} />;
    case "integer":
    case "number":
      return (
        <TextInput
          type="number"
          value={String(value ?? "")}
          min={k.min}
          max={k.max}
          step={k.type === "integer" ? 1 : 0.05}
          disabled={disabled}
          className="w-32 tnum"
          onChange={(e) => {
            const n = k.type === "integer" ? parseInt(e.target.value, 10) : Number(e.target.value);
            if (Number.isFinite(n)) onChange(n);
          }}
        />
      );
    case "choice":
      return (
        <Select value={String(value ?? "")} disabled={disabled} onChange={(e) => onChange(e.target.value)}>
          {k.choices.map((c) => (
            <option key={c.value} value={c.value}>
              {c.label}
            </option>
          ))}
        </Select>
      );
    case "text":
      // A proxy URL may carry a password in its userinfo: hidden as typed, the eye shows it (ide/13 §Secret fields).
      if (isSecretSetting(def.key)) return <SecretInput what={t("ui-secret-input-what-url")} value={String(value ?? "")} disabled={disabled} onChange={onChange} />;
      return <TextInput value={String(value ?? "")} disabled={disabled} onChange={(e) => onChange(e.target.value)} />;
    case "structured":
      return <StructuredControl value={value} onChange={onChange} disabled={disabled} />;
  }
}

/** JSON in a box: applied on blur when it parses, refused in place when it does not. */
function StructuredControl({
  value,
  onChange,
  disabled,
}: {
  value: unknown;
  onChange: (v: unknown) => void;
  disabled: boolean;
}) {
  const [text, setText] = useState(() => JSON.stringify(value ?? {}, null, 2));
  const [bad, setBad] = useState(false);
  return (
    <div>
      <TextArea
        value={text}
        rows={4}
        disabled={disabled}
        className="font-mono text-2xs"
        onChange={(e) => {
          setText(e.target.value);
          setBad(false);
        }}
        onBlur={() => {
          try {
            onChange(JSON.parse(text));
          } catch {
            setBad(true);
          }
        }}
      />
      {bad && (
        <p className="mt-1 text-2xs text-danger" role="alert">{t("settings-setting-control-json-nothing-saved")}</p>
      )}
    </div>
  );
}
