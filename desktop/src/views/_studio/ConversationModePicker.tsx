/**
 * The conversation's mode, a dropdown in the composer's foot (ide/09) —
 * shown only for a conversation about a checkout. The chip names the mode;
 * the menu offers the three with their meaning, plan greyed with its reason
 * where the harness has no tool guard. `Shift+Tab` in the box still cycles
 * it (`nextMode`) — the caller wires that through `onComposerKeyDownCapture`.
 */

import type { ConversationMode } from "../../types";
import { ChoiceMenu, ICON } from "../../ui";
import { MODE_ICON, MODE_LABEL, MODE_MEANING, modeChoices } from "./conversationModeModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export function ConversationModePicker({
  mode,
  onChange,
  toolGuard,
  disabled = false,
}: {
  mode: ConversationMode;
  onChange: (mode: ConversationMode) => void;
  toolGuard: boolean;
  disabled?: boolean;
}) {
  const Glyph = ICON[MODE_ICON[mode]];
  const choices = modeChoices(toolGuard).map((c) => ({ ...c, icon: ICON[c.icon] }));
  return (
    <ChoiceMenu
      label={t("studio-conversation-mode-picker-conversation-mode", { mode: MODE_LABEL[mode] })}
      value={mode}
      onChange={onChange}
      choices={choices}
      disabled={disabled}
      trigger={
        // A menu trigger wears a native title, never a hover card (crates/desktop.md, the kit).
        <span title={MODE_MEANING[mode]} className="anim inline-flex h-6 items-center gap-1 rounded-control px-1.5 text-2xs text-text-dim hover:bg-surface-2 hover:text-text">
          <Glyph size={12} aria-hidden />
          {MODE_LABEL[mode]}
          <ICON.collapsed size={10} aria-hidden />
        </span>
      }
    />
  );
}
