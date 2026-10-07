/** Types for `nodeBootModel.mjs`. */

export type NodeFailureKind = "exited" | "timed_out" | "no_binary" | "held_by_other";

export interface NodeFailure {
  kind: NodeFailureKind | string;
  how: string | null;
  said: string[];
  secs: number | null;
  pid: number | null;
  tried: string[];
}

export interface NodeBootState {
  /** The boot's phase, `ready` once the node answers; `null` while failed or unknown. */
  phase: string | null;
  done: number | null;
  of: number | null;
  /** Why there is no node, while there is none. */
  failure: NodeFailure | null;
  /** Unasked-for restarts in a row, as the shell counts them. */
  attempt: number;
  /** When the shell tries again, in seconds, while a failure stands. */
  nextInSecs: number | null;
  /** The node answers. */
  ready: boolean;
  /** The shell has said something: a seed or an event. */
  known: boolean;
}

export type NodeDoor = "restart_now" | "reveal_log" | "open_data_folder" | "quit";

export declare const NODE_EVENTS: Readonly<{ boot: "node:boot"; failed: "node:failed"; restarted: "node:restarted" }>;
export declare const NODE_BOOT_INITIAL: NodeBootState;
export declare const NODE_DOORS: readonly NodeDoor[];
export declare function reduceNodeEvent(state: NodeBootState, name: string, payload: unknown): NodeBootState;
export declare function seedFromStatus(
  state: NodeBootState,
  status: { running: boolean; external: boolean; booting?: boolean; restarts: number; failure?: object | null } | null,
): NodeBootState;
export declare function phaseWords(phase: string | null, done: number | null, of: number | null): string | null;
export declare function failureWords(failure: NodeFailure | null | undefined): { line: string; details: string | null } | null;
export declare function bootLine(state: NodeBootState | null | undefined): string | null;
export declare function nodeDoors(state: NodeBootState | null | undefined): NodeDoor[];
export declare function doorWords(door: string): string;
export declare function doorFailedWords(door: string, reason: unknown): string;
