/**
 * A template typed as a draft and committed on blur or Enter — what a
 * `connector` step's parameters and a connector poll's take: a value that
 * can be long, and one whose every keystroke must not rewrite the design
 * (a draft re-rendered on each would drop a space typed before a word). A
 * draft still open when the field unmounts is committed on the way out.
 */

import { useEffect, useState } from "react";
import { TextArea, TextInput, useCommitOnUnmount } from "../../../ui";

export function TemplateField({
  value,
  rows,
  disabled,
  ariaLabel,
  placeholder,
  onCommit,
}: {
  value: string;
  rows?: number;
  disabled?: boolean;
  ariaLabel: string;
  /** What an empty field would send — a parameter's default. */
  placeholder?: string;
  onCommit: (next: string) => void;
}) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  const commit = () => {
    if (draft !== value) onCommit(draft);
  };
  useCommitOnUnmount(draft !== value ? draft : null, onCommit);
  if (rows && rows > 1) {
    return <TextArea rows={rows} className="font-mono" value={draft} aria-label={ariaLabel} placeholder={placeholder} disabled={disabled} onChange={(e) => setDraft(e.target.value)} onBlur={commit} />;
  }
  return (
    <TextInput
      className="font-mono"
      value={draft}
      aria-label={ariaLabel}
      placeholder={placeholder}
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
