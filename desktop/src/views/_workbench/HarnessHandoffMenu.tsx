/**
 * The tray's third door (ide/18 §Annotating a page, ide/06 §Start words):
 * **To a harness** — the same request the agent would get, typed into a
 * harness of the person's own. The menu is one list, the launcher's way: a
 * new terminal per harness the node can run (a harness not installed stays
 * listed, disabled, so the menu does not pretend the app lacks it), then the
 * workstream's live harness tabs with what each is doing — a tab that is
 * waiting on the person is held, with the reason as its second line, since
 * a dialog typed over is a dialog answered by accident. Every row's words,
 * order and door are `harnessHandoffModel.mjs`'s; this file draws and asks.
 *
 * A new terminal opens with the words to type once its harness listens; a
 * live one takes them the same way — typed once its harness listens and is
 * quiet, held while it works — and is shown. Neither empties the tray: a
 * tab has not typed yet, and a terminal has no keep or undo — the toast
 * says the rows stay until *Clear*.
 */

import { useMemo } from "react";
import { ChoiceMenu, ICON, Tooltip, cn, useToast } from "../../ui";
import type { MenuChoice } from "../../ui/ChoiceMenu";
import { harnessMark } from "../../ui/harnessMarks";
import { useSessions } from "../../shell/sessionsStore";
import { useLaunchableHarnesses } from "../../shell/useHarnesses";
import { canOpenTerminal, handToTerminalTab, openTerminalIn, useTerminals } from "../../shell/useTerminals";
import { failureText } from "../../ui/failure";
import { handOffTarget, harnessDoor, harnessTargets, sentToWords, targetRows } from "./harnessHandoffModel.mjs";
import type { HarnessTarget } from "./harnessHandoffModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The choice that is no choice: the trigger's resting value. */
const NONE = "none";

export function HarnessHandoffMenu({
  wid,
  noun,
  words,
  disabled = false,
}: {
  wid: string;
  /** The rows' noun, plural: *annotations*. */
  noun: string;
  /** The request as the harness should read it, composed when chosen. */
  words: () => string;
  disabled?: boolean;
}) {
  const toast = useToast();
  const harnesses = useLaunchableHarnesses();
  const sessions = useSessions();
  const { sessions: terminals } = useTerminals();
  const labels = useMemo(() => Object.fromEntries(harnesses.map((h) => [h.id, h.label])), [harnesses]);
  const targets = useMemo(() => harnessTargets(harnesses, sessions, terminals, wid) ?? [], [harnesses, sessions, terminals, wid]);
  const rows = useMemo(() => targetRows(targets, labels), [targets, labels]);
  if (!canOpenTerminal()) return null;

  const byId = new Map(rows.map((r) => [r.id, r.target] as [string, HarnessTarget]));
  const choices: MenuChoice<string>[] = rows.map((r) => ({
    id: r.id,
    label: r.label,
    icon: harnessMark(r.harness),
    disabled: r.disabled,
    hint: r.disabled ? (r.hint ?? undefined) : undefined,
    description: !r.disabled && r.hint ? r.hint : undefined,
    separatorBefore: r.separatorBefore,
  }));

  const choose = (id: string) => {
    const target = byId.get(id);
    if (!target) return;
    // The door is read again on the rows as they stand now: a tab that was
    // listening a moment ago may be waiting by the time the menu closes.
    const door = harnessDoor(target);
    if (!door.ok) {
      if (door.words) toast.info(door.words);
      return;
    }
    if (target.kind === "new") {
      openTerminalIn(handOffTarget(wid, target.harness, words()));
      toast.ok(sentToWords(target, labels, noun));
      return;
    }
    try {
      handToTerminalTab(target.key, words());
    } catch (e) {
      toast.error(failureText("view", "the request could not be handed to the terminal", e)); // for the log
      return;
    }
    toast.ok(sentToWords(target, labels, noun));
  };

  return (
    <Tooltip label={t("workbench-context-tray-type-noun-into-harness-terminal", { noun })}>
      <span className="inline-flex">
        <ChoiceMenu
          value={NONE}
          onChange={choose}
          choices={choices}
          label={t("workbench-context-tray-to-a-harness")}
          disabled={disabled}
          // The menu's own trigger is the button, which fades when held;
          // this is its face, the ghost beside Attach — never a button
          // inside a button, never faded twice.
          trigger={
            <span className={cn("anim inline-flex h-7 items-center gap-1 rounded-control border border-transparent px-2 text-xs font-medium text-text-dim", !disabled && "hover:bg-surface-2 hover:text-text")}>
              <ICON.harness size={12} aria-hidden />
              {t("workbench-context-tray-to-a-harness")}
              <ICON.expanded size={12} aria-hidden />
            </span>
          }
        />
      </span>
    </Tooltip>
  );
}
