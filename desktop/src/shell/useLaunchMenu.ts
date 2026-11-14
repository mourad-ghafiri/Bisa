/**
 * The list a caret shows for a place — the shell, the project's run command,
 * every harness with *resume* or *fresh session* — as menu items with their
 * icons and acts. The list itself is `launchMenuModel.launchMenu`'s; this
 * binds it: one hook, so the Terminal control's caret and the centre strip's
 * + caret are the same list and cannot drift.
 */

import { ICON, harnessMark, useToast } from "../ui";
import type { MenuItem } from "../ui";
import { chordHint } from "../ui/keymapHints";
import { runCommandItem, runDoor } from "../views/_workbench/runCommandModel.mjs";
import { entryTarget, launchMenu } from "./launchMenuModel.mjs";
import type { LaunchMenuItem, LaunchPlace } from "./launchMenuModel.mjs";
import { isLive, isRootedAt } from "./terminalsModel.mjs";
import { useLaunchableHarnesses } from "./useHarnesses";
import { runTheCommand, useRunCommand } from "./useRunCommand";
import { useServing } from "./useServing";
import { openTerminalIn, useTerminals } from "./useTerminals";

/** The glyph an item wears: a harness's own mark, the run triangle, a plus for a fresh session, the terminal for the shell. */
function iconOf(item: LaunchMenuItem): MenuItem["icon"] {
  if (item.kind === "open" && item.harness) return harnessMark(item.harness);
  if (item.kind === "fresh") return ICON.add;
  return item.kind === "run" ? ICON.play : ICON.harness;
}

/**
 * What to open in `place`, or nothing while there is no place.
 *
 * @param busy the place has no folder to run in yet — holds the run command, as the launcher's own disabled state does
 * @param shellCommand the keymap command that opens a shell in this place, when the surface is one where it does — the item then wears its chord
 */
export function useLaunchMenu(place: LaunchPlace | null, { busy = false, shellCommand = null }: { busy?: boolean; shellCommand?: string | null } = {}): MenuItem[] {
  const toast = useToast();
  const { sessions, launched } = useTerminals();
  const harnesses = useLaunchableHarnesses();
  // The project's run command, on a checkout: the item, and whether ⌘⇧R's
  // door is it (no server up) — then the item wears the chord.
  const wid = place?.scope === "workstream" ? place.id : null;
  const { run } = useRunCommand(wid);
  const { servers } = useServing(wid);
  if (!place) return [];

  const { scope, id } = place;
  const shells = sessions.filter((s) => isRootedAt(s, scope, id) && isLive(s)).length;
  const runItem = wid ? runCommandItem(run, { door: runDoor({ run, servers }).id, busy }) : null;
  return launchMenu({ harnesses, launched, scope, id, shells, run: runItem, shellCommand }).map((item) => ({
    label: item.label,
    icon: iconOf(item),
    disabled: item.disabled,
    separatorBefore: item.separatorBefore,
    shortcut: item.command ? chordHint(item.command) : null,
    onSelect: () => {
      if (item.kind === "shell") openTerminalIn(place);
      else if (item.kind === "run") {
        if (wid && run) runTheCommand(wid, run, toast.info);
      } else if (item.harness) openTerminalIn(entryTarget({ kind: item.kind, harness: item.harness }, place));
    },
  }));
}
