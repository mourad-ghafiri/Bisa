/**
 * The control over every thinking block in a timeline (13 — Conversations
 * §The reply streams): a dropdown at the right of the status line — *Thinking:
 * Auto ▾* — offering *auto · shown · hidden* with a meaning each, the shape
 * of `ConversationModePicker`. The words are `liveTurnModel.mjs`'s; the
 * choice is `thinkingStore`'s.
 */

import { ChoiceMenu, ICON } from "../../ui";
import { thinkingChoices, thinkingTriggerWords } from "./liveTurnModel.mjs";
import type { ThinkingMode } from "./liveTurnModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export function ThinkingPicker({ mode, onChange }: { mode: ThinkingMode; onChange: (mode: ThinkingMode) => void }) {
  const words = thinkingTriggerWords(mode);
  const Glyph = ICON[words.icon];
  const choices = thinkingChoices().map((c) => ({ ...c, icon: ICON[c.icon] }));
  return (
    <ChoiceMenu
      label={t("studio-thinking-picker-thinking", { words: words.label })}
      value={mode}
      onChange={onChange}
      choices={choices}
      align="end"
      trigger={
        // A menu trigger wears a native title, never a hover card (crates/desktop.md, the kit).
        <span title={words.title} className="anim inline-flex h-5 items-center gap-1 rounded-control px-1 text-2xs text-text-dim hover:bg-surface-2 hover:text-text">
          <Glyph size={11} aria-hidden />
          {words.label}
          <ICON.collapsed size={10} aria-hidden />
        </span>
      }
    />
  );
}
