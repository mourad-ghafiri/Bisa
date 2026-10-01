/**
 * Types for `assigneeWire.mjs`, which is plain JavaScript so `node --test` can
 * import it without a build step. This file is the only reason TypeScript
 * never has to read it.
 */

import type { Assignee } from "../../types";

export type AssigneeKind = "agent" | "human" | "team";

export declare const ASSIGNEE_KINDS: readonly AssigneeKind[];

export declare function kindOf(key: string): AssigneeKind | null;
export declare function idOf(key: string): string;
export declare function wireToAssignee(key: string): Assignee | null;
export declare function requireWire(key: string): Assignee;
export declare function assigneeToWire(a: Assignee): string;
export declare function isWire(key: string): boolean;
