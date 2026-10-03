/**
 * A channel or a direct channel on a workspace this node is a guest of
 * (14-collaboration): the same conversation surface as this node's own,
 * read and written through the host. The header says whose workspace it
 * is and what this person is there; the composer is text, mentions and
 * replies — no files, no artifacts, no agents to address — and says so.
 *
 * What is here is what the host relayed: the channels the role reaches,
 * the people the host's directory names. Nothing of this node's own
 * workspace — its agents, its members, its rosters — is offered.
 *
 * A screen that has no conversation to show is still a door: back to the
 * Inbox, or to Settings › People, where a membership is kept.
 */

import { useMemo } from "react";
import { api } from "../api";
import { navigate } from "../router";
import { settingsPath, settingsSearch } from "./_settings/settingsLink.mjs";
import { useWorkspace } from "../shell/useWorkspaceData";
import { hostName, hostedNameOf, hostedPhotoOf, hostedScreen, stateWords } from "../shell/hostedModel.mjs";
import { roleLabel } from "./_settings/peopleModel.mjs";
import { Avatar, Button, Chip, EmptyState, ICON, Skeleton, SkeletonRows } from "../ui";
import { audiencePrincipals } from "../types";
import { Conversation, ConversationHeader } from "./_studio/Conversation";
import { useAsync } from "./_work/useAsync";
import { t } from "../i18n/l10n.mjs";

export default function Hosted({ host, id, kind }: { host: string; id: string; kind: "channel" | "dm" }) {
  const ws = useWorkspace();
  const section = ws.hosted.find((s) => s.host.host.pubkey === host) ?? null;
  const rows = kind === "dm" ? (section?.dms ?? []) : (section?.channels ?? []);
  const channel = rows.find((r) => r.channel.id === id)?.channel ?? null;
  const members = useAsync(async (s) => (await api.hostedMembers(host, s)).members, [host]);
  const directory = useMemo(() => members.data ?? [], [members.data]);
  const hosted = useMemo(() => ({ host, directory }), [host, directory]);

  const screen = hostedScreen({ ready: ws.ready, section, channel });
  const toInbox = <Button onClick={() => navigate({ name: "inbox" })}>{t("screens-conversation-door-back-inbox")}</Button>;
  const toPeople = <Button onClick={() => navigate({ name: "settings" }, settingsSearch("people"))}>{settingsPath("people")}</Button>;
  if (screen === "reading") {
    // The workspace is not read yet: nothing is known of the membership, so nothing is said of it.
    return (
      <div className="flex h-full min-h-0 flex-col">
        <div className="shrink-0 border-b border-border px-4 py-3">
          <Skeleton className="h-5 w-40" />
        </div>
        <SkeletonRows rows={6} className="p-3" />
      </div>
    );
  }
  if (screen === "unknown-host" || !section) {
    return (
      <div className="p-6">
        <EmptyState icon={ICON.members} title={t("screens-hosted-not-member-workspace")} hint={t("screens-hosted-section-gone-from-sidebar-left-never")} action={toInbox} />
      </div>
    );
  }
  if (screen === "not-member") {
    return (
      <div className="p-6">
        <EmptyState icon={ICON.members} title={`${hostName(section.host)}: ${stateWords(section.host) ?? t("screens-hosted-not-a-member")}`} hint={t("screens-hosted-what-already-received-stays-machine-nothing")} action={toPeople} />
      </div>
    );
  }
  if (screen === "unreached" || !channel) {
    return (
      <div className="p-6">
        <EmptyState icon={kind === "dm" ? ICON.dm : ICON.channel} title={t("screens-hosted-do-not-reach-channel")} hint={t("screens-hosted-has-not-put-gone", { host: hostName(section.host) })} action={toInbox} />
      </div>
    );
  }

  const others = kind === "dm" ? audiencePrincipals(channel).filter((p) => p !== ws.me) : [];
  const title = kind === "dm" ? others.map((p) => hostedNameOf(directory, p)).join(", ") || channel.name : channel.name;
  const you = t("screens-hosted-words", { role: roleLabel(section.host.role).toLowerCase(), host: hostName(section.host) });

  return (
    <Conversation
      scope={id}
      kind={kind}
      channel={channel}
      hosted={hosted}
      placeholder={kind === "dm" ? t("screens-hosted-write-message") : t("screens-hosted-message", { channel: channel.name, host: hostName(section.host) })}
      emptyTitle={t("screens-hosted-no-messages-yet")}
      emptyHint={t("screens-hosted-say-something-goes-host-which-relays")}
      header={
        <ConversationHeader
          icon={
            kind === "dm" && others.length === 1 ? (
              <Avatar id={others[0]!} name={hostedNameOf(directory, others[0]!)} photo={hostedPhotoOf(directory, others[0]!)} size={26} />
            ) : (
              <ICON.channel size={18} aria-hidden className="text-text-dim" />
            )
          }
          title={title}
          subtitle={
            <span className="flex min-w-0 items-center gap-1">
              <span className="shrink-0">{t("screens-hosted-hosted-by", { host: hostName(section.host) })}</span>
              <span className="truncate">{channel.topic ?? you}</span>
            </span>
          }
          chips={
            <>
              <Chip icon={ICON.members}>
                {roleLabel(section.host.role)}
              </Chip>
              <Chip title={t("screens-hosted-text-mentions-replies-reach-host-files")}>{t("screens-hosted-text-only")}</Chip>
              {/* The host's people did not answer: names read as keys, and the header says why. */}
              {members.error && !members.data && (
                <Chip tone="warn" title={members.error}>
                  {t("screens-hosted-people-not-read")}
                </Chip>
              )}
            </>
          }
        />
      }
    />
  );
}
