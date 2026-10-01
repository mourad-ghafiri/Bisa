import type { BoardColumn, PlaceWorkstreamBody, Workstream, WorkstreamRef, WorkstreamStatus } from "../../types";

export type Column = BoardColumn;

export declare const COLUMNS: readonly Column[];
export declare const COLUMN_LABEL: Readonly<Record<Column, string>>;
export declare const COLUMN_HINT: Readonly<Record<Column, string>>;

export interface BoardRow {
  id: string;
  workstream: Workstream;
  ref: WorkstreamRef;
  status: WorkstreamStatus | null;
  project: { id: string; name: string };
  column: Column;
  /** Whether a person chose the column, as against the lifecycle placing it. */
  placed: boolean;
  rank: number | null;
  due: string | null;
  title: string;
  branch: string | null;
  primary: boolean;
}

export type DueKind = "none" | "overdue" | "today" | "soon" | "due";
export interface DueWords {
  kind: DueKind;
  tone: "quiet" | "warn" | "danger";
  label: string;
  days: number | null;
}

export declare function columnForState(state: { state: string } | null | undefined): Column;
export declare function columnShown(w: Pick<Workstream, "state" | "board">): Column;
export declare function todayKey(now?: number): string;
export declare function daysUntil(due: string, today: string): number;
export declare function dueTone(due: string | null | undefined, today: string, soonDays?: number): DueWords;
export declare function matches(row: Pick<BoardRow, "title" | "branch" | "project" | "workstream">, needle: string): boolean;
export declare function boardRows(input: {
  refs: readonly WorkstreamRef[];
  statuses: Readonly<Record<string, WorkstreamStatus>>;
  needle?: string;
  /** The rail's selection: a group's projects, one project, or null for all. */
  projects?: ReadonlySet<string> | null;
  archived?: boolean;
}): { columns: Record<Column, BoardRow[]>; total: number; hidden: number };
/** A Doing column against its limit; `title` is the warning's words when it is over, else null. */
export declare function wipState(count: number, limit: number): { over: boolean; label: string; title: string | null };
/** What the bar says the scope and the filters leave: *3 of 12 workstreams*. */
export declare function countWords(shown: number, total: number): string;
/** What the Board's *Close* asks before it runs, with what ends with it when anything does. */
export declare function closeWords(consent: string | null | undefined): string;
/** A card's project, by name; the word for one the workspace could not name. */
export declare function projectWords(project: { name?: string | null } | null | undefined): string;
/** The largest index the wire takes (`u32`). */
export declare const MAX_INDEX: number;
/** The index the node is asked for, for a card set down among the cards the Board shows. */
export declare function placeIndex(drop: { refs: readonly Pick<WorkstreamRef, "workstream">[]; shown: Readonly<Record<string, readonly { id: string }[]>>; id: string; column: Column; index: number }): number;
/** The body of `PUT /workstreams/{wid}/board/place`. */
export declare function placeBody(column: Column, index: number): PlaceWorkstreamBody;
/** Whether a card may be moved to a column. */
export declare function canMoveTo(row: Pick<BoardRow, "column" | "workstream">, column: Column): boolean;
/** Every status by its workstream's id — what `boardRows` reads. */
export declare function statusIndex(statuses: readonly WorkstreamStatus[] | null | undefined): Record<string, WorkstreamStatus>;
/** When a card last moved, unix seconds: its newest session's activity, else when it was opened, else now. */
export declare function lastActivity(sessions: readonly { workstream?: string | null; last_activity?: number | null }[] | null | undefined, row: Pick<BoardRow, "id" | "workstream">, now: number): number;
export declare function optimisticMove(columns: Record<Column, BoardRow[]>, id: string, column: Column, index: number): Record<Column, BoardRow[]>;
export declare function headerCounts(rows: readonly BoardRow[], today: string, soonDays: number): { count: number; overdue: number };
