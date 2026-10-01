/**
 * Teams: who is on this.
 *
 * A team is a named set of members, each one an agent that does the work or a
 * person who approves it. Assigning `team:<id>` to a goal expands it in
 * place at resolution time, so a team is not a folder — it is a routing
 * decision and a set of approvers, made once and reused.
 *
 * It was a tab inside Agents until this wave, which was a category error: an
 * agent answers "who *can* do this work", a team answers "who is *on* this",
 * and a reader looking for the second had to know it was hiding inside the
 * first. It is a destination now, with the same shape as Agents — roster with
 * search and tags on the left, detail on the right — because two screens that
 * ask sibling questions should not need to be learned twice.
 *
 * Two things about membership are worth knowing before reading the roster:
 *
 * - **The core agent is a member of every team and is stored in none of them.**
 *   `GET /teams` and `GET /teams/{id}` add it at read time and every write
 *   strips it, so it is marked implicit here and carries no Remove control:
 *   there is nothing to remove it from. It is in the room for addressing, not
 *   for work — the pool an unassigned item routes across is the stored members
 *   only, because that agent guides and delegates rather than implementing.
 * - **A team cannot contain another team.** The picker never offers one, so
 *   nesting is not offered and then refused. A team that already holds one —
 *   only reachable by hand-editing a truth file — is still shown and still
 *   sent, so the save fails with the node's own message naming the offending
 *   member instead of the roster quietly losing it.
 *
 * Nothing ships into a workspace: a fresh one has no teams at all, and the
 * nine bundled ones are catalog entries until somebody installs one. So the
 * empty state is a door to the Catalog rather than an apology.
 *
 * The search, the tags picked and where the page was scrolled are the
 * screen's memory (`shell/viewMemoryStore`), and the list and the team open
 * beside it are drawn from what the window last read while the node is read
 * again behind them — so the screen comes back as it was left. The dialog
 * starts as it always did: from the team it edits, or empty.
 */

import { filterByTagsAndWords } from "../ui/tagSearchModel.mjs";
import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import { useConversationEvents } from "../bus";
import { href, navigate, useSearchValue } from "../router";
import { NEW_GOAL, fire } from "../shell/shortcuts";
import { useViewScroll } from "../shell/useViewScroll";
import { useWorkspace } from "../shell/useWorkspaceData";
import { placeOf, useViewState } from "../shell/viewMemoryStore";
import { textValue } from "../shell/viewValuesModel.mjs";
import { settingsSearch } from "./_settings/settingsLink.mjs";
import type { AgentDef, Assignee, AttachmentRef, TeamDef } from "../types";
import { CurrentStepPill } from "./_goals/CurrentStepPill";
import {
  AnimatedList,
  Avatar,
  Button,
  Card,
  Chip,
  Dialog,
  EmptyState,
  ErrorNote,
  Field,
  ICON,
  Labelled,
  Menu,
  NO_TAG_FILTER,
  PhotoField,
  Section,
  SkeletonRows,
  TAG_VOCABULARY,
  TagChips,
  TagFilterBar,
  TagInput,
  TextArea,
  TextInput,
  Tooltip,
  parseTagFilter,
  useToast,
  type TagFilterState,
} from "../ui";
import { AssigneePicker, useAssigneeOptions } from "./_work/AssigneePicker";
import { requireWire } from "./_work/assigneeWire.mjs";
import { DeleteButton, DeleteDialog, useUsage } from "./_work/LibraryRefs";
import { setPendingTeam } from "./_work/NewGoalDialog";
import { OriginChip } from "./_work/Origin";
import { TEAM_EFFECT, teamApi } from "./_work/teamRelation";
import { isImplicitMember, memberFace, rosterLine, storedMembers, teamMoved, withoutMember } from "./rosterModel.mjs";
import { AbsentRecord } from "./_work/AbsentRecord";
import { assigneeWire } from "./_work/types";
import { readKey } from "./_work/keptReadsModel.mjs";
import { attempt, useAsync } from "./_work/useAsync";
import { t as tr } from "../i18n/l10n.mjs";
import { rich } from "../i18n/rich";

/** Where the screen keeps its memory. */
const PLACE = placeOf({ name: "teams" });

/**
 * The two kinds a roster may hold, as a module constant rather than a literal
 * at the call site: {@link useAssigneeOptions} memoises on this array, and a
 * fresh one each render rebuilds the option list on every render.
 */
const ROSTER_KINDS = ["agent", "human"] as const;

/** Stable accessor — {@link TagFilterBar} keys its facets on the list, not on this. */
const tagsOfTeam = (t: TeamDef): string[] => t.tags ?? [];

// ---------------------------------------------------------------------------
// Create and edit
// ---------------------------------------------------------------------------

/**
 * Create or edit a team in one pass: name, brief, and the roster.
 *
 * Building one used to take seven modals — create with an empty roster, then a
 * separate dialog per member from a bare `<select>`. It is one dialog because
 * picking who is on a team *is* the act, and the roster control is the same
 * {@link AssigneePicker} the goal screens and the designer's steps use.
 *
 * `team === null` creates; otherwise it edits, pre-filled.
 */
function TeamDialog({
  open,
  team,
  onClose,
  onSaved,
}: {
  open: boolean;
  team: TeamDef | null;
  onClose: () => void;
  onSaved: (t: TeamDef) => void;
}) {
  const toast = useToast();
  const [name, setName] = useState("");
  const [purpose, setPurpose] = useState("");
  /** The team's picture — an attachment by content hash (ide/14 §Photos). */
  const [photo, setPhoto] = useState<AttachmentRef | null>(null);
  /** Wire keys — `agent:<id>` / `human:<hex>`. Never `team:` from the picker. */
  const [chosen, setChosen] = useState<string[]>([]);
  const [tags, setTags] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);

  // Re-seed whenever the dialog opens, so editing never shows the last team.
  useEffect(() => {
    if (!open) return;
    setName(team?.name ?? "");
    setPurpose(team?.purpose ?? "");
    setPhoto(team?.photo ?? null);
    // The core agent arrives in `members` at read time and is in no stored
    // list, so offering it in the picker would offer a member you can appear
    // to remove and cannot. The store strips it on every write anyway;
    // dropping it here is what keeps the screen honest.
    setChosen((team ? storedMembers(team) : []).map(assigneeWire));
    setTags([...(team?.tags ?? [])]);
  }, [open, team]);

  const nAgents = chosen.filter((k) => k.startsWith("agent:")).length;
  const nHumans = chosen.length - nAgents;

  const save = async () => {
    const trimmed = name.trim();
    if (!trimmed) return;
    // The picker offers agents and people only, so a `team:` key can only have
    // arrived by editing a team that already had one — a hand-edited truth
    // file. It is sent as it stands rather than dropped: the server refuses
    // nesting and names the offending member, which is the message worth
    // reading. `POST /teams` is typed to the two legal arms, and creating
    // starts from an empty roster, so nothing is lost narrowing there.
    setBusy(true);
    let saved: TeamDef | null = null;
    await attempt(async () => {
      const members: Assignee[] = chosen.map(requireWire);
      if (team) {
        // PATCH is partial — send only what this dialog owns.
        const res = await api.patchTeam(team.id, {
          name: trimmed,
          purpose: purpose.trim() || null,
          photo,
          members,
          tags,
        });
        saved = res.team;
      } else {
        const res = await api.createTeam({
          name: trimmed,
          purpose: purpose.trim() || undefined,
          ...(photo ? { photo } : {}),
          members: members.filter(
            (m): m is { agent: string } | { human: string } => !("team" in m),
          ),
          tags,
        });
        saved = res.team;
      }
    }, toast.error);
    setBusy(false);
    if (saved) {
      toast.ok(team ? tr("screens-teams-team-updated") : tr("screens-teams-team-created"));
      onSaved(saved);
      onClose();
    }
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={team ? tr("screens-teams-edit", { team: team.name }) : tr("screens-teams-new-team")}
      description={TEAM_EFFECT}
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={busy}>{tr("screens-channels-cancel")}</Button>
          <Button variant="primary" onClick={() => void save()} disabled={busy || !name.trim()}>
            {busy ? tr("screens-channels-saving") : team ? tr("screens-channels-save") : tr("screens-teams-create-team")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <Field label={tr("screens-channels-name")}>
          <TextInput
            value={name}
            autoFocus
            placeholder={tr("screens-teams-delivery")}
            onChange={(e) => setName(e.target.value)}
          />
        </Field>
        <Field label={tr("screens-teams-purpose")} hint={tr("screens-teams-what-team-operating-brief-optional")}>
          <TextArea
            rows={3}
            value={purpose}
            placeholder={tr("screens-teams-ships-product-surface-builds-reviews-signs")}
            onChange={(e) => setPurpose(e.target.value)}
          />
        </Field>
        <Field label={tr("screens-teams-photo")} hint={tr("screens-teams-picture-team-s-card-rows-png")}>
          <PhotoField id={team?.id ?? "team"} name={name || team?.name} photo={photo} onChange={setPhoto} disabled={busy} />
        </Field>

        <Labelled
          label={tr("screens-teams-members")}
          hint={
            chosen.length === 0
              ? tr("screens-teams-agents-do-work-people-approve-gates")
              : tr("screens-teams-agents-and-people-chosen", { agents: nAgents, people: nHumans })
          }
        >
          <AssigneePicker
            value={chosen}
            onChange={setChosen}
            kinds={[...ROSTER_KINDS]}
            disabled={busy}
            placeholder={tr("screens-teams-search-agents-people")}
          />
        </Labelled>

        <Labelled label={tr("screens-channels-tags")} hint={tr("screens-teams-how-files-tags-how-find-one")}>
          <TagInput value={tags} onChange={setTags} suggestions={[...TAG_VOCABULARY]} />
        </Labelled>
      </div>
    </Dialog>
  );
}

// ---------------------------------------------------------------------------
// The roster
// ---------------------------------------------------------------------------

function TeamCard({
  t,
  agents,
  selected,
  onOpen,
  onEdit,
  onNewGoal,
  onDelete,
}: {
  t: TeamDef;
  /** Already loaded by the shell; the avatars name each member with it. */
  agents: AgentDef[];
  selected: boolean;
  onOpen: () => void;
  onEdit: () => void;
  onNewGoal: () => void;
  onDelete: () => void;
}) {
  const ws = useWorkspace();
  const stored = storedMembers(t);
  const face = (m: Assignee) => memberFace(m, agents, ws);

  return (
    // A div rather than a clickable Card: this one holds its own controls, and
    // nesting a button inside a button hands the outer one every click.
    <Card className={selected ? "border-accent/60" : ""}>
      <div className="flex items-start gap-2">
        <button type="button" onClick={onOpen} className="flex min-w-0 flex-1 items-start gap-2 text-left">
          <Avatar id={t.id} name={t.name} photo={t.photo} size={28} />
          <span className="min-w-0 flex-1">
          <span className="flex items-center gap-1.5">
            <span className="truncate text-xs font-medium">{t.name}</span>
            <OriginChip origin={t.origin} id={t.id} />
            {stored.length === 0 && <Chip tone="warn">{tr("screens-teams-not-staffed")}</Chip>}
          </span>
          <span className="mt-0.5 block text-2xs text-text-dim">{rosterLine(t.members)}</span>
          </span>
        </button>
        <Menu
          label={tr("screens-teams-actions", { t: t.name })}
          trigger={
            <span className="anim flex h-6 w-6 items-center justify-center rounded-control text-text-dim hover:bg-surface-2 hover:text-text">
              <ICON.more size={14} aria-hidden />
            </span>
          }
          items={[
            { label: tr("screens-inbox-open"), icon: ICON.forward, onSelect: onOpen },
            { label: tr("screens-teams-edit-team"), icon: ICON.edit, onSelect: onEdit },
            { label: tr("screens-teams-new-goal-team"), icon: ICON.goal, onSelect: onNewGoal },
            {
              label: tr("screens-agents-delete-2"),
              icon: ICON.delete,
              danger: true,
              separatorBefore: true,
              onSelect: onDelete,
            },
          ]}
        />
      </div>

      {t.purpose && <p className="mt-1.5 line-clamp-2 text-2xs text-text-dim">{t.purpose}</p>}

      {(t.tags ?? []).length > 0 && (
        <div className="mt-1.5 flex flex-wrap gap-1">
          <TagChips tags={t.tags ?? []} max={4} />
        </div>
      )}

      <div className="mt-1.5 flex flex-wrap gap-1.5">
        {t.members.slice(0, 6).map((m, i) => (
          <span
            key={`${assigneeWire(m)}:${i}`}
            className="inline-flex items-center gap-1 rounded-full border border-border px-1.5 py-0.5 text-2xs"
            title={face(m).label}
          >
            <Avatar id={face(m).avatarId} name={face(m).label} photo={face(m).photo} size={14} />
            <span className="max-w-[8rem] truncate">{face(m).label}</span>
          </span>
        ))}
        {t.members.length > 6 && (
          <span className="text-2xs text-text-dim">{tr("screens-teams-more-members", { more: t.members.length - 6 })}</span>
        )}
      </div>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// One team
// ---------------------------------------------------------------------------

/**
 * The detail pane: who is on the team, and what the team is carrying.
 *
 * The goals come from `GET /teams/{id}` rather than from a second filtered
 * list, so "carrying" is the node's answer to the assignment relation rather
 * than this screen's guess at it.
 *
 * Mounted with the team id as its `key`, so selecting a different team
 * remounts rather than showing the previous team's goals under the new
 * team's name while the next request is in flight.
 */
function TeamPane({
  team,
  usage,
  onEdit,
  onNewGoal,
  onDelete,
  onChanged,
}: {
  team: TeamDef;
  usage: ReturnType<typeof useUsage>;
  onEdit: () => void;
  onNewGoal: () => void;
  onDelete: () => void;
  /** A write landed; the roster on the left is now behind. */
  onChanged: () => void;
}) {
  const toast = useToast();
  const { describe } = useAssigneeOptions([...ROSTER_KINDS]);
  const { data, error, loading, reload } = useAsync((s) => teamApi.detail(team.id, s), [team.id], { keep: readKey("team", team.id) });
  const [busy, setBusy] = useState(false);

  // The list's copy is a frame behind a rename; the detail route's is the one
  // that was just written.
  const current = data?.team ?? team;
  const goals = data?.goals ?? [];
  const stored = storedMembers(current);

  /**
   * Take one member off, by writing the roster the team should have.
   *
   * A whole-list PATCH rather than a per-member route, matching every other
   * reference list in the app: one request cannot leave a roster half-changed,
   * and the order survives. The implicit member is never in `stored`, so this
   * cannot be asked to remove it.
   */
  const removeMember = (m: Assignee) => {
    setBusy(true);
    void attempt(
      () =>
        api.patchTeam(current.id, withoutMember(current, m)),
      toast.error,
      () => {
        toast.ok(tr("screens-teams-member-removed"));
        reload();
        onChanged();
      },
    ).finally(() => setBusy(false));
  };

  if (loading && !data) return <SkeletonRows rows={6} />;

  return (
    <div className="flex flex-col gap-4">
      {error && <ErrorNote error={error} retry={reload} />}

      <Card>
        <div className="flex items-center gap-1.5">
          <h2 className="truncate text-sm font-semibold">{current.name}</h2>
          <OriginChip origin={current.origin} id={current.id} />
        </div>
        <p className="mt-0.5 text-2xs text-text-dim">{rosterLine(current.members)}</p>

        {(current.tags ?? []).length > 0 && (
          <div className="mt-2 flex flex-wrap gap-1">
            <TagChips tags={current.tags ?? []} />
          </div>
        )}

        {current.purpose && (
          <p className="mt-2 whitespace-pre-wrap text-2xs text-text-dim">{current.purpose}</p>
        )}

        <p className="mt-2 text-2xs text-text-dim">{TEAM_EFFECT}</p>

        <div className="mt-3 flex flex-wrap items-center gap-2">
          <Button size="sm" variant="primary" onClick={onNewGoal}>{tr("screens-teams-new-goal-team")}</Button>
          <Button size="sm" variant="ghost" onClick={onEdit}>{tr("screens-teams-edit-team")}</Button>
          <DeleteButton usage={usage} onOpen={onDelete} />
        </div>
      </Card>

      <Section title={tr("screens-teams-members-2", { members: current.members.length })}>
        {/* The implicit member means this list is never empty, so the count
            above cannot answer tr("screens-teams-has-anybody-been-put"). This can. */}
        {stored.length === 0 && (
          <EmptyState
            icon={ICON.team}
            title={tr("screens-teams-nobody-has-been-put-team")}
            hint={tr("screens-teams-carries-no-work-until-has-members")}
            action={
              <Button variant="primary" onClick={onEdit}>{tr("screens-teams-add-members")}</Button>
            }
          />
        )}

        <AnimatedList
          className="mt-1.5 flex flex-col gap-1"
          items={current.members}
          keyOf={(m, i) => `${assigneeWire(m)}:${i}`}
          render={(m) => {
            // The same describe() the picker uses, so a member who has since
            // been deleted reads as missing rather than as a raw id.
            const o = describe(assigneeWire(m));
            const implicit = isImplicitMember(m);
            return (
              <div
                className={`flex items-center gap-2 rounded-control border px-2 py-1.5 ${
                  implicit ? "border-dashed border-accent/50" : "border-border"
                }`}
              >
                <Avatar id={o.avatar} name={o.name} photo={o.photo} size={20} />
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-2xs">{o.name}</span>
                  <span className="block truncate text-2xs text-text-dim">{o.sub}</span>
                </span>
                <Chip tone={o.kind === "agent" ? "accent" : o.kind === "team" ? "warn" : "quiet"}>
                  {o.kind === "human" ? tr("screens-teams-person") : o.kind === "agent" ? tr("screens-teams-kind-agent") : tr("screens-teams-kind-team")}
                </Chip>
                {implicit ? (
                  <Tooltip label={tr("screens-teams-member-every-team-stored-none-them")}>
                    <span>
                      <Chip tone="accent">{tr("screens-teams-implicit")}</Chip>
                    </span>
                  </Tooltip>
                ) : (
                  <Button
                    size="sm"
                    variant="ghost"
                    disabled={busy}
                    onClick={() => removeMember(m)}
                  >{tr("screens-teams-remove")}</Button>
                )}
              </div>
            );
          }}
        />

        {current.members.some(isImplicitMember) && (
          <p className="mt-1.5 text-2xs text-text-dim">{tr("screens-teams-platform-own-agent-belongs-every-team")}</p>
        )}
      </Section>

      <Section title={tr("screens-teams-carrying", { goals: goals.length })}>
        {goals.length === 0 ? (
          <EmptyState
            icon={ICON.goal}
            title={tr("screens-teams-not-carrying-anything")}
            hint={tr("screens-teams-assign-team-goal-agents-pick-up")}
            action={
              <Button variant="primary" onClick={onNewGoal}>{tr("screens-teams-new-goal-team")}</Button>
            }
          />
        ) : (
          <ul className="flex flex-col gap-1">
            {goals.map((i) => (
              <li key={i.id}>
                <a
                  href={href({ name: "goal", id: i.id })}
                  className="anim flex items-center gap-2 rounded-control border border-border px-2 py-1.5 hover:bg-surface-2"
                >
                  <span className="min-w-0 flex-1 truncate text-2xs">
                    {i.title ?? tr("screens-teams-goal", { i: i.id.slice(-6) })}
                  </span>
                  <CurrentStepPill row={i} />
                </a>
              </li>
            ))}
          </ul>
        )}
      </Section>
    </div>
  );
}

// ---------------------------------------------------------------------------
// The screen
// ---------------------------------------------------------------------------

export default function Teams() {
  const toast = useToast();
  const { agents } = useWorkspace();
  // The command palette jumps here with `?team=<id>`, so the selection is a
  // place: Back leaves the team rather than the screen, and a reload lands on
  // the same one.
  const [selectedId, select] = useSearchValue("team");
  const { data, error, loading, reload } = useAsync((s) => api.teams(s), [], { keep: readKey("teams") });
  // A team made, edited, stood down or deleted anywhere — this window, the
  // command line, another node — moves its record: the list is read again.
  useConversationEvents((f) => {
    if (teamMoved(f)) reload();
  });

  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<TeamDef | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [query, setQuery] = useViewState(PLACE, "search", "", textValue);
  const [tagFilter, setTagFilter] = useViewState<TagFilterState>(PLACE, "tags", NO_TAG_FILTER, parseTagFilter);

  const teams = useMemo(() => data?.teams ?? [], [data]);
  // The roster and the team open beside it scroll as one page, so its place
  // is kept under the team's name: each team comes back where it was read.
  // It is put back when the screen is drawn — once the list is read — and
  // never when a team is picked: a pick must not move the roster under the
  // hand that made it.
  const root = useRef<HTMLDivElement>(null);
  useViewScroll(root, `${PLACE}#${teams.length > 0 ? "list" : "reading"}`, PLACE);
  const selected = useMemo(
    () => (selectedId ? (teams.find((t) => t.id === selectedId) ?? null) : null),
    [teams, selectedId],
  );

  // Asked at the screen so the detail pane's control and the dialog read the
  // same answer instead of running the question twice.
  const usage = useUsage("team", selected?.id ?? null);

  // The two controls compose: the tag filter narrows the roster, the search
  // narrows within whatever it left. Facets are built from the whole list, so
  // a tag you have selected cannot disappear from under you as you type.
  const shown = useMemo(
    () => filterByTagsAndWords(teams, tagFilter, query, (t) => t.tags, (t) => [t.name, t.id, t.purpose]),
    [teams, query, tagFilter],
  );

  const newGoalFor = (t: TeamDef) => {
    setPendingTeam(t.id);
    fire(NEW_GOAL);
  };

  const dialogs = (
    <>
      <TeamDialog
        open={creating || editing !== null}
        team={editing}
        onClose={() => {
          setCreating(false);
          setEditing(null);
        }}
        onSaved={(t) => {
          reload();
          // A team you just built is the one you want open; an edit should
          // leave you where you already were.
          if (!editing) select(t.id);
        }}
      />

      {/* Mounted only with a team in hand, so `remove` cannot be called
          without an id to delete. */}
      {selected && (
        <DeleteDialog
          open={deleting}
          title={tr("screens-teams-delete", { selected: selected.name })}
          usage={usage}
          consequence={tr("screens-teams-definition-removed-nothing-assigned-no-gate")}
          remove={() => api.deleteTeam(selected.id)}
          onClose={() => setDeleting(false)}
          onDeleted={() => {
            toast.ok(tr("screens-teams-team-deleted"));
            select(null);
            reload();
          }}
        />
      )}
    </>
  );

  if (loading && !data) {
    return (
      <div className="p-6">
        <SkeletonRows rows={6} />
      </div>
    );
  }

  if (error && !data) {
    return (
      <div className="p-6">
        <ErrorNote error={error} retry={reload} />
      </div>
    );
  }

  if (teams.length === 0) {
    return (
      <div className="p-6">
        <EmptyState
          icon={ICON.team}
          title={tr("screens-teams-no-teams-yet")}
          hint={tr("screens-teams-nine-teams-catalog-discovery-engineering-launch", { TEAM_EFFECT })}
          action={
            <div className="flex flex-wrap justify-center gap-2">
              <Button
                variant="primary"
                onClick={() =>
                  navigate({ name: "settings" }, settingsSearch("catalog-team"))
                }
              >{tr("screens-agents-open-catalog")}</Button>
              <Button onClick={() => setCreating(true)}>{tr("screens-teams-build-one-yourself")}</Button>
            </div>
          }
        />
        {dialogs}
      </div>
    );
  }

  return (
    // The root the page's scroll is kept from; it draws no box of its own.
    <div ref={root} className="contents">
    <div data-scroll-keep={`page:${selectedId ?? ""}`} className="min-h-0 flex-1 overflow-y-auto p-6">
      <div className="grid gap-6 lg:grid-cols-[1.3fr_1fr]">
        <div className="flex min-w-0 flex-col gap-2">
          <div className="flex flex-wrap items-center gap-2">
            <TextInput
              value={query}
              placeholder={tr("screens-teams-search-teams")}
              aria-label={tr("screens-teams-search-teams-2")}
              className="h-7 max-w-64 py-0"
              onChange={(e) => setQuery(e.target.value)}
            />
            <span className="text-2xs text-text-dim">{tr("screens-teams-words", { shown: shown.length, teams: teams.length })}</span>
            <Tooltip label={tr("screens-teams-re-read-list")}>
              <button
                type="button"
                onClick={reload}
                aria-label={tr("screens-teams-refresh-teams")}
                className="anim ml-auto flex h-7 w-7 items-center justify-center rounded-control text-text-dim hover:bg-surface-2 hover:text-text"
              >
                <ICON.refresh size={14} aria-hidden />
              </button>
            </Tooltip>
            <Button size="sm" variant="primary" onClick={() => setCreating(true)}>{tr("screens-teams-new-team")}</Button>
          </div>

          <TagFilterBar
            items={teams}
            tagsOf={tagsOfTeam}
            value={tagFilter}
            onChange={setTagFilter}
          />

          {shown.length === 0 ? (
            <EmptyState
              icon={ICON.team}
              title={tr("screens-agents-nothing-matches")}
              hint={tr("screens-teams-no-team-matches-search-tags-have")}
              action={
                <Button
                  onClick={() => {
                    setQuery("");
                    setTagFilter(NO_TAG_FILTER);
                  }}
                >{tr("screens-agents-clear-filters")}</Button>
              }
            />
          ) : (
            <AnimatedList
              className="grid gap-2 md:grid-cols-2"
              items={shown}
              keyOf={(t) => t.id}
              render={(t) => (
                <TeamCard
                  t={t}
                  agents={agents}
                  selected={t.id === selectedId}
                  onOpen={() => select(t.id === selectedId ? null : t.id)}
                  onEdit={() => setEditing(t)}
                  onNewGoal={() => newGoalFor(t)}
                  onDelete={() => {
                    // Selecting first is what puts the usage check on this
                    // team; the dialog opens on its "checking" body and fills
                    // in when the answer lands.
                    select(t.id);
                    setDeleting(true);
                  }}
                />
              )}
            />
          )}
        </div>

        <div className="min-w-0">
          {selected ? (
            <TeamPane
              key={selected.id}
              team={selected}
              usage={usage}
              onEdit={() => setEditing(selected)}
              onNewGoal={() => newGoalFor(selected)}
              onDelete={() => setDeleting(true)}
              onChanged={reload}
            />
          ) : selectedId && data !== null ? (
            // The list is read and does not hold it: the node is asked for that one team, and what it says is what is drawn.
            <AbsentRecord
              key={selectedId}
              id={selectedId}
              what={tr("screens-teams-the-team")}
              read={(tid, s) => teamApi.detail(tid, s)}
              onHere={reload}
              gone={
                <Card>
                  <p className="text-xs font-medium">{tr("screens-teams-team-not-here")}</p>
                  <p className="mt-1 text-2xs text-text-dim">
                    {rich("screens-teams-nothing-answers-to-id", { id: <code className="font-mono">{selectedId}</code> })}
                  </p>
                  <div className="mt-2">
                    <Button size="sm" onClick={() => select(null)}>{tr("screens-teams-back-list")}</Button>
                  </div>
                </Card>
              }
            />
          ) : (
            <Card>
              <p className="text-xs font-medium">{tr("screens-teams-pick-team")}</p>
              <p className="mt-1 text-2xs text-text-dim">{tr("screens-teams-who-what-carrying-how-files-show", { TEAM_EFFECT })}</p>
            </Card>
          )}
        </div>
      </div>

      {dialogs}
    </div>
    </div>
  );
}
