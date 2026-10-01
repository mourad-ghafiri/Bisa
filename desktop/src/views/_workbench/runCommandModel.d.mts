/** Types for `runCommandModel.mjs`, plain JavaScript so `node --test` reads it. */

import type { BrowserMenuItem, ServedRow } from "./serversModel.mjs";

export interface RunFacts {
  command: string;
  trusted: boolean;
}
/** The Terminal caret's item for the project's run command. */
export type RunCommandItem = BrowserMenuItem & { id: "run" | "approve-run" };

export declare const APPROVAL_PLACE: string;
export declare function approvalWords(command: string): string;
export declare function runDoor(facts: { run: RunFacts | null; servers: readonly ServedRow[] }): { id: string; hint: string };
export declare function runCommandItem(run: RunFacts | null, facts: { door: string | null; busy: boolean }): RunCommandItem | null;
