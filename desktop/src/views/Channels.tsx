/**
 * A channel: a standing conversation the whole workspace can see.
 *
 * The list used to live here, which meant leaving a channel to reach another
 * one. The sidebar owns the list now, so this route is the conversation —
 * and, with no channel picked, the Channels page: the same list as a page,
 * searched, each room with its last words (`ChannelIndex`, over
 * `channelListModel.mjs`, the sidebar's order).
 *
 * The roster is the thing this screen has to say out loud. A channel's `agents`
 * list is a **directory, not a subscription**: it is deliberately not the
 * audience, because a non-empty audience encrypts the room pairwise to exactly
 * those principals and turns a standing channel into a private one. So a
 * rostered agent does not auto-reply — five agents on `#engineering` must
 * never mean five harness sessions per message. What the roster buys is who
 * belongs here, the order of the `@`-picker, and the channel handle.
 *
 * Editing reaches the topic, the roster and the tags, and nothing else. The
 * audience is fixed when the conversation opens: past messages were encrypted
 * to exactly the principals in it, and no edit can reach back and change who
 * they were encrypted to. So a DM stays a DM, a standing channel stays
 * workspace-visible, and this screen does not offer a control for either — the
 * node refuses a DM edit outright, and an unofferable control that fails on
 * press is worse than no control.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import { href, navigate } from "../router";
import { listUnread } from "../shell/workspaceLoadModel.mjs";
import { useWorkspace } from "../shell/useWorkspaceData";
import { placeOf, useViewState } from "../shell/viewMemoryStore";
import { NEW_CHANNEL, onDoor } from "../shell/shortcuts";
import {
  AgentPicker,
  Avatar,
  Button,
  ConfirmDialog,
  CountBadge,
  Dialog,
  EmptyState,
  ErrorNote,
  Field,
  ICON,
  Labelled,
  MoreMenu,
  NO_TAG_FILTER,
  Popover,
  RelativeTime,
  ScreenBar,
  ScrollArea,
  Skeleton,
  SkeletonRows,
  TAG_VOCABULARY,
  TagChips,
  TagFilterBar,
  TagInput,
  TextInput,
  Tooltip,
  parseTagFilter,
  useToast,
  WorkingDot,
  type TagFilterState,
} from "../ui";
import { textValue } from "../shell/viewValuesModel.mjs";
import { filterChannels, latestWords, sortChannels } from "./_studio/channelListModel.mjs";
import type { AgentDef, ChannelDef } from "../types";
import { rosterAgents, rosterHumans } from "../types";
import { Conversation, ConversationHeader } from "./_studio/Conversation";
import { usePeopleRosterCandidates, useParticipants, useRosterCandidates } from "./_studio/participants";
import { resolveRoster } from "./_studio/participantsModel.mjs";
import { attempt, useAsync } from "./_work/useAsync";
import { readKey } from "./_work/keptReadsModel.mjs";
import { dirty, mayClose } from "./_work/agentDraftModel.mjs";
import { DeleteDialog, useUsage } from "./_work/LibraryRefs";
import { goneMemberName } from "./rosterModel.mjs";
import {
  channelCreateOf,
  channelDraftOf,
  channelSaveOf,
  channelStamp,
  editsRoster,
  emptyChannelDraft,
  mayCreateChannel,
  mayDeleteChannel,
  mayEditChannel,
  reachOf,
  shownChannel,
  unresolvedRoster,
  type ChannelDraft,
} from "./_studio/channelDraftModel.mjs";
import { useGonePlace } from "../shell/useGonePlace";
import { t } from "../i18n/l10n.mjs";

/**
 * The one sentence somebody has to read before they pick a roster, placed
 * where they are picking it. Said once, in the app, rather than only in
 * `docs/guide/channels.md`, where the person configuring a channel will not be.
 */
const ROSTER_RULE =
  t("screens-channels-roster-who-belongs-here-what-channel");

/** Roster names, in roster order, with an id that resolves to nothing marked; the people listed after the agents. */
function RosterLine({ ids, agents, humans = [] }: { ids: string[]; agents: AgentDef[]; humans?: string[] }) {
  const ws = useWorkspace();
  const entries = resolveRoster(ids, agents);
  if (entries.length === 0 && humans.length === 0) return null;
  return (
    // A flex item: `min-w-0` lets it shrink below its names, so a long roster ends in an ellipsis instead of running past the header's edge.
    <span className="min-w-0 truncate">
      {entries.map((e, i) => (
        <span key={e.id}>
          {i > 0 && ", "}
          {e.agent ? (
            e.name
          ) : (
            <Tooltip label={t("screens-channels-no-agent-id-node-definition-deleted")}>
              <span className="cursor-default font-mono text-warn">{t("screens-channels-unresolved", { e: e.id })}</span>
            </Tooltip>
          )}
        </span>
      ))}
      {humans.map((h, i) => (
        <span key={h}>
          {(entries.length > 0 || i > 0) && ", "}
          <Tooltip label={t("screens-channels-person-another-node-channel-s-roster")}>
            <span className="cursor-default">{ws.nameOf(h)}</span>
          </Tooltip>
        </span>
      ))}
    </span>
  );
}

/**
 * The people half of a roster (14-collaboration): hosted members, toggled.
 * A guest reaches this channel only when listed here; an admin or a member
 * reaches every standing channel anyway.
 */
function PeoplePicker({ value, onChange }: { value: string[]; onChange: (next: string[]) => void }) {
  const candidates = usePeopleRosterCandidates();
  if (candidates.length === 0) return null;
  return (
    <Labelled label={t("screens-channels-people")} hint={t("screens-channels-guest-reaches-only-when-listed")}>
      <AgentPicker
        candidates={candidates}
        value={value}
        onChange={onChange}
        chips={false}
        label={t("screens-channels-people-roster")}
        placeholder={t("screens-channels-filter-person-people", { candidates: candidates.length })}
        listClassName="max-h-40 rounded-control border border-border"
        emptyTitle={t("screens-channels-nobody-list")}
        note={value.length > 0 ? t("screens-channels-listed", { length: value.length }) : undefined}
      />
    </Labelled>
  );
}

/** A roster entry's name, with *disabled* said of an agent the handle will not reach today. */
function rosterEntryName(agent: AgentDef, name: string): string {
  return agent.enabled === false ? t("screens-channels-roster-entry-disabled", { name }) : name;
}

/**
 * What `@channel` expands to, on demand.
 *
 * The header can only afford one line of roster, and the roster is the thing
 * about a channel that is most often misread — so the handle itself opens the
 * full list, with the rule underneath it. Six names in a header would be
 * truncated; six names in a popover are six names.
 */
function RosterPeek({
  name,
  ids,
  agents,
  reach,
}: {
  name: string;
  ids: string[];
  agents: AgentDef[];
  /** How many agents the handle reaches by the node's own answer; `null` until it has answered. */
  reach: number | null;
}) {
  const entries = resolveRoster(ids, agents);
  return (
    <div className="flex w-64 flex-col gap-2">
      {/* The count is the node's: a team's agents are in it, a disabled agent is not, and the two core agents come last. Until it answers, the roster is named and no count is claimed. */}
      <p className="text-2xs font-medium text-text">
        {reach === null ? t("screens-channels-roster-of", { name }) : t("screens-channels-reaches-agents", { name, agents: reach })}
      </p>
      <ScrollArea className="max-h-56">
        <ul className="flex flex-col gap-0.5">
          {entries.map((e) => (
            <li key={e.id} className="flex min-w-0 items-center gap-2">
              {e.agent ? (
                <Avatar
                  id={e.agent.pubkey}
                  name={e.agent.name}
                  photo={e.agent.photo}
                  size={16}
                />
              ) : (
                <ICON.warn size={14} aria-hidden className="shrink-0 text-warn" />
              )}
              <span className={`truncate text-2xs ${e.agent ? "text-text" : "text-warn"}`}>
                {e.agent ? rosterEntryName(e.agent, e.name) : goneMemberName("agent", e.id)}
              </span>
            </li>
          ))}
        </ul>
      </ScrollArea>
      <p className="text-2xs text-text-dim">{ROSTER_RULE}</p>
    </div>
  );
}

/**
 * The roster picker: agent definitions, toggled, ordered as you pick them.
 *
 * It offered the core agent, and the node stores it zero times — a roster
 * submitted containing it is stripped on write, so you ticked it, saved, and
 * found it gone. `useRosterCandidates` leaves it out. A *disabled* agent is
 * still offered, and marked: a roster is a directory of who belongs in this
 * room, which an agent that is switched off today has not stopped doing.
 */
function RosterPicker({
  value,
  onChange,
}: {
  value: string[];
  onChange: (next: string[]) => void;
}) {
  const candidates = useRosterCandidates();
  return (
    <AgentPicker
      candidates={candidates}
      value={value}
      onChange={onChange}
      chips={false}
      label={t("screens-channels-roster")}
      placeholder={t("screens-channels-filter-agent-agents-name-role-tag", { candidates: candidates.length })}
      listClassName="max-h-56 rounded-control border border-border"
      emptyTitle={t("screens-channels-no-agents-put-roster")}
      emptyHint={t("screens-channels-install-one-from-catalog-settings-channel")}
      note={
        value.length > 0
          ? t("screens-channels-roster-order", { length: value.length })
          : undefined
      }
    />
  );
}

/**
 * What both channel dialogs share: one way out. Escape, a click outside, the
 * ✕ and Cancel are the same door — shut while a write is out, and asking
 * before it drops what was typed (`agentDraftModel.mayClose`).
 */
function useGuardedClose(edited: boolean, busy: boolean, onClose: () => void) {
  const [discarding, setDiscarding] = useState(false);
  const requestClose = () => {
    const leave = mayClose({ dirty: edited, busy });
    if (leave === "close") onClose();
    else if (leave === "ask") setDiscarding(true);
  };
  return { discarding, setDiscarding, requestClose };
}

/** The question asked over a dialog whose edits would be dropped: *Keep editing* loses nothing. */
function DiscardEdits({ open, body, onKeep, onDiscard }: { open: boolean; body: string; onKeep: () => void; onDiscard: () => void }) {
  return (
    <ConfirmDialog
      open={open}
      onClose={onKeep}
      onConfirm={onDiscard}
      title={t("work-draft-discard-title")}
      body={body}
      confirmLabel={t("work-draft-discard")}
      cancelLabel={t("work-draft-keep-editing")}
      danger
    />
  );
}

/**
 * The roster names no agent here answers to — a definition deleted, or one
 * this node has not synced. The picker cannot draw them, so they are listed,
 * each with a way to take it off: left unseen, they rode every save.
 */
function UnresolvedRoster({ ids, onRemove }: { ids: string[]; onRemove: (id: string) => void }) {
  if (ids.length === 0) return null;
  return (
    <ul className="flex flex-col gap-1" aria-label={t("screens-channels-roster-unresolved")}>
      {ids.map((id) => {
        const name = goneMemberName("agent", id);
        return (
          <li key={id} className="flex min-w-0 items-center gap-2 text-2xs text-text-dim">
            <ICON.warn size={13} aria-hidden className="shrink-0 text-warn" />
            <span className="min-w-0 flex-1 truncate">{t("screens-channels-roster-unresolved-line", { name })}</span>
            <Button size="sm" variant="ghost" onClick={() => onRemove(id)} aria-label={t("screens-channels-roster-take-off", { name })}>
              {t("screens-channels-roster-take-off-short")}
            </Button>
          </li>
        );
      })}
    </ul>
  );
}

function NewChannelDialog({
  open,
  onClose,
  onCreated,
}: {
  open: boolean;
  onClose: () => void;
  onCreated: (id: string) => void;
}) {
  const [d, setD] = useState<ChannelDraft>(emptyChannelDraft);
  const set = (patch: Partial<ChannelDraft>) => setD((prev) => ({ ...prev, ...patch }));
  const blank = useRef<ChannelDraft>(d);
  // A refusal is said in the dialog, where the name was typed — as the edit
  // dialog says its own — not in a toast that leaves with the moment.
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const { discarding, setDiscarding, requestClose } = useGuardedClose(dirty(d, blank.current), busy, onClose);

  useEffect(() => {
    if (!open) return;
    const fresh = emptyChannelDraft();
    blank.current = fresh;
    setD(fresh);
    setError(null);
    setDiscarding(false);
    // `setDiscarding` is a state setter: the dialog re-seeds when it opens, and for nothing else.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const create = async () => {
    if (!mayCreateChannel(d, busy)) return;
    setBusy(true);
    setError(null);
    const ok = await attempt(async () => {
      const { channel } = await api.createChannel(channelCreateOf(d));
      onCreated(channel.id);
    }, setError);
    setBusy(false);
    if (ok) onClose();
  };

  return (
    <>
      <Dialog
        open={open}
        onClose={requestClose}
        title={t("screens-channels-new-channel")}
        description={t("screens-channels-standing-conversation-whole-workspace-can-read")}
        footer={
          <>
            <Button variant="ghost" disabled={busy} onClick={requestClose}>{t("screens-channels-cancel")}</Button>
            <Button variant="primary" disabled={!mayCreateChannel(d, busy)} onClick={() => void create()}>
              {busy ? t("screens-channels-creating") : t("screens-channels-create")}
            </Button>
          </>
        }
      >
        {/* Nothing is edited under a write on its way: what is on screen is what was sent. */}
        <fieldset disabled={busy} className="m-0 min-w-0 border-0 p-0">
          <div className="flex flex-col gap-3">
          {error && <ErrorNote error={error} />}
          <Field label={t("screens-channels-name")}>
            <TextInput
              autoFocus
              value={d.name}
              placeholder={t("screens-channels-design")}
              onChange={(e) => set({ name: e.target.value })}
              onKeyDown={(e) => {
                if (e.key === "Enter") void create();
              }}
            />
          </Field>
          <Field label={t("screens-channels-topic")} hint={t("screens-channels-what-channel-optional")}>
            <TextInput
              value={d.topic}
              placeholder={t("screens-channels-how-looks-why")}
              onChange={(e) => set({ topic: e.target.value })}
            />
          </Field>
          <Labelled label={t("screens-channels-roster")} hint={ROSTER_RULE}>
            <RosterPicker value={d.agents} onChange={(agents) => set({ agents })} />
          </Labelled>
          <PeoplePicker value={d.humans} onChange={(humans) => set({ humans })} />
          <Labelled label={t("screens-channels-tags")}>
            <TagInput value={d.tags} onChange={(tags) => set({ tags })} suggestions={[...TAG_VOCABULARY]} />
          </Labelled>
          </div>
        </fieldset>
      </Dialog>
      <DiscardEdits
        open={discarding}
        body={t("work-draft-discard-new-channel")}
        onKeep={() => setDiscarding(false)}
        onDiscard={() => {
          setDiscarding(false);
          onClose();
        }}
      />
    </>
  );
}

/**
 * Editing: the topic, the roster and the tags — each written only when this
 * dialog changed it (`channelDraftModel.channelSaveOf`), so a dialog left
 * open while the roster moved elsewhere saves its topic and leaves the
 * roster as it now is. `general` is everyone's room: its roster is a policy
 * the node keeps whatever is named, so its editor offers the topic and the
 * tags and no roster.
 *
 * The node refuses this on a DM, so the caller does not offer it there. The
 * refusal is still surfaced in its own words if one ever arrives — a channel
 * can change kind under you between the render and the press, and "could not
 * save" would hide the one sentence that explains why.
 */
function EditChannelDialog({
  open,
  channel,
  onClose,
  onSaved,
}: {
  open: boolean;
  channel: ChannelDef;
  onClose: () => void;
  onSaved: () => void;
}) {
  const toast = useToast();
  const ws = useWorkspace();
  const [d, setD] = useState<ChannelDraft>(() => channelDraftOf(channel));
  const set = (patch: Partial<ChannelDraft>) => setD((prev) => ({ ...prev, ...patch }));
  // The draft as the dialog opened: what *edited* is measured against, and what a save names its changes from.
  const seeded = useRef<ChannelDraft>(d);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const { discarding, setDiscarding, requestClose } = useGuardedClose(dirty(d, seeded.current), busy, onClose);

  // Seeded when the dialog opens — on another channel, or again — and never
  // under what is being typed: the record moving behind an open dialog (a
  // reload, another window's edit) leaves the draft as the person has it.
  useEffect(() => {
    if (!open) return;
    const fresh = channelDraftOf(channel);
    seeded.current = fresh;
    setD(fresh);
    setError(null);
    setDiscarding(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, channel.id]);

  const withRoster = editsRoster(channel);
  const unresolved = withRoster ? unresolvedRoster(d.agents, ws.agents) : [];

  const save = async () => {
    if (busy) return;
    const body = channelSaveOf(d, channel, seeded.current);
    // Nothing changed is nothing to write — and nothing to say was updated.
    if (Object.keys(body).length === 0) {
      onClose();
      return;
    }
    setBusy(true);
    setError(null);
    const ok = await attempt(() => api.patchChannel(channel.id, body), setError);
    setBusy(false);
    if (ok) {
      toast.ok(t("screens-channels-channel-updated", { channel: channel.name }));
      onSaved();
      onClose();
    }
  };

  return (
    <>
      <Dialog
        open={open}
        onClose={requestClose}
        title={t("screens-channels-edit-2", { channel: channel.name })}
        description={withRoster ? t("screens-channels-topic-roster-tags-who-can-read") : t("screens-channels-topic-tags-everyone-belongs")}
        footer={
          <>
            <Button variant="ghost" disabled={busy} onClick={requestClose}>{t("screens-channels-cancel")}</Button>
            <Button variant="primary" disabled={busy} onClick={() => void save()}>
              {busy ? t("screens-channels-saving") : t("screens-channels-save")}
            </Button>
          </>
        }
      >
        <fieldset disabled={busy} className="m-0 min-w-0 border-0 p-0">
          <div className="flex flex-col gap-3">
          {error && <ErrorNote error={error} />}
          <Field label={t("screens-channels-topic")} hint={t("screens-channels-what-channel-clearing-removes")}>
            <TextInput
              autoFocus
              value={d.topic}
              placeholder={t("screens-channels-how-looks-why")}
              onChange={(e) => set({ topic: e.target.value })}
            />
          </Field>
          {withRoster && (
            <>
              <Labelled label={t("screens-channels-roster")} hint={ROSTER_RULE}>
                <RosterPicker value={d.agents} onChange={(agents) => set({ agents })} />
                <UnresolvedRoster ids={unresolved} onRemove={(id) => set({ agents: d.agents.filter((a) => a !== id) })} />
              </Labelled>
              <PeoplePicker value={d.humans} onChange={(humans) => set({ humans })} />
            </>
          )}
          <Labelled label={t("screens-channels-tags")}>
            <TagInput value={d.tags} onChange={(tags) => set({ tags })} suggestions={[...TAG_VOCABULARY]} />
          </Labelled>
          </div>
        </fieldset>
      </Dialog>
      <DiscardEdits
        open={discarding}
        body={t("work-draft-discard-edits-of", { name: channel.name })}
        onKeep={() => setDiscarding(false)}
        onDiscard={() => {
          setDiscarding(false);
          onClose();
        }}
      />
    </>
  );
}

/**
 * Delete a channel — a standing one or a direct one, never `general`
 * (`channelDraftModel.mayDeleteChannel`; the caller offers it by that rule).
 * Asked what still names the room first (`useUsage`): a workflow that posts
 * or listens there is listed, with no Delete to press, and a refusal that
 * arrives all the same is kept in the dialog in the node's words.
 */
export function DeleteChannelDialog({
  open,
  channel,
  name,
  onClose,
  onDeleted,
}: {
  open: boolean;
  channel: ChannelDef;
  /** What the room is called where it is asked about: a channel's name, a direct channel's people. */
  name: string;
  onClose: () => void;
  onDeleted: () => void;
}) {
  const toast = useToast();
  // Asked only when the dialog is: a row's menu does not cost every row a request.
  const usage = useUsage("channel", open ? channel.id : null);
  const direct = channel.kind === "direct";
  return (
    <DeleteDialog
      open={open}
      title={direct ? t("screens-channels-delete-direct", { name }) : t("screens-channels-delete", { name })}
      usage={usage}
      consequence={direct ? t("screens-channels-delete-direct-consequence") : t("screens-channels-delete-consequence")}
      remove={() => api.deleteChannel(channel.id)}
      onClose={onClose}
      onDeleted={() => {
        toast.ok(direct ? t("screens-channels-direct-deleted") : t("screens-channels-channel-deleted", { name }));
        onDeleted();
      }}
    />
  );
}

/** Where the index keeps its memory: the list's place, not a channel's. */
const INDEX_PLACE = placeOf({ name: "channels" });

/**
 * The Channels page: every standing channel as a row — the name, the topic,
 * when it last moved and who said what last (`latest`, kept live by the
 * bus), the roster's names when nothing was said yet, its tags, a working
 * dot, the **live** unread count (`ws.unread`, never the load's snapshot)
 * and a `⋮` with *Open* and *Edit…*. One toolbar in the Goals screen's
 * grammar: the words, the count, *New channel*, the tag facets. The order
 * is the sidebar's (`channelListModel.sortChannels`): `general` first, then
 * creation — a message never reshuffles the rooms; the search is how one is
 * found fast.
 */
function ChannelIndex({ onNew }: { onNew: () => void }) {
  const ws = useWorkspace();
  // The tags picked and the words typed are the index's memory: it comes back narrowed as it was left.
  const [filter, setFilter] = useViewState<TagFilterState>(INDEX_PLACE, "tags", NO_TAG_FILTER, parseTagFilter);
  const [search, setSearch] = useViewState(INDEX_PLACE, "search", "", textValue);
  /** *Edit…* picked on a row: the same dialog the channel's own header opens. Held by id, so the dialog edits the record as the list now has it. */
  const [editingId, setEditingId] = useState<string | null>(null);
  /** *Delete…* picked on a row, by id: a room that left the list takes its question with it. */
  const [deletingId, setDeletingId] = useState<string | null>(null);

  const entries = ws.channels;
  const editing = shownChannel(entries, editingId, null);
  const deleting = shownChannel(entries, deletingId, null);
  const shown = useMemo(() => sortChannels(filterChannels(entries, search, filter)), [entries, search, filter]);
  const narrowed = search.trim() !== "" || filter.selected.length > 0;
  const clear = () => {
    setSearch("");
    setFilter(NO_TAG_FILTER);
  };

  return (
    <div className="flex h-full flex-col">
      {/* The screen's band (`ui/ScreenBar.tsx`): the words narrow the list; the count and *New channel* last. */}
      <ScreenBar
        end={
          <>
            <span className="tnum text-2xs text-text-dim">{t("screens-goals-words", { shown: shown.length, rows: entries.length })}</span>
            {narrowed && (
              <Button size="sm" variant="ghost" onClick={clear}>{t("screens-agents-clear-filters")}</Button>
            )}
            <Button size="sm" variant="primary" onClick={onNew}>
              <ICON.add size={12} aria-hidden />{t("screens-channels-new-channel")}</Button>
          </>
        }
      >
        <span className="relative">
          <ICON.search size={12} aria-hidden className="pointer-events-none absolute top-1/2 left-2 -translate-y-1/2 text-text-dim" />
          <TextInput value={search} placeholder={t("screens-channels-search")} aria-label={t("screens-channels-search")} className="h-7 w-56 py-0 pl-7" onChange={(e) => setSearch(e.target.value)} />
        </span>
      </ScreenBar>
      {/* An `@container`: a row's topic shows by the list's width, not the window's. */}
      <div data-scroll-keep="list" className="@container min-h-0 flex-1 overflow-y-auto px-6 py-4">
        {/* The tags, at the head of the list as on every index screen. */}
        <TagFilterBar items={entries} tagsOf={(e) => e.channel.tags ?? []} value={filter} onChange={setFilter} className="mb-4" />
        {/* Unread is unknown, never empty: before the workspace answers, the shape of the list; a read that failed, the reason and a way to ask again — as on Goals. */}
        {!ws.ready ? (
          <SkeletonRows rows={4} />
        ) : entries.length === 0 && listUnread(ws.degraded, ws.offline, "channels") ? (
          <ErrorNote error={t("screens-channels-could-not-read")} retry={ws.refresh} />
        ) : entries.length === 0 ? (
          <EmptyState
            icon={ICON.channel}
            title={t("screens-channels-no-channels-yet")}
            hint={t("screens-channels-channel-standing-conversation-place-topic-outlives")}
            action={<Button variant="primary" onClick={onNew}>{t("screens-channels-new-channel")}</Button>}
          />
        ) : shown.length === 0 ? (
          <EmptyState
            icon={ICON.filter}
            title={t("screens-agents-nothing-matches")}
            hint={t("screens-channels-nothing-matches-words-tags")}
            action={<Button variant="ghost" onClick={clear}>{t("screens-agents-clear-filters")}</Button>}
          />
        ) : (
          <div className="flex flex-col">
            {shown.map(({ channel, latest }) => {
              const unread = ws.unread[channel.id] ?? 0;
              const busy = (ws.working[channel.id] ?? []).length > 0;
              const said = latestWords(latest, ws.nameOf);
              const roster = rosterAgents(channel);
              return (
                <div key={channel.id} className="anim flex min-h-row-lg min-w-0 items-start gap-2 rounded-control px-3 py-1.5 hover:bg-surface-2">
                  <ICON.channel size={14} aria-hidden className="mt-0.5 shrink-0 text-text-dim" />
                  <a href={href({ name: "channel", id: channel.id })} className="flex min-w-0 flex-1 flex-col gap-0.5">
                    <span className="flex min-w-0 items-center gap-2">
                      {/* The name holds its width (to half the row) and the topic takes what is left: a long topic never squeezes the name to its first letters. */}
                      <span className={`max-w-[50%] shrink-0 truncate text-sm text-text ${unread > 0 ? "font-semibold" : "font-medium"}`}>{channel.name}</span>
                      {channel.topic && <span className="hidden min-w-0 truncate text-2xs text-text-dim @lg:inline">{channel.topic}</span>}
                      {latest && <RelativeTime at={latest.at} className="tnum ml-auto shrink-0 text-2xs text-text-dim" />}
                    </span>
                    <span className="flex min-w-0 items-center gap-1 text-2xs text-text-dim">
                      {said ? (
                        <span className="truncate">{said}</span>
                      ) : roster.length > 0 ? (
                        <>
                          <span className="shrink-0">{t("screens-channels-roster-2")}</span>
                          <RosterLine ids={roster} agents={ws.agents} humans={rosterHumans(channel)} />
                        </>
                      ) : (
                        <span className="truncate">{t("screens-channels-nothing-said-yet")}</span>
                      )}
                    </span>
                  </a>
                  <span className="flex shrink-0 items-center gap-1.5">
                    <TagChips tags={channel.tags ?? []} max={3} />
                    {busy && <WorkingDot title={t("screens-channels-agent-writing-here")} />}
                    <CountBadge count={unread} tone="neutral" />
                    <MoreMenu
                      vertical
                      label={t("screens-channels-menu", { channel: channel.name })}
                      items={[
                        { label: t("screens-channels-open"), icon: ICON.forward, onSelect: () => navigate({ name: "channel", id: channel.id }) },
                        { label: t("screens-channels-edit-2", { channel: channel.name }), icon: ICON.edit, onSelect: () => setEditingId(channel.id) },
                        ...(mayDeleteChannel(channel)
                          ? [{ label: t("screens-channels-delete-menu", { channel: channel.name }), icon: ICON.delete, danger: true, onSelect: () => setDeletingId(channel.id) }]
                          : []),
                      ]}
                    />
                  </span>
                </div>
              );
            })}
          </div>
        )}
      </div>
      {editing && (
        <EditChannelDialog
          open
          channel={editing}
          onClose={() => setEditingId(null)}
          onSaved={() => {
            setEditingId(null);
            ws.refresh();
          }}
        />
      )}
      {deleting && (
        <DeleteChannelDialog
          open
          channel={deleting}
          name={`#${deleting.name}`}
          onClose={() => setDeletingId(null)}
          onDeleted={() => {
            setDeletingId(null);
            ws.refresh();
          }}
        />
      )}
    </div>
  );
}

export default function Channels({ id }: { id?: string }) {
  const ws = useWorkspace();
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState(false);
  const [deleting, setDeleting] = useState(false);
  useEffect(() => onDoor(NEW_CHANNEL, () => setCreating(true)), []);

  // The list's copy of this channel follows every write of its record, here
  // or anywhere — so it is what the page draws, and the page's own read
  // (who the handle reaches) is asked again whenever that copy moves. Kept
  // under the channel's key: another channel's page never draws this one's
  // answer while its own is on its way.
  const listed = shownChannel(ws.channels, id, null);
  const { data, loading, reload, error, missing } = useAsync(
    (s) => (id ? api.channel(id, s) : Promise.resolve(null)),
    [id, channelStamp(listed)],
    { keep: readKey("channel", id) },
  );
  // A channel deleted — here or in another window — leaves the list on its
  // removed frame, the read above is asked again and answers that there is
  // none, the place is forgotten, and a person who had been on it is left at
  // the list (`useGonePlace`).
  useGonePlace(missing, id ? { name: "channel", id } : { name: "channels" });
  const channel: ChannelDef | null = shownChannel(ws.channels, id, data?.channel ?? null);
  const { inAudience } = useParticipants(channel);
  const working = (id && ws.working[id]) || [];
  const roster = rosterAgents(channel);
  const people = rosterHumans(channel);
  // Only a standing channel is editable. A DM has an audience, and an audience
  // is what a past message was encrypted to. Any channel but `general` may be
  // deleted (`channelDraftModel`).
  const editable = mayEditChannel(channel);
  const deletable = mayDeleteChannel(channel);

  const dialog = (
    <NewChannelDialog
      open={creating}
      onClose={() => setCreating(false)}
      onCreated={(newId) => {
        ws.refresh();
        navigate({ name: "channel", id: newId });
      }}
    />
  );

  if (!id) {
    return (
      <>
        <ChannelIndex onNew={() => setCreating(true)} />
        {dialog}
      </>
    );
  }

  // A channel that could not be read is said, never drawn as a conversation
  // with nobody in it: a post there would reach nothing.
  if (error && !channel) {
    return (
      <div className="p-6">
        <ErrorNote error={error} retry={reload} />
      </div>
    );
  }

  if (loading && !channel) {
    // The frame is known before the channel is — a header, then a timeline —
    // so the placeholder can promise it rather than spin in the middle.
    return (
      <div className="flex h-full min-h-0 flex-col">
        <div className="shrink-0 border-b border-border px-4 py-3">
          <Skeleton className="h-5 w-40" />
        </div>
        <SkeletonRows rows={6} className="p-3" />
      </div>
    );
  }

  return (
    <>
      <Conversation
        scope={id}
        kind="channel"
        channel={channel}
        placeholder={t("screens-channels-message-channel", { channel: channel?.name ?? "", flag: channel ? "yes" : "no" })}
        header={
          <ConversationHeader
            // The hash *is* the glyph — `ui/icons` maps a channel to it — so
            // the title carries the name and nothing else.
            icon={<ICON.channel size={18} aria-hidden className="text-text-dim" />}
            title={channel?.name ?? id.slice(-6)}
            subtitle={
              roster.length > 0 || people.length > 0 ? (
                // The roster is the header's job: it says who belongs in the
                // room. It is not who will reply — that is still whoever the
                // message addresses. This line is the glance; the `@handle`
                // beside it opens the same list in full, for the roster too
                // long to fit on one line.
                <span className="flex min-w-0 items-center gap-1">
                  {channel?.topic && <span className="min-w-0 truncate">{t("screens-channels-topic-dot", { topic: channel.topic })}</span>}
                  <RosterLine ids={roster} agents={ws.agents} humans={rosterHumans(channel)} />
                </span>
              ) : (
                (channel?.topic ??
                (inAudience.length
                  ? t("screens-channels-here", { a: inAudience.map((a) => a.name).join(", ") })
                  : t("screens-channels-everyone-workspace-can-read")))
              )
            }
            chips={
              <>
                <TagChips tags={channel?.tags ?? []} max={3} />
                {channel && roster.length > 0 && (
                  // The handle opens the roster rather than describing it in a
                  // `title`: who `@design` reaches is the question people get
                  // wrong, and an answer only a mouse can reach is no answer.
                  <Popover
                    label={t("screens-channels-who-reaches", { channel: channel.name })}
                    trigger={
                      <span className="anim inline-flex h-5 items-center gap-1 rounded-full border border-border px-2 text-2xs font-medium text-text-dim hover:border-text/35 hover:text-text">
                        <ICON.mention size={11} aria-hidden />
                        {channel.name}
                      </span>
                    }
                  >
                    <RosterPeek name={channel.name} ids={roster} agents={ws.agents} reach={reachOf(data)} />
                  </Popover>
                )}
                {working.length > 0 && <WorkingDot title={t("screens-channels-agent-writing-here")} />}
              </>
            }
            actions={
              <>
                {editable && (
                  <Button size="sm" onClick={() => setEditing(true)}>
                    <ICON.edit size={13} aria-hidden />{t("screens-channels-edit")}</Button>
                )}
                {deletable && channel && (
                  <MoreMenu
                    label={t("screens-channels-menu", { channel: channel.name })}
                    items={[{ label: t("screens-channels-delete-menu", { channel: channel.name }), icon: ICON.delete, danger: true, onSelect: () => setDeleting(true) }]}
                  />
                )}
              </>
            }
          />
        }
      />
      {deletable && channel && (
        <DeleteChannelDialog
          open={deleting}
          channel={channel}
          name={`#${channel.name}`}
          onClose={() => setDeleting(false)}
          onDeleted={() => {
            ws.refresh();
            navigate({ name: "channels" });
          }}
        />
      )}
      {editable && channel && (
        <EditChannelDialog
          open={editing}
          channel={channel}
          onClose={() => setEditing(false)}
          onSaved={() => {
            reload();
            // The sidebar and the index draw from the store, so a roster or a
            // topic changed here has to move there too.
            ws.refresh();
          }}
        />
      )}
      {dialog}
    </>
  );
}
