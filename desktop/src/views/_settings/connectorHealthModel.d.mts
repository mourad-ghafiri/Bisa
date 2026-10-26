/** Types for `connectorHealthModel.mjs`. This file is the only reason TypeScript never has to read it. */

import type { AccountHealthView, ConnectorDetail } from "../../types";

export declare function healthTone(health: Pick<AccountHealthView, "state" | "check"> | null | undefined): "ok" | "warn" | "danger" | "quiet";
export declare function healthWords(health: AccountHealthView | null | undefined, name?: string): string;
export declare function healthDetail(health: AccountHealthView | null | undefined): string;
export declare function checkTargets(detail: ConnectorDetail): { connector: string; account: string }[];
export declare const CHECK_ALL_AT_ONCE: number;
/** Whether a bus frame moves the connectors list (a change), never a check. */
export declare function listReloads(payload: { type?: string } | null | undefined): boolean;
/** Whether a bus frame moves one connector's detail: a change, or a check of that connector's account. */
export declare function detailReloads(payload: { type?: string; connector?: string } | null | undefined, id: string | null | undefined): boolean;
/** The toast after *Check all*: null when every connector was read, else how many were not. */
export declare function checkAllSummary(tally: { queued: number; unread: number }): string | null;
