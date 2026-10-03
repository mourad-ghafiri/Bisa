/**
 * One control for "who can be given work": agents, people and teams.
 *
 * `Assignee` is one word for three kinds of principal, and every surface that
 * asks the question — a new goal, a team's roster, a step's assignee —
 * used to ask a narrower one (a single team, an agent id, nothing at all).
 * This is the whole set, searchable, and it emits the compact wire form the
 * routes accept: `agent:<id>` / `human:<hex>` / `team:<id>`.
 *
 * **A team may not contain another team.** The backend rejects it at
 * validation, so a roster picker passes `kinds={["agent", "human"]}` and the
 * option never appears — a rule you cannot reach is better than an error you
 * have to read.
 */

import { matchesWords, searchNeedle } from "../../ui/tagSearchModel.mjs";
import { useMemo, useState, type ReactNode } from "react";
import { api } from "../../api";
import { useWorkspace } from "../../shell/useWorkspaceData";
import type { TeamDef } from "../../types";
import { planSummary } from "../../types";
import {
  AgentRow,
  Avatar,
  ErrorNote,
  ICON,
  ScrollArea,
  SkeletonRows,
  TextInput,
  Tooltip,
  type AgentCandidate,
} from "../../ui";
import { idOf, kindOf } from "./assigneeWire.mjs";
import type { AssigneeKind } from "./assigneeWire.mjs";
import { useAsync } from "./useAsync";
import { t as tr } from "../../i18n/l10n.mjs";
import { teamOptionLine, teamTakesWork } from "../rosterModel.mjs";

/**
 * The wire grammar lives in `assigneeWire.mjs` and is re-exported here so the
 * dozen callers that already import it from the picker keep working. It moved
 * out because `types.ts` had a second copy of the same two conversions, and
 * because a `.tsx` file cannot be imported by `node --test`.
 */
export { assigneeToWire, kindOf, wireToAssignee } from "./assigneeWire.mjs";
export type { AssigneeKind } from "./assigneeWire.mjs";

/** One pickable principal, in the shape a row needs to render it. */
interface AssigneeOption {
  /** Wire form — `agent:<id>` / `human:<hex>` / `team:<id>`. */
  key: string;
  kind: AssigneeKind;
  name: string;
  /** One line of context: harness and model plan, role, roster size. */
  sub: string;
  /** Identicon seed. A stable id, never a name. */
  avatar: string;
  /** The picture, by content hash (ide/14 §Photos) — an agent's, a person's face, a team's. */
  photo?: { sha256: string } | null;
  /** False when the key names something this workspace no longer has. */
  known: boolean;
  /** False for what may be shown and not chosen: a team that was stood down is addressed by nothing. */
  offered: boolean;
}

const KIND_LABEL: Record<AssigneeKind, string> = {
  agent: tr("work-assignee-picker-kind-agent"),
  human: tr("work-assignee-picker-kind-person"),
  team: tr("work-assignee-picker-kind-team"),
};

const GROUP_LABEL: Record<AssigneeKind, string> = {
  agent: tr("work-assignee-picker-agents"),
  human: tr("work-assignee-picker-people"),
  team: tr("work-assignee-picker-teams"),
};

/**
 * Everything assignable in this workspace.
 *
 * Agents and people come from the shell store (already loaded, already live);
 * teams are the one list the shell does not hold, so they are fetched here —
 * skipped entirely when the caller does not offer them.
 */
export function useAssigneeOptions(kinds: AssigneeKind[] = ["agent", "human", "team"], exclude: readonly string[] = []): {
  options: AssigneeOption[];
  /** Always answers: an unknown key describes itself as missing. */
  describe: (key: string) => AssigneeOption;
  teams: TeamDef[];
  loading: boolean;
  error: string | null;
  reload: () => void;
} {
  const ws = useWorkspace();
  const wantTeams = kinds.includes("team");
  const {
    data: teamData,
    error,
    loading,
    reload,
  } = useAsync(
    async (s) => (wantTeams ? await api.teams(s) : { teams: [] as TeamDef[] }),
    [wantTeams],
  );
  const teams = teamData?.teams ?? [];

  const options = useMemo<AssigneeOption[]>(() => {
    const out: AssigneeOption[] = [];
    if (kinds.includes("agent")) {
      for (const a of ws.agents) {
        // An agent this surface may not offer — the Workflow Agent from a
        // workstream — is left out, not greyed.
        if (exclude.includes(a.id)) continue;
        out.push({
          key: `agent:${a.id}`,
          kind: "agent",
          name: a.name,
          sub: `${a.harness} · ${planSummary(a.models)}`,
          avatar: a.pubkey,
          photo: a.photo,
          known: true,
          offered: true,
        });
      }
    }
    if (kinds.includes("human")) {
      for (const m of ws.members) {
        out.push({
          key: `human:${m.pubkey}`,
          kind: "human",
          name: m.label ?? ws.nameOf(m.pubkey),
          sub: m.pubkey === ws.me ? tr("work-assignee-picker-words", { role: m.role }) : m.role,
          avatar: m.pubkey,
          photo: m.photo,
          known: true,
          offered: true,
        });
      }
    }
    if (kinds.includes("team")) {
      for (const t of teams) {
        out.push({
          key: `team:${t.id}`,
          kind: "team",
          name: t.name,
          // Its roster in a line — or that it was stood down, which is why it is not among the choices.
          sub: teamOptionLine(t),
          avatar: t.id,
          photo: t.photo,
          known: true,
          offered: teamTakesWork(t),
        });
      }
    }
    return out;
    // `teams` is derived from `teamData`; depending on it directly would
    // rebuild the list on every render. `ws` is read by its fields above.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ws.agents, ws.members, ws.nameOf, ws.me, kinds, exclude, teamData]);

  const index = useMemo(() => new Map(options.map((o) => [o.key, o])), [options]);

  const describe = (key: string): AssigneeOption => {
    const found = index.get(key);
    if (found) return found;
    // Not a wire key at all: shown, never guessed into a kind.
    const kind = kindOf(key) ?? "agent";
    const id = idOf(key);
    return {
      key,
      kind,
      name: kindOf(key) ? `${KIND_LABEL[kind]} ${id.slice(-6)}` : tr("work-assignee-picker-not-valid-key", { key }),
      sub: tr("work-assignee-picker-not-workspace"),
      avatar: id,
      known: false,
      offered: false,
    };
  };

  return { options, describe, teams, loading, error, reload };
}

/** A principal as a chip: identicon, name, kind. Read-only. */
export function AssigneeTag({ option }: { option: AssigneeOption }) {
  return (
    // The name is clipped at 9rem and the one line of context never shows at
    // all, so the tooltip is where a reader finds out which of two agents
    // called "Reviewer" this is. Filled like the kit's neutral chip; one the
    // workspace no longer has is the empty outline, the kit's `quiet`.
    <Tooltip label={`${option.name} — ${option.sub}`}>
      <span
        className={`inline-flex max-w-full items-center gap-1.5 rounded-full border px-1.5 py-0.5 text-2xs ${
          option.known ? "border-transparent bg-surface-2" : "border-border text-text-dim"
        }`}
      >
        <Avatar id={option.avatar} name={option.name} photo={option.photo} size={14} />
        <span className="max-w-[9rem] truncate">{option.name}</span>
      </span>
    </Tooltip>
  );
}

/** A list of wire keys, rendered as tags. The read-only half of the picker. */
export function AssigneeTags({
  value,
  kinds,
  empty = tr("work-assignee-picker-nobody"),
}: {
  value: string[];
  kinds?: AssigneeKind[];
  empty?: string;
}) {
  const { describe } = useAssigneeOptions(kinds);
  if (value.length === 0) return <span className="text-2xs text-text-dim">{empty}</span>;
  return (
    <span className="flex flex-wrap items-center gap-1">
      {value.map((k) => (
        <AssigneeTag key={k} option={describe(k)} />
      ))}
    </span>
  );
}

/**
 * *Assign* and *address* are different questions, so this picker stays — but
 * they must not be asked in two different visual languages. The row is the
 * kit's {@link AgentRow}, the same one the `@`-picker, the address tray, the
 * roster picker and the new-DM dialog draw.
 *
 * `AssigneeKind` and the row's `kind` are the same three words, so a team
 * still reads "team" here rather than borrowing "person" — a label that is
 * wrong is worse than none.
 */
function asCandidate(option: AssigneeOption): AgentCandidate {
  return {
    id: option.key,
    name: option.name,
    kind: option.kind,
    description: option.sub,
    avatar: option.avatar,
    photo: option.photo,
    warning: option.known ? null : tr("work-assignee-picker-not-workspace"),
  };
}

function Row({
  option,
  chosen,
  disabled,
  onToggle,
}: {
  option: AssigneeOption;
  chosen: boolean;
  disabled?: boolean;
  onToggle: () => void;
}) {
  return (
    <li>
      {/* `blocked` rather than a disabled button: the row still has to be
          readable and announced — the cap is a fact about the selection, not
          about this principal. */}
      <AgentRow
        standalone
        candidate={{ ...asCandidate(option), blocked: disabled }}
        selected={chosen}
        onSelect={onToggle}
      />
    </li>
  );
}

/**
 * Pick one or many principals.
 *
 * `value` and `onChange` speak wire keys throughout, so a caller never has to
 * remember which of the three shapes a route wants. `max={1}` makes it a
 * single-choice control that swaps rather than accumulates — what a step's
 * one assignee needs.
 */
export function AssigneePicker({
  value,
  onChange,
  kinds = ["agent", "human", "team"],
  exclude,
  disabled,
  max,
  placeholder = tr("work-assignee-picker-search-agents-people-teams"),
  note,
}: {
  value: string[];
  onChange: (next: string[]) => void;
  kinds?: AssigneeKind[];
  /** Agent ids this surface may not offer — a rule of the surface, not of the agent. */
  exclude?: readonly string[];
  disabled?: boolean;
  /** Cap on selections; `1` swaps the choice instead of adding to it. */
  max?: number;
  placeholder?: string;
  /** An extra line under the list — a rule this particular surface holds. */
  note?: ReactNode;
}) {
  const { options, describe, loading, error, reload } = useAssigneeOptions(kinds, exclude);
  const [query, setQuery] = useState("");

  const chosen = useMemo(() => new Set(value), [value]);
  const full = max !== undefined && value.length >= max;

  const shown = useMemo(() => {
    const q = searchNeedle(query);
    const match = (o: AssigneeOption) => matchesWords(q, [o.name, o.sub]);
    const rest = options.filter((o) => o.offered && !chosen.has(o.key) && match(o));
    return kinds
      .map((kind) => ({ kind, rows: rest.filter((o) => o.kind === kind) }))
      .filter((g) => g.rows.length > 0);
  }, [options, chosen, query, kinds]);

  const toggle = (key: string) => {
    if (chosen.has(key)) {
      onChange(value.filter((k) => k !== key));
      return;
    }
    if (max === 1) {
      onChange([key]);
      return;
    }
    if (full) return;
    onChange([...value, key]);
  };

  return (
    <div className="flex flex-col gap-2">
      <div className="flex min-h-7 flex-wrap items-center gap-1">
        {value.length === 0 ? (
          <span className="text-2xs text-text-dim">{tr("work-assignee-picker-nobody-yet")}</span>
        ) : (
          value.map((key) => {
            const o = describe(key);
            return (
              <Tooltip key={key} label={`${o.name} — ${o.sub}`}>
                <span
                  className={`inline-flex items-center gap-1.5 rounded-full border py-0.5 pr-1 pl-1.5 text-2xs ${
                    o.known ? "border-transparent bg-surface-2" : "border-border text-text-dim"
                  }`}
                >
                  <Avatar id={o.avatar} name={o.name} photo={o.photo} size={14} />
                  <span className="max-w-[9rem] truncate">{o.name}</span>
                  <button
                    type="button"
                    disabled={disabled}
                    onClick={() => onChange(value.filter((k) => k !== key))}
                    aria-label={tr("work-assignee-picker-remove", { o: o.name })}
                    className="anim rounded-full px-1 text-text-dim hover:bg-surface hover:text-danger disabled:opacity-45"
                  >
                    <ICON.close size={10} aria-hidden />
                  </button>
                </span>
              </Tooltip>
            );
          })
        )}
      </div>

      <TextInput
        value={query}
        disabled={disabled}
        placeholder={placeholder}
        aria-label={tr("work-assignee-picker-search-label")}
        onChange={(e) => setQuery(e.target.value)}
      />

      {error && <ErrorNote error={tr("work-assignee-picker-couldn-t-read-teams", { error })} retry={reload} />}

      {/* A ScrollArea rather than raw overflow: this list is often inside a
          dialog over a dark surface, where the OS scrollbar is drawn from the
          system palette and not the theme's. */}
      <ScrollArea className="max-h-56" viewportClassName="pr-0.5">
        {loading && options.length === 0 ? (
          <SkeletonRows rows={4} />
        ) : shown.length === 0 ? (
          <p className="py-3 text-center text-2xs text-text-dim">
            {query
              ? tr("work-assignee-picker-nothing-matches")
              : value.length > 0
                ? tr("work-assignee-picker-everyone-already-picked")
                : tr("work-assignee-picker-no-agents-people-teams-pick-yet")}
          </p>
        ) : (
          shown.map((g) => (
            <div key={g.kind} className="mb-2 last:mb-0">
              <p className="mb-1 text-2xs font-semibold text-text-dim">{GROUP_LABEL[g.kind]}</p>
              <ul className="flex flex-col gap-1">
                {g.rows.map((o) => (
                  <Row
                    key={o.key}
                    option={o}
                    chosen={false}
                    disabled={disabled || (full && max !== 1)}
                    onToggle={() => toggle(o.key)}
                  />
                ))}
              </ul>
            </div>
          ))
        )}
      </ScrollArea>

      {!kinds.includes("team") && (
        <p className="text-2xs text-text-dim">{tr("work-assignee-picker-teams-not-offered-here-team-may")}</p>
      )}
      {full && max !== 1 && (
        <p className="text-2xs text-text-dim">{tr("work-assignee-picker-limit-remove-one-pick-someone-else", { max })}</p>
      )}
      {note && <p className="text-2xs text-text-dim">{note}</p>}
    </div>
  );
}
