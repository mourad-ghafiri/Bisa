/**
 * Types for `participantsModel.mjs`, plain JavaScript so `node --test` can
 * import it without a build step. The shapes are structural: the model reads a
 * few fields of an agent, and the wire type widens to them.
 */

export interface RosterEntry<T> {
  id: string;
  name: string;
  /** Null when no agent of the workspace carries the id: kept and marked, never dropped. */
  agent: T | null;
}

export interface CandidateAgent {
  id: string;
  name: string;
  description?: string | null;
  system_prompt: string;
  harness: string;
  tags?: string[];
  pubkey: string;
  photo?: { sha256: string } | null;
  enabled?: boolean;
}

/** One agent as a picker row: the agent's tags and photo ride through as they are. */
export interface PickerRow<T extends CandidateAgent> {
  id: string;
  name: string;
  kind: "agent";
  description: string | null;
  harness: string;
  tags: T["tags"];
  avatar: string;
  photo: T["photo"];
  warning: string | null;
}

export declare function resolveRoster<T extends { id: string; name: string }>(ids: readonly string[], agents: readonly T[]): RosterEntry<T>[];
export declare function warningFor(agent: { enabled?: boolean; harness: string }, installed: ReadonlySet<string> | null): string | null;
export declare function roleLineOf(agent: { description?: string | null; system_prompt: string }): string | null;
export declare function candidateOf<T extends CandidateAgent>(agent: T, id: string, installed: ReadonlySet<string> | null): PickerRow<T>;
