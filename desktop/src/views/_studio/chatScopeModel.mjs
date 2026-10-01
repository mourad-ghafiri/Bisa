/**
 * A conversation surface's place, as facts (ide/09, ide/18): the key its
 * composer's chips are kept under — `<kind>:<id>`, the shape the IDE's
 * `workstream:<id>` has, so one store keeps every thread's tray — and the
 * words a tray outside the IDE says when it attaches to it, or cannot.
 * Plain `.mjs`, so `node --test` reads it.
 */

import { t } from "../../i18n/l10n.mjs";

/**
 * The key a conversation's chips are kept under.
 * @param {string} kind `goal` · `channel` · `dm` · `conversation`
 * @param {string} id
 */
export function chatKey(kind, id) {
  return `${kind}:${id}`;
}

/**
 * The words on the tray's *Attach to the message* — what the chips join.
 * @param {{kind: string, id: string} | null} target the conversation on screen
 */
export function attachWords(target) {
  if (!target) return { enabled: false, label: t("studio-chat-scope-attach-message"), hint: t("studio-chat-scope-open-goal-channel-message-attach-these") };
  const where = target.kind === "goal" ? t("studio-chat-scope-goal-s-message") : target.kind === "channel" ? t("studio-chat-scope-channel-s-message") : target.kind === "dm" ? t("studio-chat-scope-message") : t("studio-chat-scope-conversation-s-message");
  return { enabled: true, label: t("studio-chat-scope-attach-message"), hint: t("studio-chat-scope-put-chips-beside-pane-nothing-sent", { where }) };
}

/**
 * The toast after an attach.
 * @param {number} count
 */
export function attachedWords(count) {
  return count === 1 ? t("studio-chat-scope-attached-write-message-beside-page") : t("studio-chat-scope-attached-annotations-write-message-beside-page", { count });
}
