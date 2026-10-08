/** Types for `connectorHealthModel.mjs`. This file is the only reason TypeScript never has to read it. */

import type { AccountHealthView, ConnectorDetail } from "../../types";

export declare function healthTone(health: Pick<AccountHealthView, "state" | "check"> | null | undefined): "ok" | "warn" | "danger" | "quiet";
export declare function healthWords(health: AccountHealthView | null | undefined, name?: string): string;
export declare function healthDetail(health: AccountHealthView | null | undefined): string;
export declare function checkTargets(detail: ConnectorDetail): { connector: string; account: string }[];
export declare const CHECK_ALL_AT_ONCE: number;
