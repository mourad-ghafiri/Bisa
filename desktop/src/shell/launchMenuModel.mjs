/**
 * What a place can open in a terminal (ide/06), as facts: the shell, the
 * project's run command, and every harness with an interactive form — which
 * of them continues the session it left there, and which starts another.
 *
 * **One rule, one list.** The Terminal control's caret, the centre strip's +
 * caret, the palette's `Terminal: … here` rows and the rail's workstream menu
 * all offer the same things, and each used to work out for itself whether a
 * harness would *resume*: two copies of the rule agreed by luck, and the rail
 * had none — it said *New … here* over a click that resumed. The rule is
 * `resumesHere`; the entries are `harnessEntries`; the caret's whole list is
 * `launchMenu`. The surfaces map ids to acts and icons
 * (`shell/useLaunchMenu.ts`) and decide nothing.
 *
 * What is passed on is a harness **id**, never a program: the shell resolves
 * it (`src-tauri/src/terminal.rs`). Plain `.mjs`, so `node --test` reads it.
 */

import { t } from "../i18n/l10n.mjs";
import { hasLaunched, launchKey } from "./terminalsModel.mjs";

/**
 * Whether opening this harness in this place continues its latest session
 * there: the node reports a way to resume it (`launch.resume_args`) **and**
 * it has run in this very place — a resume with nothing to continue is an
 * error in the harness's own terminal, not an empty session
 * (`terminalsModel.rememberLaunch`).
 * @param {{id: string, launch?: {resume_args?: readonly string[]} | null} | null | undefined} harness
 * @param {readonly string[] | null | undefined} launched the launch memory (`useTerminals().launched`)
 * @param {string} scope @param {string} id the place
 */
export function resumesHere(harness, launched, scope, id) {
  if (!harness) return false;
  return (harness.launch?.resume_args?.length ?? 0) > 0 && hasLaunched(launched ?? [], launchKey(harness.id, scope, id));
}

/**
 * The harness entries of a place, in the order given: one that **opens** each
 * harness — resuming when it would — and, beside one that resumes, a
 * **fresh** one. Only then: two entries that do the same thing is a choice
 * the reader has to work out is not one. A harness that is not installed
 * keeps its one entry, to say where it stands.
 * @param {readonly {id: string, label: string, installed: boolean, detail?: string | null, launch?: {resume_args?: readonly string[]} | null}[] | null | undefined} harnesses the launchable ones (`useLaunchableHarnesses`)
 * @param {readonly string[] | null | undefined} launched
 * @param {string} scope @param {string} id
 */
export function harnessEntries(harnesses, launched, scope, id) {
  const entries = [];
  for (const h of harnesses ?? []) {
    const resumes = h.installed && resumesHere(h, launched, scope, id);
    entries.push({ id: `open:${h.id}`, kind: "open", harness: h.id, name: h.label, installed: h.installed, resumes, detail: h.detail ?? null });
    if (resumes) entries.push({ id: `fresh:${h.id}`, kind: "fresh", harness: h.id, name: h.label, installed: true, resumes: false, detail: null });
  }
  return entries;
}

/**
 * The terminal an entry opens: the place and the harness — and, for the
 * fresh one alone, `resume: false`, explicit so it beats *has run here
 * before* (`terminalsModel.openTerminal`).
 * @param {{kind: string, harness: string}} entry
 * @param {{scope: string, id: string, label?: string | null}} place
 */
export function entryTarget(entry, place) {
  return entry.kind === "fresh" ? { ...place, harness: entry.harness, resume: false } : { ...place, harness: entry.harness };
}

/**
 * What an entry says: the harness's name; **resume** when the click will
 * resume — it is the same click either way, and the difference is whether an
 * hour of conversation comes back; *fresh session* for the one beside it; and
 * for a harness that is not on this machine, why.
 * @param {{kind: string, name: string, installed: boolean, resumes: boolean, detail: string | null}} entry
 */
export function entryLabel(entry) {
  if (!entry.installed) return t("shell-terminal-launcher-unavailable", { label: entry.name, detail: entry.detail ?? t("shell-terminal-launcher-not-installed") });
  if (entry.kind === "fresh") return t("shell-terminal-launcher-fresh-session", { h: entry.name });
  return entry.resumes ? t("shell-terminal-launcher-resume", { h: entry.name }) : entry.name;
}

/**
 * The whole list a caret shows for a place: the shell — *New shell here*
 * when a live one is already rooted there —, the project's run command when
 * it sets one (`runCommandModel.runCommandItem`, as given), then the harness
 * entries under a rule.
 * @param {{
 *   harnesses: Parameters<typeof harnessEntries>[0],
 *   launched: readonly string[] | null | undefined,
 *   scope: string, id: string,
 *   shells: number,                 the live shells already rooted here
 *   run: {id: string, label: string, disabled?: boolean, command?: string} | null,
 *   shellCommand: string | null,    the keymap command that opens a shell here, where one does
 * }} facts
 * @returns {{id: string, kind: "shell" | "run" | "open" | "fresh", label: string, harness?: string, command?: string, disabled?: boolean, separatorBefore?: boolean}[]}
 */
export function launchMenu({ harnesses, launched, scope, id, shells, run, shellCommand }) {
  const items = [{ id: "shell", kind: "shell", label: shells > 0 ? t("shell-terminal-launcher-new-shell-here") : t("shell-terminal-launcher-shell"), ...(shellCommand ? { command: shellCommand } : {}) }];
  if (run) items.push({ id: run.id, kind: "run", label: run.label, disabled: run.disabled === true, separatorBefore: true, ...(run.command ? { command: run.command } : {}) });
  harnessEntries(harnesses, launched, scope, id).forEach((entry, i) => {
    items.push({ id: entry.id, kind: entry.kind, harness: entry.harness, label: entryLabel(entry), disabled: !entry.installed, separatorBefore: i === 0 });
  });
  return items;
}

/**
 * What a sentence calls the place a terminal opens in — *Open a terminal in
 * this workstream*.
 * @param {"goal" | "workstream" | "work_item" | "machine"} scope
 */
export function placeNoun(scope) {
  switch (scope) {
    case "goal":
      return t("shell-terminal-launcher-goal-s-folder");
    case "work_item":
      return t("shell-terminal-launcher-where-work-item-runs");
    case "machine":
      return t("shell-terminal-launcher-home-directory");
    default:
      return t("shell-terminal-launcher-workstream");
  }
}
