/** Types for `launchMenuModel.mjs`, plain JavaScript so `node --test` reads it. */

import type { TerminalScope } from "../terminal/session";
import type { TerminalTarget } from "./terminalsModel.mjs";

/** A launchable harness, as much of a `HarnessRow` as the list reads. */
export interface LaunchableHarness {
  id: string;
  label: string;
  installed: boolean;
  detail?: string | null;
  launch?: { resume_args?: readonly string[] } | null;
}

/** One thing a harness offers in a place: open it (resuming when it would), or start a fresh session beside one that resumes. */
export interface LaunchEntry {
  id: string;
  kind: "open" | "fresh";
  harness: string;
  name: string;
  installed: boolean;
  /** Opening it continues the session it left here. */
  resumes: boolean;
  /** Why it cannot open, when the node said. */
  detail: string | null;
}

/** Where a terminal opens. */
export interface LaunchPlace {
  scope: TerminalScope;
  id: string;
  label?: string | null;
}

/** One item of a caret's list. */
export interface LaunchMenuItem {
  id: string;
  kind: "shell" | "run" | "open" | "fresh";
  label: string;
  /** The harness an `open` or a `fresh` item starts. */
  harness?: string;
  /** The keymap command that is another door to the same thing. */
  command?: string;
  disabled?: boolean;
  separatorBefore?: boolean;
}

/** Whether opening this harness in this place continues its latest session there. */
export declare function resumesHere(harness: Pick<LaunchableHarness, "id" | "launch"> | null | undefined, launched: readonly string[] | null | undefined, scope: string, id: string): boolean;
export declare function harnessEntries(harnesses: readonly LaunchableHarness[] | null | undefined, launched: readonly string[] | null | undefined, scope: string, id: string): LaunchEntry[];
/** The terminal an entry opens — `resume: false` on the fresh one alone. */
export declare function entryTarget(entry: Pick<LaunchEntry, "kind" | "harness">, place: LaunchPlace): TerminalTarget;
export declare function entryLabel(entry: Pick<LaunchEntry, "kind" | "name" | "installed" | "resumes" | "detail">): string;
/** The whole list a caret shows for a place. */
export declare function launchMenu(facts: {
  harnesses: readonly LaunchableHarness[] | null | undefined;
  launched: readonly string[] | null | undefined;
  scope: string;
  id: string;
  /** The live shells already rooted here. */
  shells: number;
  run: { id: string; label: string; disabled?: boolean; command?: string } | null;
  /** The keymap command that opens a shell here, where one does. */
  shellCommand: string | null;
}): LaunchMenuItem[];
export declare function placeNoun(scope: TerminalScope): string;
