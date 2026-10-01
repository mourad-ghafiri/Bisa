/**
 * `#/conversations/:id` — the door a link to a conversation goes through
 * (13 — Conversations). A conversation's address is its origin's, so this
 * route reads the conversation and replaces itself with that address
 * (`routeOf`): the goal's tab, the designer's pane, the IDE's panel. Only a
 * conversation nothing owns — the node's, the workspace's — is drawn here,
 * on the one page of its own. There is no list: a conversation is reached
 * where it is about.
 */

import { useEffect } from "react";
import { navigate, replace } from "../router";
import { Button, EmptyState, ICON, Spinner } from "../ui";
import { ConversationThread } from "./_studio/ConversationThread";
import { routeOf } from "./_studio/conversationsModel.mjs";
import { useConversation } from "./_workbench/conversationsStore";
import { useGonePlace } from "../shell/useGonePlace";
import { t } from "../i18n/l10n.mjs";

export default function ConversationDoor({ id }: { id: string }) {
  const one = useConversation(id);
  const row = one.data;
  const door = row ? routeOf(row) : null;
  const owned = door !== null && door.route.name !== "conversation";
  // A conversation deleted while the app was closed: its place is forgotten,
  // and a person who had been on it is left at the home (`useGonePlace`).
  useGonePlace(one.missing, { name: "conversation", id });
  useEffect(() => {
    if (door && owned) replace(door.route as Parameters<typeof replace>[0], door.search ?? undefined);
  }, [door, owned]);
  if (one.error && !row) {
    return <EmptyState icon={ICON.dm} title={t("screens-conversation-door-no-such-conversation")} hint={one.error} action={<Button onClick={() => navigate({ name: "inbox" })}>{t("screens-conversation-door-back-inbox")}</Button>} />;
  }
  if (!row || owned) {
    return (
      <div className="p-6">
        <Spinner label={t("screens-conversation-door-opening")} />
      </div>
    );
  }
  return (
    <div className="h-full min-h-0">
      <ConversationThread row={row} onChanged={() => one.reload()} onGone={() => navigate({ name: "inbox" })} />
    </div>
  );
}
