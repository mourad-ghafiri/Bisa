/**
 * What an owner says while its thing has no conversation yet (13 —
 * Conversations): the owner's one sentence, a hint that says what a
 * conversation here is about, and one button that starts one — untitled,
 * opened at once. Every surface — the IDE's Agent pane and Agent mode, the
 * designer's Agent pane, the drawers beside a note or a drawing — says it
 * with this, through the one painter (`ConversationSurface`), and the button
 * says what the list's foot says: **New conversation**, one label for one act.
 */

import { Button, EmptyState, ICON } from "../../ui";
import { t } from "../../i18n/l10n.mjs";

export function NoConversation({ title, hint, onNew }: { title: string; hint?: string; onNew: () => void }) {
  return (
    <EmptyState
      icon={ICON.dm}
      title={title}
      hint={hint ?? t("studio-no-conversation-saved-listed-yours")}
      action={
        <Button variant="primary" size="sm" onClick={onNew}>
          <ICON.add size={12} aria-hidden />{t("studio-conversations-bar-new-conversation")}</Button>
      }
    />
  );
}
