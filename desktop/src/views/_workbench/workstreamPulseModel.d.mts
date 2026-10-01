import type { SessionRow, SessionState } from "../../types";
import type { TerminalSessionState } from "../../shell/terminalsModel.mjs";
import type { SessionTone, SessionWord } from "../../ui/sessionState.mjs";
import type { WorkstreamPort } from "./portsModel.mjs";

export declare const ARGS_CHARS: number;

export type ToolTier = "read" | "write" | "exec";

export interface PulseMore {
  count: number;
  word: SessionWord | "working";
}

export interface PulseSubagents {
  total: number;
  working: number;
  waiting: number;
  failed: number;
  /** `name — state`, one per sub-agent, for a tooltip. */
  names: string[];
}

export type PulseCta = { kind: "answer"; gateId: string } | { kind: "abort"; sessionId: string };

export interface Pulse {
  word: SessionWord;
  tone: SessionTone;
  /** The running tool's tier, for a glyph; null off a running state. */
  tier: ToolTier | null;
  /** The subject: an agent id, a harness id, or `↳ name` for a sub-agent. */
  who: string;
  harness: string | null;
  headline: string;
  /** The tooltip: who, the full sentence, the full arguments. */
  detail: string;
  /** Unix seconds the subject entered its state; 0 when unknown (a shell). */
  since: number;
  /** `12s` while live, `3m` words for an ended state, empty when unknown. */
  elapsed: string;
  /**
   * Unix seconds a live counter should count up from, or null when the line is
   * not live. When set, the view renders a self-ticking `<LiveDuration>` leaf
   * from it rather than the baked `elapsed`, so the counter advances without
   * rebuilding the rail model.
   */
  liveStart: number | null;
  more: PulseMore | null;
  subagents: PulseSubagents;
  cta: PulseCta | null;
  /** The workstream's listening ports — chips on the line. */
  ports: WorkstreamPort[];
  /** Changes exactly when something visible changed — never on a token. */
  flashKey: string;
}

export declare function truncate(text: string | null | undefined, max: number): string;
export declare function headlineOf(state: SessionState | SessionWord | null | undefined): string;
export declare function pulseOf(input: {
  sessions: readonly SessionRow[];
  terminals: readonly TerminalSessionState[];
  workstream: string;
  /** Unix seconds. */
  now: number;
  ports?: readonly WorkstreamPort[];
}): Pulse | null;
export declare function projectPulse(pulses: readonly (Pulse | null)[]): Pulse | null;
export declare function moreLabel(more: PulseMore | null): string;
