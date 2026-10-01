/** Types for `liveTurnModel.mjs`, plain JavaScript so `node --test` reads it. */

export interface LiveTurn {
  text: string;
  thinking: string;
  /** Unix seconds the turn began; zero when only the bus said so. */
  since: number;
  /** The tool the agent runs right now; null between tools. */
  working: string | null;
  /** The id of the message the reply landed as, once it did; null while in flight. */
  landed: string | null;
}
export type LiveTurns = ReadonlyMap<string, ReadonlyMap<string, LiveTurn>>;
export type ThinkingMode = "auto" | "shown" | "hidden";
export type ThinkingIcon = "thinking" | "inspect" | "hidden";

export declare const NO_TURNS: LiveTurns;
export declare const THINKING_MODES: readonly ThinkingMode[];
export declare function thinkingMode(raw: unknown): ThinkingMode;
export declare function thinkingOpen(facts: { mode: ThinkingMode | string; own?: boolean | null; live: boolean; writing: boolean }): boolean;
export declare function thinkingChoices(): { id: ThinkingMode; label: string; description: string; icon: ThinkingIcon }[];
export declare function thinkingTriggerWords(mode: ThinkingMode | string): { label: string; title: string; icon: ThinkingIcon };
export declare function applyStreamed(turns: LiveTurns, frame: { scope: string; agent: string; text?: string; thinking?: string; working?: string | null }): LiveTurns;
export declare function primeTurns(turns: LiveTurns, scope: string, rows: readonly { agent: string; text: string; thinking: string; since: number; working?: string | null }[]): LiveTurns;
/** The message an `agent_replied` frame says the turn became — null when `posted` is false or it names none. */
export declare function landedOf(replied: { posted?: boolean; message?: string | null } | null | undefined): string | null;
export declare function settleTurn(turns: LiveTurns, scope: string, agent: string, message: string | null | undefined, watched?: boolean): LiveTurns;
/** A scope's settled turns gone, those in flight kept — its last reader left. */
export declare function dropSettled(turns: LiveTurns, scope: string): LiveTurns;
export declare function retireLanded(turns: LiveTurns, ids: ReadonlySet<string>): LiveTurns;
export declare function clearTurn(turns: LiveTurns, scope: string, agent: string): LiveTurns;
export declare function clearScope(turns: LiveTurns, scope: string): LiveTurns;
export declare function turnsOf(turns: LiveTurns, scope: string): (LiveTurn & { agent: string })[];
export declare function agentsOf(turns: LiveTurns, scope: string): string[];
export declare function sameAgents(a: readonly string[], b: readonly string[]): boolean;
export declare function glimpse(text: string, n?: number): string;
export declare function lengthWords(chars: number): string;
export declare function thinkingWords(chars: number, open: boolean, live?: boolean, elapsedS?: number): { label: string; length: string; hint: string };
