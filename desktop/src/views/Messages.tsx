/**
 * Direct messages — with people, and with your agents.
 *
 * An agent is a principal, so talking to one is the same act as talking to a
 * colleague: the same surface, the same composer. The header says plainly
 * what it runs on and who it answers, because an agent that stays silent for
 * a policy reason must not look broken.
 */

import { useEffect, useMemo, useState } from "react";
import { api } from "../api";
import { href, navigate } from "../router";
import { useWorkspace } from "../shell/useWorkspaceData";
import { placeOf, useViewState } from "../shell/viewMemoryStore";
import { textValue } from "../shell/viewValuesModel.mjs";
import { settingsSearch } from "./_settings/settingsLink.mjs";
import { NEW_MESSAGE, onDoor } from "../shell/shortcuts";
import { BrowserDoor } from "../shell/BrowserDoor";
import {
  AgentPicker,
  Avatar,
  Button,
  Chip,
  CountBadge,
  Dialog,
  EmptyState,
  ErrorNote,
  ICON,
  RelativeTime,
  Skeleton,
  SkeletonRows,
  TextInput,
  useToast,
  WorkingDot,
} from "../ui";
import { dmHead, dmLabel, filterDms, latestWords, sortDms } from "./_studio/channelListModel.mjs";
import type { AgentDef, ChannelDef } from "../types";
import { audiencePrincipals, planSummary } from "../types";
import { Conversation, ConversationHeader } from "./_studio/Conversation";
import { useDirectMessageCandidates } from "./_studio/participants";
import { attempt, useAsync } from "./_work/useAsync";
import { useGonePlace } from "../shell/useGonePlace";
import { t } from "../i18n/l10n.mjs";

/**
 * Who to open a conversation with.
 *
 * It matched on names alone — not descriptions, not tags — which is the
 * narrowest of the four predicates the app used to carry, on the one screen
 * where you are most likely to be looking for a *kind* of agent rather than a
 * name you already know. It now uses the same rows and the same search as
 * every other agent list.
 */
function NewMessageDialog({
  open,
  onClose,
  onOpened,
}: {
  open: boolean;
  onClose: () => void;
  onOpened: (id: string) => void;
}) {
  const toast = useToast();
  const candidates = useDirectMessageCandidates();
  const [picked, setPicked] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (open) setPicked([]);
  }, [open]);

  const start = async () => {
    if (!picked.length || busy) return;
    setBusy(true);
    const ok = await attempt(async () => {
      const { channel } = await api.openDm(picked);
      onOpened(channel.id);
    }, toast.error);
    setBusy(false);
    if (ok) onClose();
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={t("screens-messages-new-message")}
      description={t("screens-messages-pick-people-agents-both-opening-same")}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>{t("screens-channels-cancel")}</Button>
          <Button variant="primary" disabled={!picked.length || busy} onClick={() => void start()}>
            {busy ? t("screens-messages-opening") : t("screens-messages-message-count", { n: picked.length })}
          </Button>
        </>
      }
    >
      <AgentPicker
        candidates={candidates}
        value={picked}
        onChange={setPicked}
        autoFocus
        label={t("screens-messages-people-agents")}
        placeholder={t("screens-messages-search-people-agents-name-role-tag")}
        listClassName="max-h-72"
        emptyTitle={t("screens-messages-nobody-message-yet")}
        emptyHint={t("screens-messages-install-agent-from-catalog-invite-someone")}
        emptyAction={
          <Button
            variant="ghost"
            onClick={() =>
              navigate({ name: "settings" }, settingsSearch("catalog-agent"))
            }
          >{t("screens-messages-open-catalog")}</Button>
        }
      />
    </Dialog>
  );
}

/** Where the list keeps its memory: the page's place, not a conversation's. */
const LIST_PLACE = placeOf({ name: "messages" });

/**
 * The Direct messages page: every direct channel as a row — who is in it
 * (the others' names, and the first one's face; never the stored name, which
 * is hex prefixes), an agent's harness as a dim word, when it last moved and
 * who said what last (`latest`, kept live by the bus), a working dot and the
 * **live** unread count. A recency list, the sidebar's order
 * (`channelListModel.sortDms`); one toolbar: the words, the count, *New
 * message*.
 */
function DmIndex({ onNew }: { onNew: () => void }) {
  const ws = useWorkspace();
  // The words typed are the page's memory: it comes back narrowed as it was left.
  const [search, setSearch] = useViewState(LIST_PLACE, "search", "", textValue);
  const entries = ws.dms;
  const { me, nameOf } = ws;
  const shown = useMemo(() => sortDms(filterDms(entries, search, me, nameOf)), [entries, search, me, nameOf]);

  return (
    <div className="flex h-full flex-col">
      <div className="flex flex-wrap items-center gap-2 border-b border-border px-6 py-2">
        <span className="relative">
          <ICON.search size={12} aria-hidden className="pointer-events-none absolute top-1/2 left-2 -translate-y-1/2 text-text-dim" />
          <TextInput value={search} placeholder={t("screens-messages-search")} aria-label={t("screens-messages-search")} className="h-7 w-56 py-0 pl-7" onChange={(e) => setSearch(e.target.value)} />
        </span>
        <span className="tnum text-2xs text-text-dim">{t("screens-goals-words", { shown: shown.length, rows: entries.length })}</span>
        {search.trim() !== "" && (
          <Button size="sm" variant="ghost" onClick={() => setSearch("")}>{t("screens-agents-clear-filters")}</Button>
        )}
        <Button size="sm" variant="primary" className="ml-auto" onClick={onNew}>
          <ICON.add size={12} aria-hidden />{t("screens-messages-new-message")}</Button>
      </div>
      <div data-scroll-keep="list" className="min-h-0 flex-1 overflow-y-auto px-6 py-4">
        {entries.length === 0 ? (
          <EmptyState
            icon={ICON.dm}
            title={t("screens-messages-no-direct-messages")}
            hint={t("screens-messages-message-teammate-one-agents-agent-answers")}
            action={<Button variant="primary" onClick={onNew}>{t("screens-messages-new-message")}</Button>}
          />
        ) : shown.length === 0 ? (
          <EmptyState
            icon={ICON.filter}
            title={t("screens-agents-nothing-matches")}
            hint={t("screens-messages-nothing-matches-name")}
            action={<Button variant="ghost" onClick={() => setSearch("")}>{t("screens-agents-clear-filters")}</Button>}
          />
        ) : (
          <div className="flex flex-col">
            {shown.map(({ channel, latest }) => {
              const head = dmHead(channel, me);
              const agent = ws.agentByPubkey(head);
              const label = dmLabel(channel, me, nameOf);
              const unread = ws.unread[channel.id] ?? 0;
              const busy = (ws.working[channel.id] ?? []).length > 0;
              const said = latestWords(latest, nameOf);
              return (
                <a key={channel.id} href={href({ name: "dm", id: channel.id })} className="anim flex min-w-0 items-center gap-3 rounded-card px-3 py-2 hover:bg-surface-2">
                  <Avatar id={head} name={agent?.name ?? nameOf(head)} photo={ws.photoOf(head)} size={28} />
                  <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                    <span className="flex min-w-0 items-center gap-2">
                      <span className={`truncate text-sm text-text ${unread > 0 ? "font-semibold" : "font-medium"}`}>{label}</span>
                      {agent && <span className="hidden shrink-0 text-2xs text-text-dim sm:inline">{agent.harness}</span>}
                      {latest && <RelativeTime at={latest.at} className="tnum ml-auto shrink-0 text-2xs text-text-dim" />}
                    </span>
                    <span className="truncate text-2xs text-text-dim">{said ?? t("screens-messages-nothing-said-yet")}</span>
                  </span>
                  <span className="flex shrink-0 items-center gap-1.5">
                    {busy && <WorkingDot title={t("shell-sidebar-is-writing", { name: agent?.name ?? t("shell-sidebar-an-agent") })} />}
                    <CountBadge count={unread} />
                  </span>
                </a>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}

export default function Messages({ id }: { id?: string }) {
  const ws = useWorkspace();
  const [creating, setCreating] = useState(false);
  useEffect(() => onDoor(NEW_MESSAGE, () => setCreating(true)), []);

  const { data, loading, reload, error, missing } = useAsync(
    (s) => (id ? api.channel(id, s) : Promise.resolve(null)),
    [id],
  );
  // A direct channel that is gone raises no fact on the bus: the node
  // answers that there is none, and the place is forgotten (`useGonePlace`).
  useGonePlace(missing, id ? { name: "dm", id } : { name: "messages" });
  const channel: ChannelDef | null = data?.channel ?? null;

  const { agents, humans } = useMemo(() => {
    const audience = audiencePrincipals(channel);
    const byKey = new Map(ws.agents.map((a) => [a.pubkey, a]));
    const found: AgentDef[] = [];
    const people: string[] = [];
    for (const p of audience) {
      if (p === ws.me) continue;
      const a = byKey.get(p);
      if (a) found.push(a);
      else people.push(p);
    }
    return { agents: found, humans: people };
  }, [channel, ws.agents, ws.me]);

  const soloAgent = agents.length === 1 && humans.length === 0 ? agents[0]! : null;
  const working = (id && ws.working[id]) || [];

  const dialog = (
    <NewMessageDialog
      open={creating}
      onClose={() => setCreating(false)}
      onOpened={(newId) => {
        ws.refresh();
        navigate({ name: "dm", id: newId });
      }}
    />
  );

  if (!id) {
    return (
      <>
        <DmIndex onNew={() => setCreating(true)} />
        {dialog}
      </>
    );
  }

  // A direct channel that could not be read is said, never drawn as one with nobody in it.
  if (error && !channel) {
    return (
      <div className="p-6">
        <ErrorNote error={error} retry={reload} />
      </div>
    );
  }

  if (loading && !channel) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        <div className="shrink-0 border-b border-border px-4 py-3">
          <Skeleton className="h-5 w-44" />
        </div>
        <SkeletonRows rows={6} className="p-3" />
      </div>
    );
  }

  const title = soloAgent
    ? soloAgent.name
    : [...agents.map((a) => a.name), ...humans.map((h) => ws.nameOf(h))].join(", ") ||
      channel?.name ||
      t("screens-messages-direct-message");

  return (
    <>
      <Conversation
        scope={id}
        kind="dm"
        channel={channel}
        placeholder={soloAgent ? t("screens-messages-message", { soloAgent: soloAgent.name }) : t("screens-hosted-write-message")}
        emptyTitle={soloAgent ? t("screens-messages-talk", { soloAgent: soloAgent.name }) : t("screens-hosted-no-messages-yet")}
        emptyHint={
          soloAgent
            ? t("screens-messages-ask-something-will-do-work-here", { harness: soloAgent.harness, models: planSummary(soloAgent.models) })
            : undefined
        }
        header={
          <ConversationHeader
            icon={
              // One agent means the conversation *has* a face, and a face is
              // the strongest identifier there is. A group has none, so it
              // wears the concept's glyph instead of an identicon of an id
              // nobody recognises.
              soloAgent ? (
                <Avatar
                  id={soloAgent.pubkey}
                  name={soloAgent.name}
                  photo={soloAgent.photo}
                  size={26}
                />
              ) : (
                <ICON.dm size={18} aria-hidden className="text-text-dim" />
              )
            }
            title={title}
            subtitle={
              soloAgent
                ? `${soloAgent.harness} · ${planSummary(soloAgent.models)} · ${
                    soloAgent.respond === "owner_only"
                      ? t("screens-messages-answers-you-only")
                      : t("screens-messages-answers-any-member")
                  }`
                : t("screens-messages-other-others-private-group", { channel: Math.max(audiencePrincipals(channel).length - 1, 0), channel2: audiencePrincipals(channel).length - 1 })
            }
            chips={
              <>
                {working.length > 0 && <WorkingDot title={t("screens-channels-agent-writing-here")} />}
                {agents.length > 0 && !soloAgent && (
                  <Chip tone="quiet" icon={ICON.agent}>{t("screens-messages-agents", { agents: agents.length })}</Chip>
                )}
              </>
            }
            actions={
              <>
                <BrowserDoor home={id ? { scope: "dm", id } : null} />
                {soloAgent && (
                  <Button asChild size="sm">
                    <a href={href({ name: "agent", id: soloAgent.id })}>
                      <ICON.open size={13} aria-hidden />{t("screens-messages-open-agent")}</a>
                  </Button>
                )}
              </>
            }
          />
        }
      />
      {dialog}
    </>
  );
}
