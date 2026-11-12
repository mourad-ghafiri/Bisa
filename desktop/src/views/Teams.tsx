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
 * bundled ones are catalog entries until somebody installs one. So the
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
import { navigate, useSearchValue } from "../router";
import { useViewScroll } from "../shell/useViewScroll";
import { useWorkspace } from "../shell/useWorkspaceData";
import { placeOf, useViewState } from "../shell/viewMemoryStore";
import { textValue } from "../shell/viewValuesModel.mjs";
import { settingsSearch } from "./_settings/settingsLink.mjs";
import type { AgentDef, Assignee, TeamDef } from "../types";
import {
  AnimatedList,
  Avatar,
  Button,
  Card,
  Chip,
  ConfirmDialog,
  Dialog,
  EmptyState,
  ErrorNote,
  Field,
  ICON,
  Labelled,
  MoreMenu,
  NO_TAG_FILTER,
  PhotoField,
  Section,
  SkeletonRows,
  TAG_VOCABULARY,
  TagChips,
  TagFilterBar,
  TagInput,
  TextArea,
  ScreenBar,
  TextInput,
  Tooltip,
  parseTagFilter,
  useToast,
  type TagFilterState,
} from "../ui";
import { AssigneePicker, useAssigneeOptions } from "./_work/AssigneePicker";
import { dirty, mayClose } from "./_work/agentDraftModel.mjs";
import { emptyTeamDraft, maySaveTeam, teamDraftOf, teamSaveOf, type TeamDraft } from "./_work/teamDraftModel.mjs";
import { DeleteButton, DeleteDialog, useUsage } from "./_work/LibraryRefs";
import { OriginChip } from "./_work/Origin";
import { TEAM_EFFECT, teamApi } from "./_work/teamRelation";
import { switchOf, asksDelete, detailStacked, isImplicitMember, memberFace, rosterLine, rosterNotOffered, shownTeam, storedMembers, teamMoved, withoutMember } from "./rosterModel.mjs";
import { useHeldAnswer } from "./_work/useHeldAnswer";
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
  // The dialog's own draft (`teamDraftModel.mjs`): the roster as wire keys —
  // `agent:<id>` / `human:<hex>`, never `team:` from the picker — and never a
  // core agent, which arrives in `members` at read time and is in no stored
  // list: offering it would offer a member you can appear to remove and cannot.
  const [d, setD] = useState<TeamDraft>(emptyTeamDraft);
  const set = (patch: Partial<TeamDraft>) => setD((prev) => ({ ...prev, ...patch }));
  // The draft as the dialog opened: what *edited* is measured against, and what a save names its changes from.
  const seeded = useRef<TeamDraft>(d);
  const [busy, setBusy] = useState(false);
  const [discarding, setDiscarding] = useState(false);
  // The core agents are in every team and stored in none: the picker leaves them out.
  const { agents } = useWorkspace();
  const notOffered = useMemo(() => rosterNotOffered(agents), [agents]);

  // Re-seed whenever the dialog opens, so editing never shows the last team.
  useEffect(() => {
    if (!open) return;
    const fresh = team ? teamDraftOf(team) : emptyTeamDraft();
    seeded.current = fresh;
    setD(fresh);
    setDiscarding(false);
  }, [open, team]);

  const nAgents = d.members.filter((k) => k.startsWith("agent:")).length;
  const nHumans = d.members.length - nAgents;

  // Every way out is one door — Escape, a click outside, the ✕, Cancel: it
  // stays shut while a save is out, and asks before it drops what was typed.
  const requestClose = () => {
    const leave = mayClose({ dirty: dirty(d, seeded.current), busy });
    if (leave === "close") onClose();
    else if (leave === "ask") setDiscarding(true);
  };

  const save = async () => {
    if (!maySaveTeam(d, busy)) return;
    setBusy(true);
    let saved: TeamDef | null = null;
    await attempt(async () => {
      // What is sent is the model's to say (`teamSaveOf`): the fields this
      // dialog changed of a team edited — a roster nobody touched is not
      // written back — or a new team.
      const save = teamSaveOf(d, team, seeded.current);
      saved = save.kind === "patch" ? (await api.patchTeam(save.id, save.body)).team : (await api.createTeam(save.body)).team;
    }, toast.error);
    setBusy(false);
    if (saved) {
      toast.ok(team ? tr("screens-teams-team-updated") : tr("screens-teams-team-created"));
      onSaved(saved);
      onClose();
    }
  };

  return (
    <>
    <Dialog
      open={open}
      onClose={requestClose}
      title={team ? tr("screens-teams-edit", { team: team.name }) : tr("screens-teams-new-team")}
      description={TEAM_EFFECT}
      footer={
        <>
          <Button variant="ghost" onClick={requestClose} disabled={busy}>{tr("screens-channels-cancel")}</Button>
          <Button variant="primary" onClick={() => void save()} disabled={!maySaveTeam(d, busy)}>
            {busy ? tr("screens-channels-saving") : team ? tr("screens-channels-save") : tr("screens-teams-create-team")}
          </Button>
        </>
      }
    >
      {/* Nothing is typed into a save that is out: every field is held while the node answers. */}
      <fieldset disabled={busy} className="m-0 min-w-0 border-0 p-0">
      <div className="flex flex-col gap-3">
        <Field label={tr("screens-channels-name")}>
          <TextInput
            value={d.name}
            autoFocus
            placeholder={tr("screens-teams-delivery")}
            onChange={(e) => set({ name: e.target.value })}
          />
        </Field>
        <Field label={tr("screens-teams-purpose")} hint={tr("screens-teams-what-team-operating-brief-optional")}>
          <TextArea
            rows={3}
            value={d.purpose}
            placeholder={tr("screens-teams-ships-product-surface-builds-reviews-signs")}
            onChange={(e) => set({ purpose: e.target.value })}
          />
        </Field>
        <Field label={tr("screens-teams-photo")} hint={tr("screens-teams-picture-team-s-card-rows-png")}>
          <PhotoField id={team?.id ?? "team"} name={d.name || team?.name} photo={d.photo} onChange={(photo) => set({ photo })} disabled={busy} />
        </Field>

        <Labelled
          label={tr("screens-teams-members")}
          hint={
            d.members.length === 0
              ? tr("screens-teams-agents-do-work-people-approve-gates")
              : tr("screens-teams-agents-and-people-chosen", { agents: nAgents, people: nHumans })
          }
        >
          <AssigneePicker
            value={d.members}
            onChange={(members) => set({ members })}
            kinds={[...ROSTER_KINDS]}
            exclude={notOffered}
            disabled={busy}
            placeholder={tr("screens-teams-search-agents-people")}
          />
        </Labelled>

        <Labelled label={tr("screens-channels-tags")} hint={tr("screens-teams-how-files-tags-how-find-one")}>
          <TagInput value={d.tags} onChange={(tags) => set({ tags })} suggestions={[...TAG_VOCABULARY]} />
        </Labelled>
      </div>
      </fieldset>
    </Dialog>
    {/* Asked over the dialog, which stays as it is underneath: *Keep editing* loses nothing. */}
    <ConfirmDialog
      open={discarding}
      onClose={() => setDiscarding(false)}
      onConfirm={() => {
        setDiscarding(false);
        onClose();
      }}
      title={tr("work-draft-discard-title")}
      body={team ? tr("work-draft-discard-edits-of", { name: team.name }) : tr("work-draft-discard-new-team")}
      confirmLabel={tr("work-draft-discard")}
      cancelLabel={tr("work-draft-keep-editing")}
      danger
    />
    </>
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
  onSwitch,
  switching,
  onDelete,
}: {
  t: TeamDef;
  /** Already loaded by the shell; the avatars name each member with it. */
  agents: AgentDef[];
  selected: boolean;
  onOpen: () => void;
  onEdit: () => void;
  /** Stand it down or bring it back. */
  onSwitch: () => void;
  /** The switch is out: its entry is held until the node answers. */
  switching: boolean;
  onDelete: () => void;
}) {
  const ws = useWorkspace();
  const flip = switchOf("team", t);
  const stored = storedMembers(t);
  const face = (m: Assignee) => memberFace(m, agents, ws);

  return (
    // A div rather than a clickable Card: this one holds its own controls, and
    // nesting a button inside a button hands the outer one every click.
    // The open team is where you are, not something asking for you: a neutral edge.
    <Card className={selected ? "border-text/35" : ""}>
      <div className="flex items-start gap-2">
        <button type="button" onClick={onOpen} className="group flex min-w-0 flex-1 items-start gap-2 text-left">
          <Avatar id={t.id} name={t.name} photo={t.photo} size={28} />
          <span className="min-w-0 flex-1">
          <span className="flex items-center gap-1.5">
            <span className="truncate text-sm font-medium underline-offset-2 group-hover:underline">{t.name}</span>
            <OriginChip origin={t.origin} id={t.id} />
            {t.enabled === false && <Chip tone="warn">{tr("screens-roster-stood-down")}</Chip>}
            {stored.length === 0 && <Chip tone="warn">{tr("screens-teams-not-staffed")}</Chip>}
          </span>
          {/* What somebody put on it: the count beside *Not staffed* never says two agents nobody chose. */}
          <span className="mt-0.5 block text-2xs text-text-dim">{rosterLine(stored)}</span>
          </span>
        </button>
        <MoreMenu
          vertical
          label={tr("screens-teams-actions", { t: t.name })}
          items={[
            { label: tr("screens-inbox-open"), icon: ICON.forward, onSelect: onOpen },
            { label: tr("screens-teams-edit-team"), icon: ICON.edit, onSelect: onEdit },
            ...(flip ? [{ label: switching ? flip.doing : flip.label, icon: flip.on ? ICON.pause : ICON.play, disabled: switching, onSelect: onSwitch }] : []),
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

      {/* Who is on it, as a line of faces and names rather than a cloud of
          pills: the same six and the same "+N", read as one quiet fact. */}
      <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1">
        {stored.slice(0, 6).map((m, i) => (
          <span
            key={`${assigneeWire(m)}:${i}`}
            className="inline-flex items-center gap-1 text-2xs text-text-dim"
            title={face(m).label}
          >
            <Avatar id={face(m).avatarId} name={face(m).label} photo={face(m).photo} size={14} />
            <span className="max-w-[8rem] truncate">{face(m).label}</span>
          </span>
        ))}
        {stored.length > 6 && (
          <span className="text-2xs text-text-dim">{tr("screens-teams-more-members", { more: stored.length - 6 })}</span>
        )}
      </div>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// One team
// ---------------------------------------------------------------------------

/**
 * The detail pane: who is on the team.
 *
 * It draws the list's own copy of the team — the one the screen reads again
 * on every frame about a team — so an edit made in the dialog, at the
 * prompt or on another node is on screen as soon as the list is. It used to
 * keep a read of its own, which an edit left behind: the pane went on
 * showing the old roster, and *Remove* wrote that old roster back, undoing
 * the edit.
 *
 * Mounted with the team id as its `key`, so selecting a different team
 * starts from that team alone.
 */
function TeamPane({
  team,
  usage,
  onEdit,
  onSwitch,
  switching,
  onDelete,
  onChanged,
}: {
  team: TeamDef;
  usage: ReturnType<typeof useUsage>;
  onEdit: () => void;
  onSwitch: () => void;
  switching: boolean;
  onDelete: () => void;
  /** A write landed; the roster on the left is now behind. */
  onChanged: () => void;
}) {
  const toast = useToast();
  const { describe } = useAssigneeOptions([...ROSTER_KINDS]);
  const [busy, setBusy] = useState(false);

  // What the node answered this pane's last write stands until the list is
  // read again (`shownTeam`): the next removal is made from that record,
  // never from the roster as it was before the first.
  const [answered, hold] = useHeldAnswer<TeamDef, TeamDef>(team);
  const current = shownTeam(team, answered);
  const stored = storedMembers(current);
  const flip = switchOf("team", team);

  /**
   * Take one member off, by writing the roster the team should have.
   *
   * A whole-list PATCH rather than a per-member route, matching every other
   * reference list in the app: one request cannot leave a roster half-changed,
   * and the order survives. The implicit member is never in `stored`, so this
   * cannot be asked to remove it. The latch holds until the node's answer is
   * what the pane draws.
   */
  const removeMember = (m: Assignee) => {
    setBusy(true);
    void attempt(
      () =>
        api.patchTeam(current.id, withoutMember(current, m)),
      toast.error,
      (res) => {
        hold(res.team);
        toast.ok(tr("screens-teams-member-removed"));
        onChanged();
      },
    ).finally(() => setBusy(false));
  };

  return (
    <div className="flex flex-col gap-4">

      <Card>
        <div className="flex items-center gap-1.5">
          <h2 className="truncate text-lg font-semibold tracking-tight">{current.name}</h2>
          <OriginChip origin={current.origin} id={current.id} />
          {team.enabled === false && <Chip tone="warn">{tr("screens-roster-stood-down")}</Chip>}
        </div>
        <p className="mt-0.5 text-2xs text-text-dim">{team.enabled === false ? tr("screens-teams-stood-down-pane") : rosterLine(stored)}</p>

        {(current.tags ?? []).length > 0 && (
          <div className="mt-2 flex flex-wrap gap-1">
            <TagChips tags={current.tags ?? []} />
          </div>
        )}

        {current.purpose && (
          <p className="mt-2 max-w-measure whitespace-pre-wrap text-xs leading-relaxed text-text-dim">{current.purpose}</p>
        )}

        <p className="mt-2 max-w-measure text-2xs leading-relaxed text-text-dim">{TEAM_EFFECT}</p>

        {/* The pane's own actions: the screen's one primary is New team, up in the toolbar. */}
        <div className="mt-3 flex flex-wrap items-center gap-2">
          <Button size="sm" variant="ghost" onClick={onEdit}>{tr("screens-teams-edit-team")}</Button>
          {flip && (
            <Button size="sm" variant="ghost" disabled={switching} onClick={onSwitch}>
              {flip.on ? <ICON.pause size={12} aria-hidden /> : <ICON.play size={12} aria-hidden />}
              {switching ? flip.doing : flip.label}
            </Button>
          )}
          <DeleteButton usage={usage} onOpen={onDelete} />
        </div>
      </Card>

      {/* The count is of who was put here — the number the card says. */}
      <Section title={tr("screens-teams-members-2", { members: stored.length })}>
        {/* The implicit member means this list is never empty, so the count
            above cannot answer tr("screens-teams-has-anybody-been-put"). This can. */}
        {stored.length === 0 && (
          <EmptyState
            icon={ICON.team}
            title={tr("screens-teams-nobody-has-been-put-team")}
            hint={tr("screens-teams-carries-no-work-until-has-members")}
            // The pane's door, not the screen's ask: the one primary is New team, up in the toolbar.
            action={<Button onClick={onEdit}>{tr("screens-teams-add-members")}</Button>}
          />
        )}

        {/* One list, rows divided by a hairline, rather than a stack of boxes. */}
        <AnimatedList
          className="mt-1.5 flex flex-col divide-y divide-hairline rounded-control border border-border"
          items={current.members}
          keyOf={(m, i) => `${assigneeWire(m)}:${i}`}
          render={(m) => {
            // The same describe() the picker uses, so a member who has since
            // been deleted reads as missing rather than as a raw id.
            const o = describe(assigneeWire(m));
            const implicit = isImplicitMember(m);
            return (
              <div
                className={`flex min-h-row-lg items-center gap-2.5 px-2.5 py-1.5 ${
                  implicit ? "bg-surface-2/50" : ""
                }`}
              >
                <Avatar id={o.avatar} name={o.name} photo={o.photo} size={20} />
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-xs">{o.name}</span>
                  <span className="block truncate text-2xs text-text-dim">{o.sub}</span>
                </span>
                {/* A member's kind is identity, not a summons: neutral. A team inside a team is the anomaly, so it keeps warn. */}
                <Chip tone={o.kind === "team" ? "warn" : "neutral"}>
                  {o.kind === "human" ? tr("screens-teams-person") : o.kind === "agent" ? tr("screens-teams-kind-agent") : tr("screens-teams-kind-team")}
                </Chip>
                {implicit ? (
                  <Tooltip label={tr("screens-teams-member-every-team-stored-none-them")}>
                    <span>
                      <Chip>{tr("screens-teams-implicit")}</Chip>
                    </span>
                  </Tooltip>
                ) : (
                  <Button
                    size="sm"
                    variant="ghost"
                    disabled={busy}
                    aria-label={tr("screens-teams-remove-named", { name: o.name })}
                    onClick={() => removeMember(m)}
                  >{tr("screens-teams-remove")}</Button>
                )}
              </div>
            );
          }}
        />

        {current.members.some(isImplicitMember) && (
          <p className="mt-2 max-w-measure text-2xs leading-relaxed text-text-dim">{tr("screens-teams-platform-own-agent-belongs-every-team")}</p>
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
  // The teams a switch is out for: each one's control is held until the node answers.
  const [switching, setSwitching] = useState<string[]>([]);
  /** Stand one team down or bring it back: `enabled` alone, on the record as it stands. */
  const flip = (team: TeamDef) => {
    const turn = switchOf("team", team);
    if (!turn || switching.includes(team.id)) return;
    setSwitching((ids) => [...ids, team.id]);
    void attempt(() => api.patchTeam(team.id, turn.body), toast.error, () => {
      toast.ok(turn.done);
      reload();
    }).finally(() => setSwitching((ids) => ids.filter((id) => id !== team.id)));
  };
  const [editing, setEditing] = useState<TeamDef | null>(null);
  // The id a delete was asked of — never a bare yes (`asksDelete`).
  const [deleting, setDeleting] = useState<string | null>(null);
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

  // A pick in a narrow screen: the team opens stacked under the list, out of
  // sight, so it is brought into view — the nearest edge, the least movement.
  // Beside the list it is already in view and nothing moves; the place the
  // screen opens on stays `useViewScroll`'s to put back.
  const rosterCol = useRef<HTMLDivElement>(null);
  const detailCol = useRef<HTMLDivElement>(null);
  const picked = useRef(selectedId);
  useEffect(() => {
    if (picked.current === selectedId) return;
    picked.current = selectedId;
    const detail = detailCol.current;
    if (!selectedId || !detail) return;
    if (detailStacked(rosterCol.current?.getBoundingClientRect(), detail.getBoundingClientRect())) detail.scrollIntoView({ block: "nearest" });
  }, [selectedId]);

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
          open={asksDelete(deleting, selected)}
          title={tr("screens-teams-delete", { selected: selected.name })}
          usage={usage}
          consequence={tr("screens-teams-definition-removed-nothing-assigned-no-gate")}
          remove={() => api.deleteTeam(selected.id)}
          onClose={() => setDeleting(null)}
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
          hint={tr("screens-teams-teams-catalog-installing", { TEAM_EFFECT })}
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
    // The root the page's scroll is kept from: the screen's band, then the scroll under it.
    <div ref={root} className="flex h-full min-h-0 flex-col">
    {/* The screen's band (`ui/ScreenBar.tsx`): the search narrows the list; the count, the re-read and *New team* last, as on every index screen. */}
    <ScreenBar
      end={
        <>
          <span className="tnum text-2xs text-text-dim">{tr("screens-teams-words", { shown: shown.length, teams: teams.length })}</span>
          <Tooltip label={tr("screens-teams-re-read-list")}>
            <Button size="icon" variant="ghost" onClick={reload} aria-label={tr("screens-teams-refresh-teams")}>
              <ICON.refresh size={14} aria-hidden />
            </Button>
          </Tooltip>
          {/* The catalog's teams, ready to install — Settings › Library › Teams — beside the screen's one primary. */}
          <Tooltip label={tr("screens-teams-catalog-hint")}>
            <Button size="sm" onClick={() => navigate({ name: "settings" }, settingsSearch("catalog-team"))}>
              <ICON.catalog size={12} aria-hidden />
              {tr("screens-agents-catalog")}
            </Button>
          </Tooltip>
          <Button size="sm" variant="primary" onClick={() => setCreating(true)}>{tr("screens-teams-new-team")}</Button>
        </>
      }
    >
      <TextInput
        value={query}
        placeholder={tr("screens-teams-search-teams")}
        aria-label={tr("screens-teams-search-teams-2")}
        className="h-7 w-full max-w-64 py-0"
        onChange={(e) => setQuery(e.target.value)}
      />
    </ScreenBar>
    {/* A size container: list and detail split by the room this screen has —
        the window, less the sidebar and any pane beside it — never by the
        window alone, which split a screen squeezed to 360px by an open pane. */}
    <div data-scroll-keep={`page:${selectedId ?? ""}`} className="@container min-h-0 flex-1 overflow-y-auto p-6">
      <div className="grid gap-6 @2xl:grid-cols-[1.3fr_1fr]">
        {/* A size container: the cards split into two columns by this column's
            width, not the window's, so a narrow window beside the detail pane
            never squeezes a team's name to its first letter. */}
        <div ref={rosterCol} className="@container flex min-w-0 flex-col gap-2">
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
              className="grid gap-2 @xl:grid-cols-2"
              items={shown}
              keyOf={(t) => t.id}
              render={(t) => (
                <TeamCard
                  t={t}
                  agents={agents}
                  selected={t.id === selectedId}
                  onOpen={() => select(t.id === selectedId ? null : t.id)}
                  onEdit={() => setEditing(t)}
                  onSwitch={() => flip(t)}
                  switching={switching.includes(t.id)}
                  onDelete={() => {
                    // Selecting first is what puts the usage check on this
                    // team; the dialog opens on its "checking" body and fills
                    // in when the answer lands.
                    select(t.id);
                    setDeleting(t.id);
                  }}
                />
              )}
            />
          )}
        </div>

        {/* Beside the roster the detail stays in view as the roster scrolls — the
            list ran past two screens while the detail fit in a third of one —
            with a scroll of its own when it is the taller. Stacked under a
            narrow roster it scrolls with the page, as before. */}
        <div ref={detailCol} className="min-w-0 @2xl:sticky @2xl:top-0 @2xl:max-h-[calc(100vh-11rem)] @2xl:self-start @2xl:overflow-y-auto">
          {selected ? (
            <TeamPane
              key={selected.id}
              team={selected}
              usage={usage}
              onEdit={() => setEditing(selected)}
              onSwitch={() => flip(selected)}
              switching={switching.includes(selected.id)}
              onDelete={() => setDeleting(selected.id)}
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
            // Nothing picked yet: an open door, never a box — the list beside it is the action.
            <EmptyState icon={ICON.team} title={tr("screens-teams-pick-team")} hint={tr("screens-teams-who-what-carrying-how-files-show", { TEAM_EFFECT })} action={null} className="py-16" />
          )}
        </div>
      </div>

      {dialogs}
    </div>
    </div>
  );
}
