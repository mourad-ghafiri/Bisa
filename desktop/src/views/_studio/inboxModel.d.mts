import type { IconName } from "../../ui/icons";
/**
 * Types for `inboxModel.mjs`, which is plain JavaScript so `node --test` can
 * import it without a build step. This file is the only reason TypeScript
 * never has to read it.
 */

import type { Tone } from "../../activityModel.mjs";
import type { GateKind, InboxKind, InboxRow, NeedsAction, NoticeDto, PulseSource } from "../../types";
import type { InboxFrame } from "../../types";
import type { Route } from "../../router";

export type FilterId = "needs_you" | "unread" | "all";
export type SourceId = "any" | "messages" | "projects" | "workflows" | "goals" | "people";
export type RowState = "waiting" | "unread" | "read";
export type GroupId = "waiting" | "new" | "kept";
export type KeyAction = "next" | "prev" | "open" | "focus_ask" | "mark_read" | "mark_unread" | "clear";

export interface Option<T> {
  id: T;
  label: string;
  /** The glyph's key in `ui/icons.ts`, on the tabs that wear one. */
  icon?: IconName;
}

export declare const SOURCES: ReadonlyArray<Option<SourceId>>;
export declare const KIND_LABEL: Record<string, string>;

/** What a conversation row is about, in words — *about goal Dark mode* — or null: no conversation, or one about nothing in particular. */
export declare function aboutWords(row: Pick<InboxRow, "kind" | "origin"> | null | undefined, names?: { goal?: (id: string) => string | null; project?: (id: string) => string | null; workstream?: (id: string) => string | null; workflow?: (id: string) => string | null }): string | null;
/** The glyph a row wears — its kind's, or, for a conversation about a thing, that thing's. */
export declare function rowGlyph(row: Pick<InboxRow, "kind" | "origin"> | null | undefined): IconName;
/** The bucket the address names; *Needs you* for anything else. */
export declare function filterOf(raw: unknown): FilterId;
/** The source the address names; *Any* for anything else. */
export declare function sourceIdOf(raw: unknown): SourceId;
export declare function needsOf(row: Pick<InboxRow, "needs_action"> | null | undefined): InboxRow["needs_action"];
/** What tells one owed decision from the next: the gate's id, else the step it completes, else its kind and question — never its place in the list. */
export declare function needsKey(action: Pick<NeedsAction, "gate_id" | "run" | "step" | "gate_kind" | "question">): string;
export declare function noticesOf(row: Pick<InboxRow, "notices"> | null | undefined): NoticeDto[];
export declare function unreadNoticesOf(row: Pick<InboxRow, "notices" | "unread_notices"> | null | undefined): number;
export declare function kindLabel(kind: string | null | undefined): string;
export declare function rowState(row: InboxRow): RowState;
/** The claim waiting on you on a people row, when there is one (14-collaboration). */
export declare function joinOf(row: Pick<InboxRow, "join"> | null | undefined): InboxRow["join"] | null;
export declare function waitingOf(row: Pick<InboxRow, "waiting"> | null | undefined): InboxRow["waiting"] | null;
export declare function waitingWords(row: Pick<InboxRow, "waiting" | "title">, labels?: Record<string, string>): string | null;
/** What the join card says: who asks, as what. */
export declare function joinWords(row: InboxRow): string | null;
export declare function isHandled(row: InboxRow): boolean;
export declare function matchesFilter(row: InboxRow, filter: FilterId): boolean;
export declare function matchesSource(row: InboxRow, source: SourceId): boolean;
export declare function rank(row: InboxRow): number;
export declare function compareRows(a: InboxRow, b: InboxRow): number;
export declare function visibleRows(rows: readonly InboxRow[], filter: FilterId, source: SourceId): InboxRow[];
export declare function counts(
  rows: readonly InboxRow[],
  filter: FilterId,
): { buckets: Record<FilterId, number>; sources: Record<SourceId, number> };
export declare function filterSegments(buckets: Record<FilterId, number>): { id: FilterId; label: string }[];
export declare function sourceTabs(sources: Record<SourceId, number>): { id: SourceId; label: string; icon: IconName; count: number }[];
export declare function groupRows(visible: readonly InboxRow[]): { id: GroupId; title: string; rows: InboxRow[] }[];
export declare function nextSelection(visible: readonly InboxRow[], known: readonly InboxRow[], selected: string | null): string | null;
export declare function applyDelta(rows: readonly InboxRow[], frame: InboxFrame): InboxRow[];
export declare function needsReload(rows: readonly InboxRow[], frame: InboxFrame): boolean;

/**
 * `icon` is a name, not a component: `waiting` | `question` | `handled` |
 * `gate:<GateKind>` | `notice:<NoticeKind>` | `kind:<InboxKind>`. The view maps it.
 * `tone` is the accent for an ask, the notice's own tone, `ok` when handled, `dim` otherwise.
 */
export declare function summarize(row: InboxRow): {
  text: string;
  icon: string;
  urgent: boolean;
  tone: Tone | "accent" | "ok";
};

export interface NoticeLine {
  key: string;
  at: number;
  text: string;
  tone: Tone;
  icon?: string;
  notice: NoticeDto["notice"];
  source: PulseSource;
}
export declare function noticeLine(notice: NoticeDto): NoticeLine;
/** The chip an ask's card wears: *question*, or what its gate is about. */
export declare function askTitle(action: Pick<NeedsAction, "subject" | "gate_kind">, isQuestion: boolean): string;
export declare function askVerbs(action: Pick<NeedsAction, "subject">): { approve: string; decline: string | null };
export declare function askHint(action: Pick<NeedsAction, "subject">): string | null;

export declare function doorOf(row: InboxRow): { route: Route; search: string | null } | null;
export declare function conversationOf(row: InboxRow): { kind: "goal" | "channel" | "dm" | "conversation"; scope: string } | null;
export declare function readOnSelect(row: InboxRow): boolean;
export declare function unreadKeys(visible: readonly InboxRow[]): string[];
export declare const MARKS_AT_ONCE: number;
export declare function markBatches(keys: readonly string[], size?: number): string[][];
export declare function isNewestRead(asked: number, newest: number): boolean;
/** The door's words, by the kind of thing a row is. */
export declare function doorLabel(kind: InboxKind | string | null | undefined): string;
/** What an empty list says under a bucket and a source. */
export declare function emptyWords(filter: FilterId, source: SourceId): { title: string; hint: string };
export declare function keyAction(
  key: string,
  ctx: {
    inInput: boolean;
    inAsk: boolean;
    modifier: boolean;
    hasSelection: boolean;
    /** Focus stands on a control outside the list's rows — anything but the page itself or the list. */
    onControl?: boolean;
  },
): KeyAction | null;

export type { GateKind };

/** A gate's one line, from its subject: an adoption, an amendment, a held step's release, a step, or the kind. */
export declare function browserHomeOf(row: { kind: string; key: string } | null | undefined): { scope: "goal" | "channel" | "dm" | "conversation" | "workstream" | "workflow"; id: string } | null;
export declare function scopeKindOf(id: string, channels: readonly { channel: { id: string } }[], dms: readonly { channel: { id: string } }[]): "channel" | "dm" | null;
export declare function channelOfRow<C extends { id: string }>(row: { kind: string; key: string } | null | undefined, channels: readonly { channel: C }[], dms: readonly { channel: C }[]): C | null;
export declare const RELOAD_ON: readonly string[];
export declare const RELOAD_DEBOUNCE_MS: number;
export declare function wantsList(frame: { stream?: string; payload?: unknown } | null | undefined): boolean;
export declare function withReadMark(rows: readonly InboxRow[], key: string, read: boolean): InboxRow[];
