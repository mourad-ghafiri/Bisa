/**
 * One way to pick an agent.
 *
 * There were seven "pick an agent" interactions and six of them were hand
 * rolled: the `@` autocomplete, the address tray, the channel roster picker,
 * the new-DM dialog, the assignee picker, the Message button on an agent card
 * and the omnibox. They shared no row, no match predicate and no notion of
 * whether the agent could answer at all, so the same typing found different
 * agents in each, and only two of them ever showed a description or a tag —
 * the two you cannot address from.
 *
 * **Every row here answers "can this one help me?"** — a role line, the tags,
 * the harness — and, when the answer is no, says why in the row rather than
 * quietly offering a name that will never reply.
 *
 * The interaction is the omnibox's, because that is the one fully accessible
 * list in the app: focus stays in a `role="combobox"` input and
 * `aria-activedescendant` moves, so a screen reader hears one control with a
 * moving selection rather than forty controls to walk. The rows are real
 * buttons carrying `role="option"` and `tabIndex={-1}` — reachable by pointer,
 * not a second set of tab stops.
 *
 * The selection semantics are the assignee picker's: a value of ids, a `max`,
 * and `max === 1` swapping rather than accumulating. Everything either of them
 * can get *wrong* lives in `agentPickerModel.mjs`, where a test can reach it.
 */

import { useEffect, useId, useMemo, useRef, useState, type ReactNode } from "react";
import { Avatar } from "./Avatar";
import { Chip } from "./Chip";
import { cn } from "./cn";
import { EmptyState } from "./EmptyState";
import { TextInput } from "./Field";
import { ICON } from "./icons";
import { ScrollArea } from "./ScrollArea";
import { NO_TAG_FILTER, TagFilterBar, passesTagFilter, type TagFilterState } from "./Tags";
import { clampCursor, cursorAction, groupRows, rank, toggleValue } from "./agentPickerModel.mjs";
import { t as tr } from "../i18n/l10n.mjs";

/**
 * One pickable principal, in the shape a row needs to draw it.
 *
 * `id` is what `value` speaks, and which id that is belongs to the caller: a
 * roster stores agent definition ids, an address tray stores pubkeys, the
 * assignee picker stores a wire key. The picker never interprets it.
 */
export interface AgentCandidate {
  id: string;
  name: string;
  /**
   * `channel` is a channel's own handle — one token that expands to a whole
   * roster at post time. It is drawn differently from a person on purpose:
   * picking it addresses several agents at once, and a row that looked like
   * everything else would hide that.
   */
  kind: "agent" | "human" | "channel" | "team";
  /** The line that says what this one is for. */
  description?: string | null;
  harness?: string | null;
  tags?: string[];
  /** Identicon seed. A stable id, never a name. Defaults to `id`. */
  avatar?: string;
  /** The picture, by content hash (ide/14 §Photos). */
  photo?: { sha256: string } | null;
  /**
   * Why this one may not answer — disabled, or running on a harness this node
   * has not installed. Shown in the row; never a reason to drop it. An id you
   * cannot see is one you cannot reason about, which is the same rule
   * `participants.ts` holds for an unresolvable roster id.
   */
  warning?: string | null;
  /** Picking it here would do nothing: the row is shown, marked, unpickable. */
  blocked?: boolean;
  /** Header this row sits under. Absent puts it in one unlabelled bucket. */
  group?: string;
}

const KIND_LABEL: Record<AgentCandidate["kind"], string> = {
  agent: "agent",
  human: "person",
  channel: "roster",
  team: "team",
};

/** The leading mark: a face for a principal, a glyph for a channel handle. */
function Leading({ c, size }: { c: AgentCandidate; size: number }) {
  if (c.kind === "channel") {
    return (
      <span
        aria-hidden
        className="flex shrink-0 items-center justify-center rounded-full bg-surface-2 text-2xs font-semibold text-text-dim"
        style={{ width: size, height: size }}
      >
        #
      </span>
    );
  }
  return <Avatar id={c.avatar ?? c.id} name={c.name} photo={c.photo} size={size} />;
}

/**
 * The row every agent list in the studio draws.
 *
 * Exported on its own because two controls need the row without the combobox
 * around it: the `@` autocomplete, whose keyboard belongs to the textarea
 * underneath, and the assignee picker, which asks a different question —
 * *assign* is not *address* — but must not answer it in a different visual
 * language.
 */
export function AgentRow({
  candidate: c,
  selected = false,
  active = false,
  compact = false,
  standalone = false,
  id,
  onSelect,
  onHover,
}: {
  candidate: AgentCandidate;
  selected?: boolean;
  /** The cursor is on this row. Presentation only; focus stays in the input. */
  active?: boolean;
  /** Drop the tag chips: a dropdown floating over a textarea has no room. */
  compact?: boolean;
  /**
   * Not inside a `role="listbox"`. `role="option"` outside a listbox names a
   * relationship that is not there, so a caller with its own list markup gets
   * a plain toggle button with `aria-pressed` instead.
   */
  standalone?: boolean;
  id?: string;
  onSelect: () => void;
  /** Pointing at a row moves the cursor to it, so the two never disagree. */
  onHover?: () => void;
}) {
  const tags = c.tags ?? [];
  return (
    <button
      id={id}
      type="button"
      role={standalone ? undefined : "option"}
      aria-selected={standalone ? undefined : active}
      aria-pressed={standalone ? selected : undefined}
      aria-disabled={c.blocked || undefined}
      // Focus stays in the combobox input above; these are reached with the
      // arrow keys, so they must not also be tab stops. A standalone row has
      // no input above it and is an ordinary control.
      tabIndex={standalone ? undefined : -1}
      onMouseEnter={onHover}
      onMouseDown={(e) => {
        // The `@` dropdown sits over a textarea that must keep its caret, and
        // a mousedown that moves focus first would close the list under the
        // pointer before the click landed.
        if (!standalone) e.preventDefault();
      }}
      onClick={() => {
        if (!c.blocked) onSelect();
      }}
      className={cn(
        "anim flex w-full items-start gap-2 rounded-control px-2 py-1.5 text-left",
        // The cursor is where you are, not a call: the neutral wash, never the accent.
        active ? "bg-selected text-text" : "text-text hover:bg-surface-2",
        c.blocked && "opacity-55",
      )}
    >
      {/* The tick keeps its slot whether or not it is drawn, so picking a row
          does not shift every label in the list sideways. */}
      <span aria-hidden className="mt-0.5 flex w-3 shrink-0 justify-center">
        {selected && <ICON.check size={11} />}
      </span>
      <Leading c={c} size={compact ? 18 : 22} />
      <span className="min-w-0 flex-1">
        <span className="flex min-w-0 items-center gap-1.5">
          <span className="truncate text-xs font-medium">{c.name}</span>
          {c.kind !== "agent" && (
            <span className="shrink-0 text-2xs text-text-dim">{KIND_LABEL[c.kind]}</span>
          )}
        </span>
        {c.description && (
          <span className="mt-0.5 line-clamp-2 block text-2xs text-text-dim">{c.description}</span>
        )}
        {(c.harness || c.warning || (!compact && tags.length > 0)) && (
          <span className="mt-1 flex flex-wrap items-center gap-1">
            {c.harness && <Chip tone="quiet">{c.harness}</Chip>}
            {!compact &&
              tags.slice(0, 3).map((t) => (
                <Chip key={t} tone="quiet">
                  {t}
                </Chip>
              ))}
            {c.warning && (
              <Chip tone="warn" icon={ICON.warn}>
                {c.warning}
              </Chip>
            )}
          </span>
        )}
      </span>
    </button>
  );
}

/** A chosen id as a removable chip. An id nothing resolves is marked, not hidden. */
export function AgentChip({
  candidate,
  name,
  onRemove,
}: {
  /** Null when the id resolves to nothing here — see {@link AgentPicker}. */
  candidate: AgentCandidate | null;
  /** What to call it when it resolves to nothing. */
  name: string;
  onRemove?: () => void;
}) {
  const label = candidate?.name ?? name;
  return (
    <span
      className={cn(
        "inline-flex max-w-full items-center gap-1 rounded-full border py-0.5 pr-1 pl-1.5 text-2xs",
        candidate ? "border-border" : "border-dashed border-border text-text-dim",
      )}
      title={candidate?.warning ?? (candidate ? undefined : tr("ui-agent-picker-list"))}
    >
      {candidate ? (
        <Leading c={candidate} size={14} />
      ) : (
        <ICON.warn size={11} aria-hidden className="shrink-0 text-warn" />
      )}
      <span className="max-w-36 truncate">{label}</span>
      {candidate?.warning && <ICON.warn size={10} aria-hidden className="shrink-0 text-warn" />}
      {onRemove && (
        <button
          type="button"
          onClick={onRemove}
          aria-label={tr("ui-agent-picker-remove", { label })}
          className="anim rounded-full px-1 text-text-dim hover:text-danger"
        >
          <ICON.close size={10} aria-hidden />
        </button>
      )}
    </span>
  );
}

/**
 * Pick one or many principals, by name, role, tag or harness.
 *
 * `max === 1` swaps the choice rather than refusing it. Nothing here closes
 * the surface it sits in: the picker does not know whether it is in a dialog,
 * a popover or a panel, and a control that dismissed its own container would
 * be wrong in two of the three.
 */
export function AgentPicker({
  candidates,
  value,
  onChange,
  max,
  label = tr("ui-agent-picker-search-agents"),
  placeholder = tr("ui-agent-picker-search-name-role-tag"),
  autoFocus = false,
  chips = true,
  facets = true,
  emptyTitle = tr("ui-agent-picker-nobody-pick-yet"),
  emptyHint,
  emptyAction,
  note,
  listClassName = "max-h-64",
}: {
  candidates: AgentCandidate[];
  /** Chosen ids, in the order they were picked. */
  value: string[];
  onChange: (next: string[]) => void;
  /** Cap on selections; `1` swaps the choice instead of adding to it. */
  max?: number;
  label?: string;
  placeholder?: string;
  autoFocus?: boolean;
  /** Draw the chosen above the input. Off where the caller draws them itself. */
  chips?: boolean;
  facets?: boolean;
  emptyTitle?: string;
  /**
   * What to do instead, when there is nothing here to pick. Since M13 the
   * fastest way to reach an agent is often not to pick one — just ask the
   * room and triage routes it — so an empty picker should say so rather than
   * leaving a reader looking for a list that is not coming.
   */
  emptyHint?: string;
  emptyAction?: ReactNode;
  note?: ReactNode;
  listClassName?: string;
}) {
  const listId = useId();
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<TagFilterState>(NO_TAG_FILTER);
  const [cursor, setCursor] = useState(0);
  const listEl = useRef<HTMLDivElement>(null);

  const index = useMemo(() => new Map(candidates.map((c) => [c.id, c])), [candidates]);

  const pool = useMemo(
    () => candidates.filter((c) => passesTagFilter(c.tags ?? [], filter)),
    [candidates, filter],
  );
  const rows = useMemo(() => rank(pool, query), [pool, query]);
  const groups = useMemo(() => groupRows(rows), [rows]);
  const flat = useMemo(() => groups.flatMap((g) => g.rows), [groups]);

  // A filtered list gets shorter under a cursor pointing at row nine, and an
  // `aria-activedescendant` naming a row that is gone is announced as nothing.
  useEffect(() => setCursor((c) => clampCursor(c, flat.length)), [flat.length]);

  // Keep the highlighted row on screen when the arrow keys walk past the fold.
  useEffect(() => {
    listEl.current
      ?.querySelector<HTMLElement>('[aria-selected="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [cursor, flat]);

  const rowId = (i: number) => `${listId}-row-${i}`;
  const activeId = flat[cursor] ? rowId(cursor) : undefined;

  const pick = (c: AgentCandidate) => {
    if (c.blocked) return;
    // The rule is the model's, over `value` alone — never over `candidates`,
    // so an id the tray remembers from another scope is still removable. A
    // full picker answers the same array, and nothing is said.
    const next = toggleValue(value, c.id, max);
    if (next !== value) onChange(next);
  };

  const full = max !== undefined && max !== 1 && value.length >= max;
  /** Empty because of a search is a different fact from empty full stop. */
  const narrowed = Boolean(query) || filter.selected.length > 0;

  return (
    <div className="flex min-w-0 flex-col gap-1.5">
      {chips && (
        <div className="flex min-h-6 flex-wrap items-center gap-1">
          {value.length === 0 ? (
            <span className="text-2xs text-text-dim">{tr("ui-agent-picker-nobody-yet")}</span>
          ) : (
            value.map((id) => (
              <AgentChip
                key={id}
                candidate={index.get(id) ?? null}
                name={id}
                onRemove={() => onChange(value.filter((k) => k !== id))}
              />
            ))
          )}
        </div>
      )}

      <TextInput
        value={query}
        autoFocus={autoFocus}
        placeholder={placeholder}
        role="combobox"
        aria-expanded
        aria-controls={listId}
        aria-activedescendant={activeId}
        aria-label={label}
        aria-autocomplete="list"
        autoComplete="off"
        spellCheck={false}
        onChange={(e) => {
          setQuery(e.target.value);
          setCursor(0);
        }}
        onKeyDown={(e) => {
          const action = cursorAction(flat.length, cursor, e.key);
          if (!action) return;
          if (action.type === "dismiss") {
            // Escape clears the query rather than closing anything: this
            // control does not own the dialog or popover it sits in.
            if (query) {
              e.preventDefault();
              setQuery("");
            }
            return;
          }
          e.preventDefault();
          if (action.type === "move") setCursor(action.index);
          else {
            const c = flat[action.index];
            if (c) pick(c);
          }
        }}
      />

      {facets && (
        <TagFilterBar
          items={candidates}
          tagsOf={(c) => c.tags ?? []}
          value={filter}
          onChange={setFilter}
        />
      )}

      {/* A ScrollArea rather than raw overflow: this list is usually inside a
          dialog or a popover over a dark surface, where the OS scrollbar is
          drawn from the system palette and not from the theme. */}
      <ScrollArea className={listClassName} viewportClassName="pr-0.5">
        <div ref={listEl} id={listId} role="listbox" aria-label={label}>
          {flat.length === 0 ? (
            <EmptyState
              className="border-0 px-2 py-5"
              icon={ICON.agent}
              title={narrowed ? tr("ui-agent-picker-nothing-matches") : emptyTitle}
              hint={narrowed ? tr("ui-agent-picker-clear-search-see-everyone") : emptyHint}
              action={narrowed ? null : (emptyAction ?? null)}
            />
          ) : (
            groups.map((g) => (
              <div key={g.label} className="mb-1 last:mb-0">
                {g.label && (
                  <p className="px-2 pt-1.5 pb-1 text-2xs font-semibold text-text-dim">
                    {g.label}
                  </p>
                )}
                {g.rows.map((c) => {
                  const i = flat.indexOf(c);
                  return (
                    <AgentRow
                      key={c.id}
                      id={rowId(i)}
                      candidate={c}
                      selected={value.includes(c.id)}
                      active={i === cursor}
                      onHover={() => setCursor(i)}
                      onSelect={() => pick(c)}
                    />
                  );
                })}
              </div>
            ))
          )}
        </div>
      </ScrollArea>

      {full && (
        <p className="text-2xs text-text-dim">{tr("ui-agent-picker-limit-remove-one-pick-someone-else", { max })}</p>
      )}
      {note && <p className="text-2xs text-text-dim">{note}</p>}
    </div>
  );
}
