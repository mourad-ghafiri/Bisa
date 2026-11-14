/**
 * The one control that opens a terminal, so five surfaces cannot each invent it.
 *
 * It renders **nothing** outside the desktop shell. That is why the check lives
 * in a component rather than at each call site: a button that opens a panel
 * saying "not here" teaches whoever is doing UI work in a browser that the
 * terminal is broken, and the fifth surface to be wired is the one that forgets
 * to ask.
 *
 * # It is no longer a toggle, and that is the point
 *
 * The single-shell drawer's button was a toggle: pressing it while its own
 * shell showed closed the drawer, which ended the shell. That was defensible
 * when there was exactly one and the button was the only way to reach it. With
 * many, a control that *closes* sits one misclick away from ending somebody's
 * half-finished rebase — and it is the same button people press when they
 * cannot see the panel.
 *
 * So it reveals. Press it and you get a live shell for this place: the one
 * already here if there is one, a new one if there is not. Nothing this control
 * does can end a running process. Closing is the tab's own `×`, one level
 * further in, where it belongs.
 *
 * The caret beside it opens what to run — the login shell, or any harness the
 * node reports an interactive form for. The frontend passes the harness **id**;
 * `src-tauri/src/terminal.rs` resolves it to a command. A menu that carried
 * `{program, args}` would hand the webview a general-purpose process launcher.
 *
 * # The run command is the caret's too
 *
 * On a workstream the caret's second item is the project's **run command**
 * (ide/18) — *Run `npm run dev`* — when the project sets one, and nothing
 * when it does not: the setting is the Workstream scripts card's, and a
 * menu item that only opened a settings card was a door nobody wanted. The
 * words are `runCommandModel.runCommandItem`'s; the act is `useRunCommand`'s
 * `runTheCommand`, the same one ⌘⇧R takes when no server is up and the
 * command is approved — so the item wears the chord then. A command this
 * machine has not approved says so on its label, and a press opens the card
 * that approves it. The shell asks the node for the command itself
 * (`run: true`), and refuses one this machine has not approved.
 *
 * # Resuming, and why the label says so
 *
 * A harness the node reports a resume form for continues its latest session in
 * *this place* once it has run here before — the working directory is the
 * workstream, and these CLIs file their history by directory, so nothing has to
 * be tracked for that to mean the right thing.
 *
 * The entry says **resume** when it will resume, because it is the same click
 * either way and the difference is whether an hour of conversation comes back.
 * A second entry opens a fresh one, and appears only when the first would not
 * already do that — two entries that do the same thing is a choice the reader
 * has to work out is not one.
 *
 * # The list is not this control's alone
 *
 * What the caret lists — and whether a harness resumes — is
 * `launchMenuModel.mjs`'s, bound to acts by `useLaunchMenu`. The centre
 * strip's + has a caret with the same list (`TerminalStripControls.tsx`),
 * and the palette and the rail read the same rule, so no two doors to a
 * harness can say different things about one click.
 */

import { useEffect, useState } from "react";
import { api } from "../api";
import { ICON, Menu, Tooltip, cn, harnessMark } from "../ui";

/** The preferred harness's own mark on the split button. */
function PreferredMark({ id }: { id: string }) {
  const Glyph = harnessMark(id);
  return <Glyph size={12} aria-hidden />;
}
import type { TerminalScope } from "../terminal/session";
import { useLaunchableHarnesses } from "./useHarnesses";
import { placeNoun } from "./launchMenuModel.mjs";
import { isLive, isRootedAt } from "./terminalsModel.mjs";
import { useLaunchMenu } from "./useLaunchMenu";
import { canOpenTerminal, revealTerminalIn, useTerminals } from "./useTerminals";
import { t } from "../i18n/l10n.mjs";

export function TerminalLauncher({
  scope,
  id,
  project = null,
  label,
  disabledReason,
  className,
  wordClassName,
}: {
  scope: TerminalScope;
  id: string;
  /** The project this place belongs to — where `terminal.default_harness` is read. */
  project?: string | null;
  /** What this surface calls the place — shown on the tab. */
  label?: string | null;
  /** Set when there is nothing to open a terminal *in*, e.g. a missing folder. */
  disabledReason?: string | null;
  className?: string;
  /**
   * Classes for the word beside the glyph. A host that folds its toolbar to
   * glyphs on a narrow container passes `sr-only` plus its own container
   * variant, so the word leaves the line but stays the button's name.
   */
  wordClassName?: string;
}) {
  const { sessions } = useTerminals();
  const launchable = useLaunchableHarnesses();
  const defaultHarness = useDefaultHarness(project);
  const disabled = disabledReason !== null && disabledReason !== undefined;
  // What the caret lists — the shell, the run command, every harness with
  // resume or a fresh session — is the one list every such caret shows
  // (`launchMenuModel.launchMenu`); the run item is off exactly when this
  // control is.
  const items = useLaunchMenu({ scope, id, label }, { busy: disabled });
  if (!canOpenTerminal()) return null;

  const here = sessions.filter((s) => isRootedAt(s, scope, id) && isLive(s)).length;
  // `terminal.default_harness` (project scope): the plain Terminal control
  // opens that harness when it names one the node can run; otherwise a shell.
  const preferred = launchable.find((h) => h.installed && h.id === defaultHarness) ?? null;
  const mainTarget = preferred ? { scope, id, label, harness: preferred.id } : { scope, id, label };

  return (
    <span className={cn("inline-flex items-center", className)}>
      <Tooltip
        label={
          disabledReason ??
          (here > 0 ? t("shell-terminal-launcher-show-terminal-in", { where: placeNoun(scope) }) : t("shell-terminal-launcher-open-terminal-in", { where: placeNoun(scope) }))
        }
      >
        <button
          type="button"
          disabled={disabled}
          onClick={() => revealTerminalIn(mainTarget)}
          className="anim inline-flex h-7 items-center gap-1.5 rounded-l-control border border-r-0 border-border px-2 text-xs text-text-dim hover:bg-surface-2 hover:text-text disabled:opacity-45"
        >
          {preferred ? <PreferredMark id={preferred.id} /> : <ICON.harness size={12} aria-hidden />}
          <span className={wordClassName}>{preferred ? preferred.label : t("shell-terminal-launcher-terminal")}</span>
          {here > 0 && <span className="tnum text-text">{here}</span>}
        </button>
      </Tooltip>
      <Menu
        label={t("shell-terminal-launcher-what-to-open-in", { where: placeNoun(scope) })}
        items={items}
        trigger={
          <span
            aria-disabled={disabled}
            className={cn(
              "anim inline-flex h-7 items-center rounded-r-control border border-border px-1 text-text-dim",
              disabled ? "opacity-45" : "hover:bg-surface-2 hover:text-text",
            )}
          >
            <ICON.expanded size={12} aria-hidden />
          </span>
        }
      />
    </span>
  );
}

/**
 * The project's `terminal.default_harness`, or null. Read once per project;
 * a change in Settings shows up on the next mount, which is the page reload
 * the settings screen already implies.
 */
function useDefaultHarness(project: string | null): string | null {
  const [value, setValue] = useState<string | null>(null);
  useEffect(() => {
    if (!project) {
      setValue(null);
      return;
    }
    let alive = true;
    api
      .settingsResolved(project)
      .then((r) => {
        if (!alive) return;
        const v = r.settings.find((x) => x.key === "terminal.default_harness")?.value;
        setValue(typeof v === "string" && v.trim() ? v.trim() : null);
      })
      .catch(() => {
        if (alive) setValue(null);
      });
    return () => {
      alive = false;
    };
  }, [project]);
  return value;
}
