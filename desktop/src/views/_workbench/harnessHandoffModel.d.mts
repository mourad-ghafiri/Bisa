import type { ContextRef, HarnessRow, SessionRow, SessionState } from "../../types";
import type { TerminalSessionState, TerminalTarget } from "../../shell/terminalsModel.mjs";
import type { Annotation } from "./annotationModel.mjs";
import type { PageRef } from "./contextChips.mjs";

/** A fresh terminal for a harness the node lists, or a live harness tab of the workstream. */
export type HarnessTarget =
  | { kind: "new"; harness: string; label: string; installed: boolean; detail: string | null }
  | { kind: "tab"; key: string; harness: string; state: SessionState | null; sessionId: string | null };

/** One row of the menu: the target, its words, and whether it may take the words now. */
export interface HarnessTargetRow {
  id: string;
  target: HarnessTarget;
  label: string;
  harness: string;
  disabled: boolean;
  /** The door's caution or refusal, beside the row. */
  hint: string | null;
  separatorBefore: boolean;
}

export declare function contextBlock(chips: readonly ContextRef[]): string;
export declare function handoffContent(instruction: string, target: string): string;
export declare function harnessWords(draft: { message: string; count: number; page: PageRef; annotations: readonly Annotation[] }): string;
export declare function harnessTargets(
  harnesses: readonly HarnessRow[] | null,
  sessions: readonly SessionRow[],
  terminals: readonly TerminalSessionState[],
  wid: string,
): HarnessTarget[] | null;
export declare function harnessDoor(target: HarnessTarget): { ok: boolean; words: string | null };
export declare function targetRows(targets: readonly HarnessTarget[] | null, labels: Readonly<Record<string, string>>): HarnessTargetRow[];
export declare function handOffTarget(wid: string, harness: string, words: string): TerminalTarget;
export declare function sentToWords(target: HarnessTarget, labels: Readonly<Record<string, string>>, noun: string): string;
