/**
 * The sidebar: the reason this reads as a workspace.
 *
 * It never unmounts. Every channel and every direct message — with people
 * *and* agents — is visible at once, with its unread, while you are reading
 * something else. Choosing one is one click, not "pick a section, then pick
 * a thing". Goals are deliberately *not* a section here: the Goals
 * destination is the one live list of runs, and mirroring it in the nav would
 * be a second lifecycle by another name. Nor are conversations (13): a
 * conversation is reached where it is about — a goal's Conversation tab, a
 * workflow's Agent pane, the IDE's Agent pane — never from a list of all.
 *
 * The primary nav lists seven destinations, in the file's order what concerns
 * you, who does the work and who is on it (Agents and Teams — they were one
 * row called "Agents & Teams", a decision the reader had to undo every time),
 * where it lives, the shapes it runs, the goals it serves, then Pulse: what
 * happened. The order a person sees is theirs (`navOrderStore.ts`).
 *
 * Counts come from {@link useWorkspace} and only from there. The Inbox carries
 * two counts, never one sum (`sidebarModel.inboxBadge`): what needs the
 * person, in the accent — the one colour that means *your attention* — and
 * beside it what is merely unread, neutral. It is the *only* place those
 * appear in the nav: repeating them on Goals would show the same attention
 * twice and make clearing one look like it failed to clear the other. Rows
 * carry their own unread because that is a different fact, and it is neutral
 * too: an unread channel is news, not a summons.
 *
 * While the node is away, or a list could not be read, a section offers no
 * door to fill it (`workspaceLoadModel.listUnread`): an empty list that was
 * never read is not an empty list, and a create made then would fail.
 *
 * The destinations stand in **the person's order**: a row dragged to a new
 * place — here, or an icon in the rail — is remembered per viewer
 * (`navOrderStore`, one store for both sidebars and the palette's *Go to*),
 * so the sidebar opens tomorrow as it was left. The rows are one
 * `SortableList` (`ui/dnd`): a click still opens, a drag past the kit's
 * distance moves; Space lifts a focused row, the arrows move it, Space
 * drops it. *Reset order* is under Settings › Appearance.
 *
 * Collapsed, the sidebar is its rail (`SidebarRail`): the same doors as icons,
 * the Inbox count in its icon's corner, so the count is never out of sight.
 */

import { href, navigate, section, useRoute } from "../router";
import { Avatar, CountBadge, ICON, SortableList, WorkingDot, navRowDrag, useArrivals } from "../ui";
import type { SortableHandle } from "../ui";
import type { ReactNode } from "react";
import type { NavEntry } from "./nav";
import { placeNav, usePrimaryNav } from "./navOrderStore";
import { useSectionHref } from "./sectionDoor";
import { AddButton, EmptyDoor, SidebarRow, SidebarSection, onArrowKeys } from "./SidebarSection";
import { useWorkspace } from "./useWorkspaceData";
import { audiencePrincipals } from "../types";
import { NEW_CHANNEL, NEW_MESSAGE, fire } from "./shortcuts";
import { hostedNameOf, isMember, orderSections, sectionTitle, stateWords, unreadOf } from "./hostedModel.mjs";
import { SidebarRail } from "./SidebarRail";
import { inboxBadge } from "./sidebarModel.mjs";
import { listUnread } from "./workspaceLoadModel.mjs";
import { settingsPath, settingsSearch } from "../views/_settings/settingsLink.mjs";
import { sortChannels, sortDms } from "../views/_studio/channelListModel.mjs";
import type { SidebarMode } from "./sidebarModel.mjs";
import { t } from "../i18n/l10n.mjs";

/**
 * One destination's row. Its link is where the person was in the section —
 * the goal, the workflow, the conversation they left, on the tab they left
 * it — and the section's list from inside it (`sectionDoor.ts`).
 */
function DestinationRow({ entry, active, handle, trailing }: { entry: NavEntry; active: boolean; handle?: SortableHandle; trailing?: ReactNode }) {
  const to = useSectionHref(entry.route);
  return <SidebarRow href={to} active={active} icon={entry.icon} label={entry.label} prominent handle={handle} trailing={trailing} />;
}

export function Sidebar({ mode }: { mode: SidebarMode }) {
  const route = useRoute();
  const active = section(route);
  const here = "id" in route ? route.id : null;
  const ws = useWorkspace();
  const inbox = inboxBadge(ws.inbox);
  // Something new waiting on you: the owed count arrives once, never on a re-read — and never for what is merely unread.
  const arrivals = useArrivals(inbox.needs);
  const nav = usePrimaryNav();

  if (mode === "collapsed") return <SidebarRail />;

  return (
    <div data-pane className="flex h-full min-h-0 flex-col bg-surface">
      <nav aria-label={t("shell-sidebar-workspace")} className="min-h-0 flex-1 overflow-y-auto px-2 pt-2.5 pb-3">
        <div className="flex flex-col gap-0.5" onKeyDown={onArrowKeys}>
          <SortableList<{ id: string; entry: NavEntry }>
            items={nav.map((entry) => ({ id: entry.key, entry }))}
            direction="vertical"
            dragData={({ entry }) => navRowDrag(entry.key, entry.label, entry.glyph)}
            onReorder={placeNav}
          >
            {({ entry }, handle) => (
              <DestinationRow
                entry={entry}
                active={active === entry.key}
                handle={handle}
                trailing={
                  entry.key === "inbox" && (inbox.needs > 0 || inbox.unread > 0) ? (
                    <>
                      {inbox.needs > 0 && (
                        <span key={arrivals} className={arrivals > 0 ? "motion-pop inline-flex" : "inline-flex"}>
                          <CountBadge count={inbox.needs} tone="accent" title={inbox.needsTitle ?? undefined} />
                        </span>
                      )}
                      <CountBadge count={inbox.unread} tone="neutral" title={inbox.unreadTitle ?? undefined} />
                    </>
                  ) : undefined
                }
              />
            )}
          </SortableList>
        </div>

        <SidebarSection
          id="channels"
          title={t("shell-sidebar-channels")}
          count={ws.channels.length}
          action={<AddButton label={t("shell-omnibox-new-channel")} icon={ICON.add} onClick={() => fire(NEW_CHANNEL)} />}
          empty={!ws.ready || listUnread(ws.degraded, ws.offline, "channels") ? undefined : <EmptyDoor onClick={() => fire(NEW_CHANNEL)} label={t("shell-sidebar-create-channel")} />}
        >
          {/* One order here and on the Channels page: `general` first, then creation — a message never reshuffles the rooms. */}
          {sortChannels(ws.channels).map(({ channel }) => {
            const unread = ws.unread[channel.id] ?? 0;
            const busy = (ws.working[channel.id] ?? []).length > 0;
            return (
              <SidebarRow
                key={channel.id}
                href={href({ name: "channel", id: channel.id })}
                active={route.name === "channel" && here === channel.id}
                icon={ICON.channel}
                label={channel.name}
                title={channel.topic ?? channel.name}
                trailing={
                  <>
                    {busy && <WorkingDot title={t("shell-sidebar-agent-writing-here")} />}
                    <CountBadge count={unread} tone="neutral" />
                  </>
                }
              />
            );
          })}
        </SidebarSection>

        <SidebarSection
          id="dms"
          title={t("shell-sidebar-direct-messages")}
          count={ws.dms.length}
          action={
            <AddButton label={t("shell-omnibox-new-message")} icon={ICON.add} onClick={() => fire(NEW_MESSAGE)} />
          }
          empty={!ws.ready || listUnread(ws.degraded, ws.offline, "dms") ? undefined : <EmptyDoor onClick={() => fire(NEW_MESSAGE)} label={t("shell-sidebar-message-someone-agent")} />}
        >
          {/* A recency list, here and on the Messages page: the direct channel that moved last first. */}
          {sortDms(ws.dms).map(({ channel }) => {
            const others = audiencePrincipals(channel).filter((p) => p !== ws.me);
            const agent = others.map((p) => ws.agentByPubkey(p)).find(Boolean);
            const label = others.length ? others.map((p) => ws.nameOf(p)).join(", ") : t("shell-omnibox-just");
            const unread = ws.unread[channel.id] ?? 0;
            const busy = (ws.working[channel.id] ?? []).length > 0;
            const head = others[0] ?? channel.id;
            return (
              <SidebarRow
                key={channel.id}
                href={href({ name: "dm", id: channel.id })}
                active={route.name === "dm" && here === channel.id}
                leading={
                  <Avatar
                    id={head}
                    name={agent?.name ?? ws.nameOf(head)}
                    photo={ws.photoOf(head)}
                    size={18}
                  />
                }
                label={label}
                title={agent ? `${agent.name} · ${agent.harness}` : label}
                trailing={
                  <>
                    {busy && <WorkingDot title={t("shell-sidebar-is-writing", { name: agent?.name ?? t("shell-sidebar-an-agent") })} />}
                    <CountBadge count={unread} tone="neutral" />
                  </>
                }
              />
            );
          })}
        </SidebarSection>

        {/* The workspaces this node is a guest of (14-collaboration): one
            section per host, its channels and direct channels as the host
            relays them. No add button — a channel there is the host's to
            make; a direct message is asked for from a hosted channel. An
            empty one is a door to where the membership is kept, with its
            standing beneath. */}
        {orderSections(ws.hosted).map((section) => {
          const key = section.host.host.pubkey;
          const state = stateWords(section.host);
          return (
            <SidebarSection
              key={key}
              id={`hosted:${key}`}
              title={sectionTitle(section.host)}
              count={isMember(section.host) ? unreadOf(section) : undefined}
              empty={
                <EmptyDoor
                  icon={ICON.members}
                  onClick={() => navigate({ name: "settings" }, settingsSearch("people"))}
                  label={settingsPath("people")}
                  note={state ? t("shell-sidebar-hosted-membership", { state }) : t("shell-sidebar-channel-reaches-yet-ask-host-put")}
                />
              }
            >
              {isMember(section.host) &&
                section.channels.map(({ channel, unread_count }) => (
                  <SidebarRow
                    key={channel.id}
                    href={href({ name: "hosted_channel", host: key, id: channel.id })}
                    active={route.name === "hosted_channel" && route.host === key && here === channel.id}
                    icon={ICON.channel}
                    label={channel.name}
                    title={channel.topic ?? channel.name}
                    trailing={<CountBadge count={unread_count} tone="neutral" />}
                  />
                ))}
              {isMember(section.host) &&
                section.dms.map(({ channel, unread_count }) => {
                  const others = audiencePrincipals(channel).filter((p) => p !== ws.me);
                  const label = others.length ? others.map((p) => hostedNameOf([], p)).join(", ") : t("shell-omnibox-just");
                  return (
                    <SidebarRow
                      key={channel.id}
                      href={href({ name: "hosted_dm", host: key, id: channel.id })}
                      active={route.name === "hosted_dm" && route.host === key && here === channel.id}
                      leading={<Avatar id={others[0] ?? channel.id} name={label} size={18} />}
                      label={label}
                      title={label}
                      trailing={<CountBadge count={unread_count} tone="neutral" />}
                    />
                  );
                })}
            </SidebarSection>
          );
        })}
      </nav>

      <SidebarFooter />
    </div>
  );
}

/**
 * The offline notice, pinned below the scroll: "the node is unreachable" is
 * the one thing that explains every empty list above it at once. Nothing
 * else lives here — the person's own destinations (Identity, Settings,
 * About) are the top chrome's profile menu, out of the work's way — so with
 * the node reachable the sidebar ends with its last section. A status: a
 * screen reader hears it once, when the node goes away.
 */
function SidebarFooter() {
  const ws = useWorkspace();
  if (!ws.offline) return null;
  return (
    <div className="shrink-0 border-t border-hairline p-2">
      <div role="status" className="rounded-control border border-border bg-danger-soft px-2 py-1.5">
        <p className="flex items-center gap-1.5 text-2xs font-medium text-danger">
          <ICON.warn size={11} aria-hidden />{t("shell-sidebar-node-unreachable")}</p>
        <p className="mt-0.5 text-2xs text-danger">{ws.offline}</p>
      </div>
    </div>
  );
}
