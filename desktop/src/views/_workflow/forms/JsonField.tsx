/**
 * A JSON value typed as text — an output schema, a check's schema. The text
 * is a draft while it is being typed: it is parsed and committed on blur (or
 * ⌘⏎), and until it parses the field says so under itself rather than
 * dropping the keystroke. A controlled textarea re-serialised from the parsed
 * value cannot be typed into — the first `{` fails to parse, nothing is set,
 * and the render puts the old text back — which is why this exists.
 */

import { useEffect, useState } from "react";
import { Field, TextArea } from "../../../ui";
import { t } from "../../../i18n/l10n.mjs";

function shown(value: unknown): string {
  return value === undefined || value === null ? "" : JSON.stringify(value, null, 2);
}

export function JsonField({
  label,
  hint,
  value,
  rows = 5,
  disabled,
  onCommit,
}: {
  label: string;
  hint?: string;
  value: unknown;
  rows?: number;
  disabled?: boolean;
  /** The parsed value; `null` for a blank field. */
  onCommit: (value: unknown | null) => void;
}) {
  const stored = shown(value);
  const [draft, setDraft] = useState(stored);
  const [problem, setProblem] = useState<string | null>(null);
  useEffect(() => {
    setDraft(stored);
    setProblem(null);
  }, [stored]);

  const commit = () => {
    const text = draft.trim();
    if (!text) {
      setProblem(null);
      if (stored !== "") onCommit(null);
      return;
    }
    try {
      const parsed = JSON.parse(text) as unknown;
      setProblem(null);
      if (JSON.stringify(parsed) !== JSON.stringify(value)) onCommit(parsed);
    } catch (e) {
      setProblem(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <Field label={label} hint={problem ? <span className="text-danger">{t("workflow-json-field-not-json", { problem })}</span> : hint}>
      <TextArea
        rows={rows}
        className="font-mono"
        value={draft}
        disabled={disabled}
        aria-invalid={problem !== null}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
            e.preventDefault();
            commit();
          }
        }}
      />
    </Field>
  );
}
