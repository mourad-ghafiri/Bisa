/**
 * Types for `activityModel.mjs`, which is plain JavaScript so `node --test` can
 * import it without a build step. This file is the only reason TypeScript
 * never has to read it.
 *
 * `activity.ts` is the layer above: it resolves an [`IconName`] to a component
 * and holds the exhaustiveness guards that keep this module's `switch`
 * statements honest against the wire types.
 */

import type {
  EngineEvent,
  JournalEntry,
  PulseRow,
  SessionEvent,
  ConversationFrame,
} from "./types";

/**
 * Salience, not category: the spine, the ambient, a failure, a wait, a
 * warning that is not yet a failure (a repository nobody can commit in) and
 * danger (a script that failed). The Pulse's fifth was once a conversation
 * *kind* of row rather than a level, so the same message rendered `dim`
 * while live and by its kind once it came back from the node — a concept is
 * a field of its own now, never a tone.
 */
export type Tone = "spine" | "dim" | "fail" | "wait" | "warn" | "danger";

/**
 * A glyph by name, resolved in `activity.ts` against the maps the Goals screen
 * Catalog read. `icon:<key of ICON>` | `gate:<GateKind>` |
 * `work:<WorkItemState>` | `status:<GoalStatus>` | `step:<StepState>`. An unrecognised name draws
 * nothing rather than the nearest thing that happens to exist — see `activity.ts`.
 */
export type IconName = string;

/** One field the summary line could not fit, shown when the row is opened. */
export interface DetailField {
  label: string;
  value: string;
}

export interface ModelLine {
  text: string;
  tone: Tone;
  icon?: IconName;
  detail?: DetailField[];
}

export interface ActivityLine extends ModelLine {
  key: string;
  at: number;
  author?: string;
  /** Conversation key for click-through, when the line has one. */
  goal?: string;
  /** What that conversation is called, when the source knows it. */
  title?: string;
  /** The feed's concept the line belongs to — a stored row's; a live line has none. */
  concept?: import("./types").ActivityConcept;
  /** What a stored row is about: the record a click opens. */
  source?: import("./types").PulseSource;
}

export declare function truncate(s: string, max?: number): string;
export declare function shortId(id: string | null | undefined): string;
/** Who listens, from a listener's or a host's wire word: `goal:…` a goal, anything else a library workflow. */
export declare function hostOf(word: unknown): "goal" | "workflow";
export declare function cents(v: number): string;
/** The Workflow Agent's standing on a guided goal, in words — one sentence per status. */
export declare function guidedWords(phase: string | null | undefined, status: string, detail?: string | null): ModelLine;
export declare function payloadLine(payload: JournalEntry["payload"]): ModelLine;
export declare function journalLine(e: JournalEntry, index: number): ActivityLine;
export declare function pulseLine(row: PulseRow): ActivityLine;
export declare function sessionLine(e: SessionEvent): ModelLine | null;
export declare function engineLine(e: EngineEvent, at?: number): ActivityLine | null;
export declare function conversationLine(f: ConversationFrame, at?: number): ActivityLine | null;
export declare function appendCapped(
  lines: ActivityLine[],
  next: ActivityLine,
  cap?: number,
): ActivityLine[];
