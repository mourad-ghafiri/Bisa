export declare const CHECKOUT_KINDS: readonly string[];
/**
 * Types for `addressModel.mjs`, which is plain JavaScript so `node --test` can
 * import it without a build step.
 *
 * The shapes are structural rather than imports of `AgentDef`: the model reads
 * four fields, and the wire type widens to them.
 */

export interface AddressableAgent {
  id: string;
  pubkey: string;
  /** `"core"` marks the platform's own agent; see `isCoreAgent`. */
  origin?: unknown;
  enabled?: boolean;
}

export interface AddressChip<T> {
  pubkey: string;
  /** Null when this scope's candidates do not include the addressee. */
  agent: T | null;
}

export declare const WORKFLOW_AGENT_ID: string;
export declare const GENERAL_AGENT_ID: string;
export declare function coreAgents<T extends AddressableAgent>(agents: readonly T[] | null | undefined): T[];

export declare function isCore(a: AddressableAgent): boolean;
export declare function canAnswer(a: AddressableAgent): boolean;
export declare function isWorkflow(a: { id?: string } | null | undefined): boolean;
/** Whether a conversation of this kind can reach the agent at all — the Workflow Agent is a goal's, never a workstream's. */
export declare function reachableIn(a: { id?: string } | null | undefined, kind: string | null | undefined): boolean;
/** The agent a conversation of this kind is for — the Workflow Agent in one about a workflow — or null when it follows `agents.default`. */
export declare function defaultAddresseeIn(kind: string | null | undefined): string | null;
/** Whether the `@`-picker offers this otherwise hidden core agent here: the room's own agent, in its own room. */
export declare function suggestedIn(a: { id?: string } | null | undefined, kind: string | null | undefined): boolean;
export declare function addressable<T extends AddressableAgent>(
  agents: readonly T[] | null | undefined,
): T[];
export declare function addressableIn<T extends AddressableAgent>(
  agents: readonly T[] | null | undefined,
  kind: string | null | undefined,
): T[];
export declare function rosterable<T extends AddressableAgent>(
  agents: readonly T[] | null | undefined,
): T[];
export interface TeamMentionable {
  id: string;
  name: string;
  kind: "team";
  description: string;
  suggest: true;
}
export declare function teamMentionables(
  teams: readonly { id: string; name: string; purpose?: string | null; enabled?: boolean; members?: readonly unknown[] }[] | null | undefined,
): TeamMentionable[];
export declare function orderByRoster<T extends AddressableAgent>(
  agents: readonly T[] | null | undefined,
  rosterIds: readonly string[] | null | undefined,
): T[];
/** `null` is *never chosen*; `[]` is *chose nobody*. */
export declare function parseStored(raw: string | null | undefined): string[] | null;
export declare function initialAddressed(
  stored: readonly string[] | null | undefined,
  seed: readonly string[] | null | undefined,
): string[];
export declare function addressChips<T extends AddressableAgent>(
  pubkeys: readonly string[] | null | undefined,
  candidates: readonly T[] | null | undefined,
): AddressChip<T>[];
export declare function storedAddressKey(scope: string): string;
/** The remembered addressees that are still agents that can answer; nothing is dropped before the roster is read. */
export declare function answerableAddressed(
  pubkeys: readonly string[] | null | undefined,
  agents: readonly { pubkey: string; enabled?: boolean }[] | null | undefined,
  ready: boolean,
): string[];
