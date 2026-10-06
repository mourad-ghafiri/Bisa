/**
 * An agent's skills and MCP servers are **references**, and this is what a
 * reference looks like on screen.
 *
 * Both used to be embedded content — a private markdown copy per agent, a raw
 * JSON transport blob per agent — so twenty agents doing code review grew
 * twenty divergent checklists, and every agent snapshot carried one machine's
 * command lines off the box. They are now ids into a shared skill library and
 * a local MCP registry, resolved at launch.
 *
 * The consequence this file exists to show: **an id can resolve to nothing.**
 * A dangling reference should now be rare — the store refuses to delete a
 * skill or an MCP server while any agent still names it, which is what the
 * `DeleteDialog` at the bottom of this file surfaces. But a hand-edited truth
 * file can still produce one, and at launch it costs that skill silently,
 * with only a line in the log. So an unresolved id renders as the raw id
 * marked unresolved rather than being filtered out of the list. A reference
 * you cannot see is one you will never fix.
 *
 * The file carries the **other direction** too, at the bottom: what points
 * back at an object, and the delete dialog built over it. Both directions are
 * the same relation read from opposite ends, and keeping them apart is how
 * two screens end up disagreeing about what "still in use" means.
 */

import { healthWords } from "../_settings/mcpHealthModel.mjs";
import { useMemo, useState, type ReactNode } from "react";
import { api, type UsageKind } from "../../api";
import { useConversationEvents } from "../../bus";
import { skillMoved } from "../rosterModel.mjs";
import { href } from "../../router";
import type { McpServerView, Origin, Reference, ReferenceKind, SkillDef } from "../../types";
import {
  Button,
  Chip,
  Dialog,
  ErrorNote,
  ICON,
  SOLID_DANGER,
  SkeletonRows,
  TagChips,
  TextInput,
  failureText,
  useToast,
} from "../../ui";
import { OriginChip } from "./Origin";
import { attempt, useAsync } from "./useAsync";
import { t as tr } from "../../i18n/l10n.mjs";

/**
 * The rule, in the words that decide what belongs where. Rendered wherever
 * somebody is about to write or attach one, because the mistake it prevents —
 * a skill that restates the agent's role — is made at exactly that moment.
 */
export const SKILL_RULE =
  tr("work-library-refs-agent-s-system-prompt-role-skill");

/** Why a short list is not a stylistic preference. */
export const SKILL_BUDGET =
  tr("work-library-refs-every-skill-delivered-into-every-session");

/** Where an MCP server comes from, said once so every empty state agrees. */
export const MCP_REGISTRY_NOTE =
  tr("work-library-refs-nothing-ships-registry-bundled-server-shells");

// ---------------------------------------------------------------------------
// One shape for both libraries
// ---------------------------------------------------------------------------

/**
 * A skill and an MCP entry differ in everything except how they are browsed
 * and referenced, so the picker and the attached-list speak this and neither
 * has to know which library it is over.
 */
interface LibraryEntry {
  id: string;
  name: string;
  description: string;
  tags: string[];
  /**
   * Where it came from. Absent on an MCP entry, and absent on purpose:
   * nothing ships in that registry, so a provenance field there could only
   * ever read "created here" — a badge that says the same thing about every
   * row is a badge that says nothing.
   */
  origin?: Origin;
  /** False when the id resolved to nothing — see the header. */
  known: boolean;
  /** Registry entries can be switched off without losing the entry. */
  disabled?: boolean;
  /** A word beside the name — a failing MCP server's last probe. */
  warning?: string;
}

const skillEntry = (s: SkillDef): LibraryEntry => ({
  id: s.id,
  name: s.name,
  description: s.description,
  tags: s.tags ?? [],
  origin: s.origin,
  known: true,
});

const mcpEntry = (m: McpServerView): LibraryEntry => ({
  id: m.id,
  name: m.name,
  description: m.description ?? "",
  tags: m.tags ?? [],
  known: true,
  disabled: !(m.enabled ?? true),
  // A server whose last probe failed is offered, marked: the agent editor
  // says what Settings knows, and one model says how (`mcpHealthModel`).
  warning: m.health?.state === "failing" ? healthWords(m.health) : undefined,
});

/**
 * Resolve an agent's id list against a library, in the agent's own order —
 * a harness reads skills in the order they are listed, so this must not sort.
 */
export function resolveRefs(ids: string[], library: LibraryEntry[]): LibraryEntry[] {
  const index = new Map(library.map((e) => [e.id, e]));
  return ids.map(
    (id) =>
      index.get(id) ?? {
        id,
        name: id,
        description:
          tr("work-library-refs-nothing-answers-id-launch-drops-warning"),
        tags: [],
        known: false,
      },
  );
}

// ---------------------------------------------------------------------------
// Loading the two libraries
// ---------------------------------------------------------------------------

export interface Library {
  entries: LibraryEntry[];
  loading: boolean;
  error: string | null;
  reload: () => void;
}

/** The library plus the raw definitions, which the library screen edits. */
type SkillLibrary = Library & { skills: SkillDef[] };

/** The shared skill library. One fetch per screen; ids resolve against it. */
export function useSkillLibrary(): SkillLibrary {
  const { data, error, loading, reload } = useAsync((s) => api.skills(undefined, s), []);
  // A skill written, edited or removed anywhere moves its record: the library is read again, so a picker never offers what is gone.
  useConversationEvents((f) => {
    if (skillMoved(f)) reload();
  });
  const entries = useMemo(() => (data?.skills ?? []).map(skillEntry), [data]);
  return { entries, skills: data?.skills ?? [], loading, error, reload };
}

/** The local MCP registry. Empty on a fresh workspace, deliberately. */
export function useMcpRegistry(): Library {
  const { data, error, loading, reload } = useAsync((s) => api.mcps(undefined, s), []);
  const entries = useMemo(() => (data?.mcp ?? []).map(mcpEntry), [data]);
  return { entries, loading, error, reload };
}

// ---------------------------------------------------------------------------
// Showing what an agent carries
// ---------------------------------------------------------------------------

/**
 * The attached list, with detach beside each row.
 *
 * `onDetach` is optional: the same list renders read-only wherever the caller
 * has no id to write back to.
 */
export function AttachedRefs({
  entries,
  onDetach,
  busy,
}: {
  entries: LibraryEntry[];
  onDetach?: (id: string) => void;
  busy?: boolean;
}) {
  return (
    <ul className="flex flex-col gap-1">
      {entries.map((e) => (
        <li
          key={e.id}
          // No edge of its own — the list sits in a fieldset or a section that
          // has one; an unresolved id is washed in danger beside its chip.
          className={`flex items-start gap-2 rounded-control px-2 py-1.5 ${
            e.known ? "bg-surface-2/50" : "bg-danger-soft/50"
          }`}
        >
          <span className="min-w-0 flex-1">
            <span className="flex flex-wrap items-center gap-1.5">
              <span className={`min-w-0 truncate text-2xs font-medium ${e.known ? "" : "font-mono"}`}>
                {e.name}
              </span>
              {e.origin && <OriginChip origin={e.origin} id={e.id} />}
              {e.disabled && <Chip tone="warn">{tr("work-library-refs-disabled")}</Chip>}
              {e.warning && <Chip tone="warn">{e.warning}</Chip>}
              {!e.known && <Chip tone="danger">{tr("work-library-refs-unresolved")}</Chip>}
            </span>
            <span className="mt-0.5 block text-2xs text-text-dim">{e.description}</span>
            {(e.tags ?? []).length > 0 && (
              <span className="mt-1 flex flex-wrap gap-1">
                <TagChips tags={e.tags} max={4} />
              </span>
            )}
          </span>
          {onDetach && (
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => onDetach(e.id)}>{tr("work-library-refs-detach")}</Button>
          )}
        </li>
      ))}
    </ul>
  );
}

/**
 * Attach one more, from what is not attached yet.
 *
 * A `<select>` rather than another modal: attaching is one choice, it has to
 * stay reachable from the keyboard, and the detail pane behind it must not
 * lose its scroll position to a dialog. The control resets to its placeholder
 * after each pick, so it reads as an action and not as current state.
 */
export function AttachSelect({
  library,
  attached,
  onAttach,
  label,
  busy,
}: {
  library: LibraryEntry[];
  attached: string[];
  onAttach: (id: string) => void;
  label: string;
  busy?: boolean;
}) {
  const free = library.filter((e) => !attached.includes(e.id));
  if (free.length === 0) return null;
  return (
    <select
      value=""
      disabled={busy}
      aria-label={label}
      onChange={(e) => {
        if (e.target.value) onAttach(e.target.value);
      }}
      className="anim h-7 rounded-control border border-border bg-surface px-2 text-2xs text-text focus:border-accent focus:outline-none disabled:opacity-45"
    >
      <option value="">{label}</option>
      {free.map((e) => (
        <option key={e.id} value={e.id}>
          {e.name}
        </option>
      ))}
    </select>
  );
}

// ---------------------------------------------------------------------------
// Picking references while editing
// ---------------------------------------------------------------------------

/**
 * A multi-select over one library, emitting ids.
 *
 * Chosen entries stay in the order they were picked, because that is the
 * order the harness reads them in — sorting the list here would silently
 * reorder what a launch delivers.
 */
export function RefPicker({
  library,
  value,
  onChange,
  placeholder,
  disabled,
}: {
  library: LibraryEntry[];
  value: string[];
  onChange: (next: string[]) => void;
  placeholder: string;
  disabled?: boolean;
}) {
  const [query, setQuery] = useState("");
  const chosen = useMemo(() => new Set(value), [value]);

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return library;
    return library.filter(
      (e) =>
        e.name.toLowerCase().includes(q) ||
        e.id.includes(q) ||
        e.description.toLowerCase().includes(q) ||
        (e.tags ?? []).some((t) => t.includes(q)),
    );
  }, [library, query]);

  const toggle = (id: string) =>
    onChange(chosen.has(id) ? value.filter((v) => v !== id) : [...value, id]);

  return (
    <div className="flex flex-col gap-2">
      <TextInput
        value={query}
        disabled={disabled}
        placeholder={placeholder}
        onChange={(e) => setQuery(e.target.value)}
      />
      <div className="max-h-56 overflow-y-auto pr-0.5">
        {shown.length === 0 ? (
          <p className="py-3 text-center text-2xs text-text-dim">
            {query ? tr("work-assignee-picker-nothing-matches") : tr("work-library-refs-library-empty")}
          </p>
        ) : (
          <ul className="flex flex-col gap-1">
            {shown.map((e) => {
              const on = chosen.has(e.id);
              return (
                <li key={e.id}>
                  <button
                    type="button"
                    disabled={disabled}
                    aria-pressed={on}
                    onClick={() => toggle(e.id)}
                    className={`anim flex w-full items-start gap-2 rounded-control border px-2 py-1.5 text-left disabled:opacity-45 ${
                      on ? "border-text/35 bg-selected text-text" : "border-transparent hover:bg-surface-2"
                    }`}
                  >
                    <span
                      aria-hidden
                      className={`mt-0.5 flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded border ${
                        on ? "border-text bg-text text-surface" : "border-border text-transparent"
                      }`}
                    >
                      <ICON.check size={10} strokeWidth={3} />
                    </span>
                    <span className="min-w-0 flex-1">
                      <span className="flex flex-wrap items-center gap-1.5">
                        <span className="min-w-0 truncate text-2xs font-medium">{e.name}</span>
                        {e.origin && <OriginChip origin={e.origin} id={e.id} />}
                        {e.disabled && <Chip tone="warn">{tr("work-library-refs-disabled")}</Chip>}
              {e.warning && <Chip tone="warn">{e.warning}</Chip>}
                      </span>
                      <span className="mt-0.5 block truncate text-2xs text-text-dim">
                        {e.description}
                      </span>
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </div>
  );
}


// ---------------------------------------------------------------------------
// The other direction: what points back
// ---------------------------------------------------------------------------

/**
 * What to call a holder in a sentence.
 *
 * Deliberately not the wire tag: `governance` is the variant, but the thing
 * the reader is being sent to is a gate policy. The store draws the same
 * distinction for the same reason, and these two records mirror its `noun()`
 * and `remedy()` word for word — a refusal the node writes and a sentence
 * this screen writes must not be two different accounts of one relation. Both
 * are exhaustive over `ReferenceKind`, so a new variant lands here as a
 * type error rather than as a blank label.
 */
const REFERENCE_NOUN: Record<ReferenceKind, string> = {
  agent: "agent",
  team: "team",
  channel: "channel",
  goal: "goal",
  project: "project",
  work_item: tr("work-library-refs-work-item"),
  governance: tr("work-library-refs-gate-policy"),
  workflow: "workflow",
  account: "account",
};

/** The one thing to do next, per kind of holder. */
const REFERENCE_REMEDY: Record<ReferenceKind, string> = {
  agent: tr("work-library-refs-detach-from-agents-carry-first"),
  team: tr("work-library-refs-remove-from-team-first"),
  channel: tr("work-library-refs-take-off-channel-s-roster-first"),
  goal: tr("work-library-refs-unassign-from-goal-first"),
  project: tr("work-library-refs-unassign-from-project-first"),
  work_item: tr("work-library-refs-let-work-item-settle-cancel-first"),
  governance: tr("work-library-refs-take-out-gate-policy-first"),
  workflow: tr("work-library-refs-change-workflow-step-names-first"),
  account: tr("work-library-refs-forget-account-first"),
};

/**
 * Where to send someone to detach a holder, when the desktop has a screen for
 * it.
 *
 * One kind answers `null`, and it is not an oversight: a work item is
 * addressed as goal-plus-item and a bare item id names no route. It renders
 * as an unlinked row that still says what it is, because a row that looks
 * clickable and is not costs the reader a click and their place in the list.
 */
function holderHref(r: Reference): string | null {
  switch (r.kind) {
    case "workflow":
      return href({ name: "workflow", id: r.id });
    case "agent":
      return href({ name: "agent", id: r.id });
    case "team":
      // Teams are an index screen; the selection travels in the hash, which
      // is the same jump the command palette makes.
      return href({ name: "teams" }, { team: r.id });
    case "channel":
      return href({ name: "channel", id: r.id });
    case "goal":
      return href({ name: "goal", id: r.id });
    case "project":
      return href({ name: "workbench", scope: "workstream", id: r.id });
    case "governance":
      return href({ name: "settings" }, { tab: "governance" });
    case "account":
      return href({ name: "settings" }, { tab: "connectors" });
    case "work_item":
      return null;
  }
}

/** What still points at one object, as a screen reads it. */
export interface Usage {
  /** Null while the check is in flight or has failed — never a stale answer. */
  refs: Reference[] | null;
  error: string | null;
  checking: boolean;
  reload: () => void;
}

/**
 * Ask what still points at an object. An empty list means the matching
 * `DELETE` would go through.
 *
 * Both the answer and the failure are tagged with the id they belong to, and
 * both are dropped the moment the selection moves. `useAsync` keeps its last
 * result and its last error on screen while the next request runs — right for
 * a list, wrong here: showing agent A's holders under agent B's name sends
 * somebody to detach the wrong thing, and A's failure under B's name offers a
 * delete nobody checked. That is also why the request's own failure is caught
 * and returned rather than thrown, since a thrown one lands in state that
 * carries no id.
 */
export function useUsage(kind: UsageKind, id: string | null): Usage {
  const { data, reload } = useAsync(
    async (s) => {
      if (id === null) return null;
      try {
        return { id, refs: (await api.usage(kind, id, s)).usage, error: null as string | null };
      } catch (e) {
        // An abort is not a failure to report — it is this hook being asked a
        // different question — so it stays thrown for `useAsync` to swallow.
        if (s.aborted) throw e;
        return {
          id,
          refs: null as Reference[] | null,
          error: failureText("work", "library-refs-failed", e),
        };
      }
    },
    [kind, id],
  );
  const answered = data !== null && data.id === id;
  return {
    refs: answered ? data.refs : null,
    error: answered ? data.error : null,
    // `useAsync` raises `loading` on its first load only, so on every
    // selection after the first, "asked about an id, nothing back for that id"
    // is what actually says the check is in flight.
    checking: id !== null && !answered,
    reload,
  };
}

/**
 * Everything still pointing at an object, one row each.
 *
 * The label is the holder's own — a team's name, a goal's title — because
 * a list of ULIDs tells the reader a delete was refused and leaves them to
 * search the workspace for why. Each row links to where the reference is
 * held, with the remedy beside it, so "what is using this" and "where do I go
 * to stop it" are one answer rather than two.
 */
function UsageRows({ usage }: { usage: Reference[] }) {
  return (
    <ul className="flex flex-col gap-1">
      {usage.map((r, i) => {
        const to = holderHref(r);
        const body = (
          <>
            <Chip tone="quiet">{REFERENCE_NOUN[r.kind]}</Chip>
            <span className="min-w-0 flex-1">
              <span className="block truncate text-2xs font-medium text-text">{r.label}</span>
              <span className="block text-2xs text-text-dim">{REFERENCE_REMEDY[r.kind]}</span>
            </span>
          </>
        );
        return (
          <li key={`${r.kind}:${r.id}:${i}`}>
            {to === null ? (
              <div className="flex items-center gap-2 rounded-control bg-surface-2/50 px-2 py-1.5">
                {body}
              </div>
            ) : (
              <a
                href={to}
                className="anim flex items-center gap-2 rounded-control bg-surface-2/50 px-2 py-1.5 hover:bg-surface-2"
              >
                {body}
              </a>
            )}
          </li>
        );
      })}
    </ul>
  );
}

/**
 * The one control that opens {@link DeleteDialog}, in the three states the
 * usage check can be in.
 *
 * There is no red Delete while something still points at the object: that
 * button's only possible outcome is the node's 400, and a control that exists
 * only to refuse you is worse than no control. What stands in its place says
 * how many holders there are and opens the same dialog to list them, which is
 * the work that actually has to happen next.
 */
export function DeleteButton({ usage, onOpen }: { usage: Usage; onOpen: () => void }) {
  if (usage.checking) {
    return (
      <Button size="sm" variant="ghost" disabled>{tr("work-library-refs-checking-usage")}</Button>
    );
  }
  const blocking = usage.refs?.length ?? 0;
  if (blocking > 0) {
    return (
      <Button size="sm" onClick={onOpen}>
        {tr("work-library-refs-in-use-by-things", { n: blocking })}
      </Button>
    );
  }
  return (
    <Button size="sm" variant="danger" onClick={onOpen}>{tr("work-library-refs-delete")}</Button>
  );
}

/**
 * Delete, having already asked what would refuse it.
 *
 * The node removes nothing while something still points at it, and the 400 it
 * answers with names the first couple of holders, the total and the remedy.
 * So this dialog has three bodies rather than one: the holders when there are
 * any, the consequence when there are none, and the check's own failure when
 * it could not be read. In that last case Delete is still offered — the node
 * refuses on its own and its message is surfaced unchanged, which is a better
 * outcome than a dialog that can only be closed.
 *
 * The check is the caller's, not this dialog's, so the detail pane behind it
 * can label its own button from the same answer instead of running a second
 * request for it.
 */
export function DeleteDialog({
  open,
  title,
  consequence,
  note,
  usage,
  remove,
  onClose,
  onDeleted,
}: {
  open: boolean;
  /** The question, e.g. `Delete Engineering?`. */
  title: string;
  /** What removing it costs, in the caller's own words. Shown when clear. */
  consequence: ReactNode;
  /** Anything the caller wants said about *why* a holder counts. */
  note?: ReactNode;
  usage: Usage;
  /** The caller's own `api.delete…` call, so the client stays typed. */
  remove: () => Promise<unknown>;
  onClose: () => void;
  onDeleted: () => void;
}) {
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  // Read once into a local so the blocked branch narrows without an assertion.
  const refs = usage.refs;
  const blocked = refs !== null && refs.length > 0;

  const run = async () => {
    setBusy(true);
    // The refusal is the store's own sentence, naming the holders and the
    // remedy. It is passed through unchanged: rewording it here would drop
    // the names, which are the only part worth reading.
    const ok = await attempt(remove, toast.error);
    setBusy(false);
    if (ok) {
      onDeleted();
      onClose();
    }
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={title}
      footer={
        <>
          <Button variant="ghost" disabled={busy} onClick={onClose}>
            {blocked ? tr("work-after-merge-dialog-close") : tr("work-agent-editor-cancel")}
          </Button>
          {!blocked && !usage.checking && (
            // A destructive confirm fills solid danger, as the kit's `ConfirmDialog` does (The Irreversible Asks Rule).
            <Button variant="primary" className={SOLID_DANGER} disabled={busy} onClick={() => void run()}>
              {busy ? tr("work-delete-branch-dialog-deleting") : tr("work-library-refs-delete")}
            </Button>
          )}
        </>
      }
    >
      <div className="flex flex-col gap-3 text-2xs text-text-dim">
        {usage.checking ? (
          <>
            <p>{tr("work-library-refs-checking-what-still-points")}</p>
            <SkeletonRows rows={2} />
          </>
        ) : usage.error !== null ? (
          <>
            <ErrorNote error={usage.error} retry={usage.reload} />
            <p>{tr("work-library-refs-check-could-not-read-so-offered")}</p>
            <div>{consequence}</div>
          </>
        ) : blocked ? (
          <>
            <p>
              {refs.length === 1
                ? tr("work-library-refs-one-thing-still-points-so-cannot")
                : tr("work-library-refs-things-still-point-so-cannot-removed", { refs: refs.length })}{" "}
              {tr("work-library-refs-detach-where-held-go-through")}
            </p>
            <UsageRows usage={refs} />
            {note && <div>{note}</div>}
          </>
        ) : (
          <>
            <p>{tr("work-library-refs-nothing-points-so-will-go-through")}</p>
            <div>{consequence}</div>
          </>
        )}
      </div>
    </Dialog>
  );
}
