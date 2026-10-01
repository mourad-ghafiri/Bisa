import type { ConversationView, SessionRow } from "../../types";
import type { PaneSummary, RailRow } from "./agentRailModel.mjs";

export declare const VIEW_KEY: string;
export declare const LIST_VIEW: string;
export declare const MAX_REMEMBERED: number;
export declare function parseRemembered(raw: unknown): Record<string, string>;
export declare function remember(memory: Record<string, string>, root: string, conversation: string | null): Record<string, string>;
/** The conversation a hand-off from a checkout lands in — the one guess the desktop makes, written as the pick. */
export declare function currentConversation<R extends { id: string; origin: { kind: string; id?: string }; project?: string | null; archived: boolean; last_message_at?: number | null; created_at: number; agents: readonly string[] }>(
  rows: readonly R[],
  workstream: string,
  project: string,
  remembered: string | null | undefined,
): R | null;
export declare function sessionsOf(sessions: readonly SessionRow[], conversation: string | null | undefined): RailRow[];
export declare function turnSummary(rows: readonly RailRow[]): PaneSummary;
export declare function handOffTarget(
  current: { id: string; origin?: { kind?: string; id?: string } | null } | null | undefined,
  workstream: string,
  project: string,
): { conversation: string } | { start: { kind: "workstream"; id: string; project: string } };
export declare function rememberedStillHere(conversation: { project?: string | null; archived?: unknown } | null | undefined, pid: string): boolean;
export declare function handOffWords(agentName: string, started: boolean): string;
export type { ConversationView };
