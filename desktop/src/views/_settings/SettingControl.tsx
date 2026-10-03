/**
 * One control per settings kind (ide/13): a switch for a bool, a select for a
 * choice, a number box, a text box, JSON in a box for a structured value.
 * Shared by the registry panel in Settings (workspace and machine scope) and
 * the project cards in the Project IDE (project scope), so one setting is
 * drawn one way wherever it is edited.
 *
 * A switch and a choice commit as they move — one gesture, one value. A box
 * you type into keeps a draft and commits it on blur or Enter (Escape puts
 * the value back): a write per keystroke would write every half-typed value,
 * and a box disabled under its own write would lose the caret between
 * letters. What a draft commits is `settingDraftModel.mjs`'s.
 */

import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import type { SettingDef } from "../../types";
import { NumberInput, SecretInput, Select, Switch, TextArea, TextInput } from "../../ui";
import { isSecretSetting } from "./networkModel.mjs";
import { committedValue, draftOf } from "./settingDraftModel.mjs";
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
      return (
        <NumberInput
          value={typeof value === "number" ? value : Number(value ?? k.min ?? 0)}
          min={k.min}
          max={k.max}
          disabled={disabled}
          className="w-32 tnum"
          onCommit={onChange}
        />
      );
    case "number":
    case "text":
      return <DraftControl def={def} value={value} onChange={onChange} disabled={disabled} />;
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
    case "structured":
      return <StructuredControl value={value} onChange={onChange} disabled={disabled} />;
  }
}

/** A text, secret or decimal box: a draft, committed on blur or Enter. */
function DraftControl({
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
  const [draft, setDraft] = useState(() => draftOf(value));
  // The value moved under the box (a write landed, another window wrote it): the box follows.
  useEffect(() => setDraft(draftOf(value)), [value]);
  // Leaving by a chord (⌘K, a panel link) unmounts the box without a blur:
  // what was typed is committed then too, never dropped. An unchanged draft
  // commits nothing (`committedValue`).
  const latest = useRef({ draft, value, onChange, kind: def.kind });
  latest.current = { draft, value, onChange, kind: def.kind };
  useEffect(
    () => () => {
      const { draft: typed, value: held, onChange: write, kind } = latest.current;
      const next = committedValue(kind, typed, held);
      if (next !== undefined) write(next);
    },
    [],
  );
  const commit = () => {
    const next = committedValue(def.kind, draft, value);
    if (next === undefined) setDraft(draftOf(value));
    else onChange(next);
  };
  const keys = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault();
      commit();
    } else if (e.key === "Escape") {
      setDraft(draftOf(value));
    }
  };
  if (def.kind.type === "number") {
    return (
      <TextInput
        inputMode="decimal"
        value={draft}
        disabled={disabled}
        className="w-32 tnum"
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={keys}
      />
    );
  }
  // A proxy URL may carry a password in its userinfo: hidden as typed, the eye shows it (ide/13 §Secret fields).
  if (isSecretSetting(def.key)) return <SecretInput what={t("ui-secret-input-what-url")} value={draft} disabled={disabled} onChange={setDraft} onBlur={commit} onKeyDown={keys} />;
  return <TextInput value={draft} disabled={disabled} onChange={(e) => setDraft(e.target.value)} onBlur={commit} onKeyDown={keys} />;
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
        aria-invalid={bad || undefined}
        onChange={(e) => {
          setText(e.target.value);
          setBad(false);
        }}
        onBlur={() => {
          try {
            const next = JSON.parse(text);
            // Unchanged JSON writes nothing — a blur is not an edit.
            if (JSON.stringify(next) !== JSON.stringify(value ?? {})) onChange(next);
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
