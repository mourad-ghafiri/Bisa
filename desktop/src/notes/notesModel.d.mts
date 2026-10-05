/**
 * Types for `notesModel.mjs`. This file is the only reason TypeScript never
 * has to read it.
 */

import type { Route } from "../router";

export type MarkKind = "bold" | "italic" | "code" | "link" | "heading" | "bullet" | "quote";

export declare const MARKS: Record<
  MarkKind,
  | { kind: "wrap"; mark: string; empty: string }
  | { kind: "line"; prefix: string }
  | { kind: "link"; empty: string }
>;

/** The six kinds a note can be about, as the wire spells them. */
export type OwnerScopeKind = "workspace" | "project" | "goal" | "workflow" | "channel" | "node";

/** A note's scope, in the shape `GET /notes` and `POST /notes` both take. */
export interface OwnerScope {
  scope: OwnerScopeKind;
  id?: string;
}

/** The tabs the panel lists by; `all` is first and the default. */
export type NoteTab = "all" | "workspace" | "projects" | "goals" | "workflows" | "channels" | "node";

export declare const NOTE_TABS: readonly NoteTab[];
export declare const NOTE_TAB_LABEL: Record<NoteTab, string>;

/** Which tab a stored preference selects; anything unknown gets `all`. */
export declare function noteTab(raw: string | null | undefined): NoteTab;
/** The scope kind a tab lists, or `null` for every kind. */
export declare function tabKind(tab: NoteTab): OwnerScopeKind | null;
/** The query string a tab's listing sends to `GET /notes`; empty for *All*. */
export declare function tabQuery(tab: NoteTab): string;
/** Whether a note of `kind` belongs on `tab`. */
export declare function tabAdmits(tab: NoteTab, kind: string): boolean;
/** The frames of a place leaving with its notes and drawings, and no note or drawing frame saying so. */
export declare const OWNER_GONE_FRAMES: readonly string[];
/** Whether a frame of this type is a place leaving with its notes and drawings. */
export declare function ownerGone(type: unknown): boolean;
/** Whether a frame of this type moves how many notes there are — what the dock's count is read again on. */
export declare function movesNoteCount(type: unknown): boolean;

/** A record a new note may be about: its id and the name a person knows it by. */
export interface NamedRecord {
  id: string;
  name: string;
}

/** The workspace's directory, for naming scopes and aiming new notes. */
export interface ScopeNames {
  projects?: NamedRecord[];
  goals?: NamedRecord[];
  workflows?: NamedRecord[];
  channels?: NamedRecord[];
}

/** One place a new note may go, with its words and whether it is where you stand. */
export interface NoteTarget {
  scope: OwnerScope;
  label: string;
  here: boolean;
}

export declare function routeTarget(
  route: Route | null | undefined,
  projectOf?: (workstream: string) => string | null | undefined,
): OwnerScope;
export declare function noteTargets(
  tab: NoteTab,
  route: Route | null | undefined,
  names: ScopeNames | null | undefined,
  projectOf?: (workstream: string) => string | null | undefined,
): NoteTarget[];
export declare function scopeWords(scope: OwnerScope | null | undefined, names: ScopeNames | null | undefined): string;
export declare function scopeOfRow(row: { scope: string; scope_id?: string | null } | null | undefined): OwnerScope;
export declare function filterNotes<T extends { title?: string; body?: string }>(notes: T[], query: string | null | undefined): T[];
export declare function matchAt<M>(matches: readonly M[] | null | undefined, index: number): M | null;


/** The spliced text plus the range to select afterwards. */
export interface Spliced {
  text: string;
  from: number;
  to: number;
}

export declare function wrapSelection(
  text: string,
  from: number,
  to: number,
  kind: MarkKind,
): Spliced;

/**
 * The outcome of a server copy arriving while the editor is open: nothing
 * (`same`), an agent's block landed under the buffer (`appended`), a rewrite
 * taken over a clean buffer (`taken`), or a rewrite over unsaved text
 * (`conflict`), with the body to show.
 */
export interface Adoption {
  body: string;
  outcome: "same" | "appended" | "taken" | "conflict";
}

export declare function adoptIncoming(
  local: string,
  confirmed: string,
  incoming: string,
): Adoption;

/** A parked draft: the text and the hash of the note it was typed against (`null` for an older build's bare text). */
export interface NoteDraft {
  body: string;
  base_hash: string | null;
}
export declare function parseDraft(raw: string): NoteDraft;
/** What the editor opens with for a note with a parked draft: restored and typed into, or restored as a conflict when the note moved since. */
export declare function restoredDraft(draft: NoteDraft | null | undefined, note: { body: string; hash: string }): { body: string; conflict: boolean; restored: boolean };
/** The newer of a read's row and a save's answer: the save stands unless the read is strictly newer. */
export declare function latestNote<R extends { updated_at: number }>(read: R, saved: R): R;
/** A list read landing beside the saves that landed while it was out. */
export declare function landedNotes<R extends { id: string; updated_at: number }>(rows: readonly R[], savedSince: ReadonlyMap<string, R>): readonly R[];

export declare function draftKey(id: string): string;

export declare function sameScope(a: OwnerScope | null | undefined, b: OwnerScope | null | undefined): boolean;
export declare function scopeQuery(scope: OwnerScope): string;

/** The three ways to look at a note, in the order the control draws them. */
export type NoteView = "write" | "split" | "read";

export declare const NOTE_VIEWS: readonly NoteView[];

/** Which view a stored preference selects; anything unknown gets `write`. */
export declare function noteView(raw: string | null | undefined): NoteView;

export declare const SAVE_MIN_MS: number;
export declare const SAVE_MAX_MS: number;
export declare const SAVE_STEP_MS: number;
export declare const SAVE_DEFAULT_MS: number;

/** A stored or typed autosave delay, made safe to hand to `setTimeout`. */
export declare function clampSaveDelay(raw: unknown): number;
export declare function noteStatusWords(s: { gone?: string | null; conflict?: string | null; error?: string | null; saving?: boolean; dirty?: boolean }): string;
/** What a refusal of a note's read or write means, by its status: a lost save, a note that is gone, or a failure. */
export declare function noteRefusal(status: number | null | undefined): "conflict" | "gone" | "failed";
export declare function noteGoneWords(why: string): string;
export declare function draftAction(body: string, confirmedBody: string): "forget" | "park";
/** The open note once the list has loaded: one kept from the last window that is gone opens nothing. */
export declare function listedNote(active: string | null, restored: string | null, listed: readonly string[]): string | null;
