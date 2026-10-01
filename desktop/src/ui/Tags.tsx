/**
 * Tags: reading them, filtering by them, editing them.
 *
 * The workspace ships a catalog of agents, teams, channels and skills. At that
 * size a flat list is a wall, so every screen that lists something tagged
 * gets the same two controls — a row of chips on the object, and a filter bar
 * above the list. One implementation, so "filter by engineering" looks and
 * behaves identically on Agents, Channels, Projects and Workflows.
 *
 * The filter bar is built from what the list actually contains rather than
 * from the documented vocabulary: a tag nobody uses is a button that returns
 * nothing, and offering it is a small lie.
 */

import { useMemo, useState } from "react";
import { Chip } from "./Chip";

/** Normalization, mirroring `bisa-core`'s `Tags`. */
export function normalizeTag(raw: string): string | null {
  let out = "";
  for (const ch of raw.trim()) {
    if (/[a-z0-9]/.test(ch)) out += ch;
    else if (/[A-Z]/.test(ch)) out += ch.toLowerCase();
    else if (/[ \t_\-/.]/.test(ch)) {
      if (!out.endsWith("-")) out += "-";
    } else return null;
  }
  const trimmed = out.replace(/^-+|-+$/g, "");
  return trimmed && trimmed.length <= 32 ? trimmed : null;
}

/** A read-only row of an object's tags. */
export function TagChips({ tags, max }: { tags: string[]; max?: number }) {
  if (!tags.length) return null;
  const shown = max ? tags.slice(0, max) : tags;
  const rest = tags.length - shown.length;
  return (
    <>
      {shown.map((t) => (
        <Chip key={t} tone="quiet">
          {t}
        </Chip>
      ))}
      {rest > 0 && (
        <Chip tone="quiet" title={tags.join(", ")}>
          +{rest}
        </Chip>
      )}
    </>
  );
}

// The filter's facts — what passes, how the words narrow — are `tagSearchModel.mjs`'s,
// shared with every tagged, searchable list; the kit draws the bar.
import { NO_TAG_FILTER } from "./tagSearchModel.mjs";
import type { TagFilterState } from "./tagSearchModel.mjs";
import { t as tr } from "../i18n/l10n.mjs";
export { NO_TAG_FILTER, parseTagFilter, passesTagFilter } from "./tagSearchModel.mjs";
export type { TagFilterState, TagMatch } from "./tagSearchModel.mjs";

/**
 * The filter bar. `items` is the unfiltered list, so the facet counts and the
 * offered tags come from the data on screen.
 *
 * `all` narrows and `any` widens, so the mode toggle only appears once two
 * tags are selected — with one tag the two modes are the same thing, and a
 * control that changes nothing is noise.
 */
export function TagFilterBar<T>({
  items,
  tagsOf,
  value,
  onChange,
  className = "",
}: {
  items: T[];
  tagsOf: (item: T) => string[];
  value: TagFilterState;
  onChange: (next: TagFilterState) => void;
  className?: string;
}) {
  const facets = useMemo(() => {
    const counts = new Map<string, number>();
    for (const item of items) {
      for (const tag of tagsOf(item)) counts.set(tag, (counts.get(tag) ?? 0) + 1);
    }
    return [...counts.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
    // `tagsOf` is a stable accessor at every call site; keying on it would
    // rebuild the facets on every render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [items]);

  if (facets.length < 2) return null;

  const toggle = (tag: string) => {
    const selected = value.selected.includes(tag)
      ? value.selected.filter((t) => t !== tag)
      : [...value.selected, tag];
    onChange({ ...value, selected });
  };

  return (
    <div className={`flex flex-wrap items-center gap-1 ${className}`}>
      {facets.map(([tag, count]) => {
        const on = value.selected.includes(tag);
        return (
          <button
            key={tag}
            type="button"
            onClick={() => toggle(tag)}
            aria-pressed={on}
            className={`inline-flex h-6 items-center gap-1 rounded-full border px-2 text-2xs font-medium transition-colors ${
              on
                ? "border-transparent bg-accent-soft text-accent-ink"
                : "border-border text-text-dim hover:bg-surface-2"
            }`}
          >
            {tag}
            <span className="tnum opacity-60">{count}</span>
          </button>
        );
      })}
      {value.selected.length > 1 && (
        <button
          type="button"
          onClick={() => onChange({ ...value, match: value.match === "any" ? "all" : "any" })}
          title={
            value.match === "any"
              ? tr("ui-tags-showing-anything-one-selected-tags-switch")
              : tr("ui-tags-showing-only-what-carries-every-selected")
          }
          className="h-6 rounded-full border border-border px-2 text-2xs text-text-dim hover:bg-surface-2"
        >{tr("ui-tags-match", { match: value.match })}</button>
      )}
      {value.selected.length > 0 && (
        <button
          type="button"
          onClick={() => onChange(NO_TAG_FILTER)}
          className="h-6 rounded-full px-2 text-2xs text-text-dim underline-offset-2 hover:underline"
        >{tr("ui-tags-clear")}</button>
      )}
    </div>
  );
}

/**
 * Editing an object's tags.
 *
 * Rejects what will not slugify instead of silently mangling it, matching the
 * store: a tag that does not round-trip is a filter that lies. `suggestions`
 * is the documented vocabulary, offered rather than enforced — free-form tags
 * are legal, they are just not what the bundled library uses.
 */
export function TagInput({
  value,
  onChange,
  suggestions = [],
  id,
  disabled = false,
}: {
  value: string[];
  onChange: (next: string[]) => void;
  suggestions?: string[];
  id?: string;
  /** Read-only: the chips show, nothing adds or removes. */
  disabled?: boolean;
}) {
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);

  const add = (raw: string) => {
    const tag = normalizeTag(raw);
    if (!tag) {
      setError(tr("ui-tags-not-tag", { raw: raw.trim() }));
      return;
    }
    setError(null);
    setDraft("");
    if (!value.includes(tag)) onChange([...value, tag].sort());
  };

  const unused = suggestions.filter((t) => !value.includes(t));

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex flex-wrap items-center gap-1">
        {value.map((tag) => (
          <button
            key={tag}
            type="button"
            title={disabled ? undefined : tr("ui-tags-remove")}
            disabled={disabled}
            onClick={() => onChange(value.filter((t) => t !== tag))}
            className="inline-flex h-5 items-center gap-1 rounded-full border border-border px-2 text-2xs text-text-dim hover:bg-surface-2"
          >
            {tag}
            <span aria-hidden className="opacity-50">
              ×
            </span>
          </button>
        ))}
        <input
          id={id}
          value={draft}
          disabled={disabled}
          placeholder={value.length ? tr("ui-tags-add") : tr("ui-tags-engineering-product")}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === ",") {
              e.preventDefault();
              if (draft.trim()) add(draft);
            } else if (e.key === "Backspace" && !draft && value.length) {
              onChange(value.slice(0, -1));
            }
          }}
          onBlur={() => draft.trim() && add(draft)}
          className="h-6 min-w-24 flex-1 bg-transparent text-xs outline-none placeholder:text-text-dim/60"
        />
      </div>
      {error && <p className="text-2xs text-danger">{error}</p>}
      {unused.length > 0 && (
        <div className="flex flex-wrap gap-1">
          {unused.map((tag) => (
            <button
              key={tag}
              type="button"
              onClick={() => add(tag)}
              className="h-5 rounded-full px-1.5 text-2xs text-text-dim/70 hover:bg-surface-2 hover:text-text-dim"
            >
              +{tag}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/**
 * The vocabulary the bundled library draws from, mirroring
 * `bisa-core::tags::VOCABULARY`. Offered as suggestions; anything that
 * slugifies is accepted.
 *
 * A hand-copy with nothing binding it to the Rust, so it drifts silently: M12
 * added `legal` and this list kept offering the old twenty, which does not
 * break tagging — free-form tags are still accepted — it just stops offering
 * the word the catalog files a whole agent under. `tagVocabulary.test.mjs`
 * now reads `tags.rs` and asserts the two agree, because the failure here is
 * an absence and nobody files a bug about a suggestion they never saw.
 */
export const TAG_VOCABULARY = [
  "business",
  "code",
  "content",
  "data",
  "delivery",
  "design",
  "discovery",
  "engineering",
  "legal",
  "management",
  "marketing",
  "mobile",
  "ops",
  "planning",
  "product",
  "quality",
  "research",
  "review",
  "sales",
  "security",
  "thinking",
  "web",
  "writing",
] as const;
