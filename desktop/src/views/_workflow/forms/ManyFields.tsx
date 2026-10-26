/**
 * The controls a field that takes one value or a list is drawn with
 * (`manyModel.mjs`): a closed set as checkboxes — any checked word matches,
 * and the set's own default stays checked alone, since the wire reads an
 * absent field as it — and free words as chips with one line to add the
 * next. The chips are the kit's tags' in shape and not in grammar: a branch,
 * a glob, a topic, an id is written as typed. Shared by every event filter,
 * a start's as a wait's.
 */

import { useState } from "react";
import { Checkbox, ICON, cn } from "../../../ui";
import { addWord, removeWord, toggleWord, wordsOr, type OneOrMany } from "./manyModel.mjs";
import { t } from "../../../i18n/l10n.mjs";

/** A closed set, any checked word matching. `fallback` is what an absent field means and cannot be unchecked alone; `null` when absent means any. */
export function WordChecks<W extends string>({
  options,
  value,
  fallback,
  onChange,
  disabled,
  columns = 2,
}: {
  options: readonly { word: W; label: string }[];
  value: OneOrMany<W> | null | undefined;
  fallback: W | null;
  onChange: (next: OneOrMany<W> | null) => void;
  disabled?: boolean;
  columns?: 2 | 3;
}) {
  const checked = wordsOr(value, fallback);
  return (
    <div className={cn("grid gap-x-4 gap-y-1", columns === 3 ? "grid-cols-2 @sm:grid-cols-3" : "grid-cols-1 @sm:grid-cols-2")}>
      {options.map((o) => {
        const on = checked.includes(o.word);
        const alone = fallback !== null && on && checked.length === 1;
        return <Checkbox key={o.word} label={o.label} checked={on} disabled={disabled || alone} onChange={() => onChange(toggleWord(checked, o.word))} />;
      })}
    </div>
  );
}

/**
 * Free words as chips and a line to add one: Enter, a comma or leaving the
 * line adds; Backspace on an empty line takes the last chip back. A
 * suggestion not yet chosen is a click away.
 */
export function WordsInput({
  value,
  onChange,
  disabled,
  placeholder,
  mono,
  suggestions = [],
  labelOf = (word) => word,
  "aria-label": ariaLabel,
}: {
  value: OneOrMany<string> | null | undefined;
  onChange: (next: OneOrMany<string> | null) => void;
  disabled?: boolean;
  placeholder?: string;
  /** The words are the machine's — a glob, a topic, an id. */
  mono?: boolean;
  /** Offered beside the line, each a click away, the chosen ones left out. */
  suggestions?: readonly { word: string; label: string }[];
  /** How a word reads on its chip when not as written — a channel's name for its id. */
  labelOf?: (word: string) => string;
  "aria-label"?: string;
}) {
  const [draft, setDraft] = useState("");
  const words = Array.isArray(value) ? value : value ? [value] : [];
  const add = (raw: string) => {
    setDraft("");
    const next = addWord(value, raw);
    if (next !== (value ?? null)) onChange(next);
  };
  const offered = suggestions.filter((s) => !words.includes(s.word));
  return (
    <div className="flex flex-col gap-1.5">
      <div className={cn("control-group anim flex flex-wrap items-center gap-1 rounded-control border border-border bg-surface px-1.5 py-1", !disabled && "hover:border-text-dim/40")}>
        {words.map((word) => (
          <button
            key={word}
            type="button"
            title={disabled ? undefined : t("workflow-many-fields-remove", { word })}
            aria-label={disabled ? labelOf(word) : t("workflow-many-fields-remove", { word: labelOf(word) })}
            disabled={disabled}
            onClick={() => onChange(removeWord(value, word))}
            className={cn("anim inline-flex h-5 max-w-full items-center gap-1 rounded-full bg-surface-2 pr-1.5 pl-2 text-2xs text-text hover:bg-selected disabled:opacity-60", mono && "font-mono")}
          >
            <span className="truncate">{labelOf(word)}</span>
            {!disabled && <ICON.close size={10} aria-hidden className="shrink-0 text-text-dim" />}
          </button>
        ))}
        <input
          value={draft}
          disabled={disabled}
          aria-label={ariaLabel}
          autoCapitalize="off"
          autoCorrect="off"
          placeholder={words.length ? t("workflow-many-fields-add") : placeholder}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === ",") {
              e.preventDefault();
              if (draft.trim()) add(draft);
            } else if (e.key === "Backspace" && !draft && words.length) {
              onChange(removeWord(value, words[words.length - 1]));
            }
          }}
          onBlur={() => draft.trim() && add(draft)}
          className={cn("h-5 min-w-24 flex-1 bg-transparent px-1 text-xs outline-none placeholder:text-text-dim", mono && "font-mono")}
        />
      </div>
      {offered.length > 0 && !disabled && (
        <div className="flex flex-wrap items-center gap-1">
          {offered.map((s) => (
            <button
              key={s.word}
              type="button"
              onClick={() => add(s.word)}
              aria-label={t("workflow-many-fields-add-word", { word: s.label })}
              className="anim inline-flex h-5 max-w-full items-center gap-0.5 rounded-full px-1.5 text-2xs text-text-dim hover:bg-surface-2 hover:text-text"
            >
              <ICON.add size={10} aria-hidden className="shrink-0" />
              <span className="truncate">{s.label}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
