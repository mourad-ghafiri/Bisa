/** Types for `mcpHealthModel.mjs`. This file is the only reason TypeScript never has to read it. */

import type { McpHealthView, McpProbeReport } from "../../types";

export declare function healthTone(health: Pick<McpHealthView, "state"> | null | undefined): "ok" | "warn" | "quiet";
export declare function stageWords(stage: string | undefined): string;
export declare function eraWords(era: string | undefined): string;
export declare function healthWords(health: McpHealthView | null | undefined): string;
export declare function checkedWords(checkedAt: number | null | undefined, now?: number): string;
export declare function capabilityWords(caps: McpProbeReport["capabilities"] | undefined): string[];
export declare function reportLines(report: McpProbeReport): { ok: boolean; headline: string; detail: string[]; tools: { name: string; description?: string | null }[]; more: number };
export declare const CHECK_ALL_AT_ONCE: number;
