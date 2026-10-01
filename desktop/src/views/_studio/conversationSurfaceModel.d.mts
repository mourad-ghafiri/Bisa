/** Types for `conversationSurfaceModel.mjs`, plain JavaScript so `node --test` reads it. */

import type { ConversationOrigin } from "../../types";
import type { ConversationsQuery } from "../_workbench/conversationsStore";

/** What a surface lists and starts: the owner, a project's list in its place, and where its view is kept. */
export interface SurfaceSource {
  /** What *New conversation* is about, and whose pick is remembered. */
  owner: ConversationOrigin;
  /** List every conversation standing in this project instead of the owner's own — the IDE's pane. */
  project?: string | null;
  /** Where the list's search and switch are kept; `ownerPlace(owner)` when absent. */
  place?: string | null;
}

export type SurfaceView = "thread" | "list" | "reading" | "error" | "fallback" | "empty";

export interface SurfaceWords {
  door: string;
  hint: string;
  empty: string;
  reading: string;
}

export declare const PAGE: number;
export declare const PICK_KEY: string;
export declare function surfaceWords(source: SurfaceSource | null | undefined, at: { count: number; open?: boolean; picked?: boolean; hasFallback?: boolean }): SurfaceWords;
export declare function countWords(n: number): string;
export declare function selectedOf<R extends { id: string }>(rows: readonly R[] | null | undefined, wanted: string | null | undefined): R | null;
export declare function pickedId(wanted: string | null | undefined, remembered: string | null | undefined): string | null;
export declare function belongsTo(row: { origin?: { kind?: string; id?: string | null } | null; project?: string | null } | null | undefined, source: SurfaceSource): boolean;
export declare function surfaceView(at: { selected: { id: string } | null | undefined; settling: boolean; error: unknown; listOpen: boolean; loading: boolean; count: number; hasFallback: boolean }): SurfaceView;
export declare function ownerKey(origin: { kind: string; id?: string | null }): string;
/** The place an owner's list keeps its search and its switch under. */
export declare function ownerPlace(origin: { kind: string; id?: string | null }): string;
export declare function parsePicks(raw: unknown): Record<string, string>;
export declare function rememberPick(memory: Readonly<Record<string, string>>, owner: string, conversation: string | null): Readonly<Record<string, string>>;
export declare function listQuery(source: SurfaceSource, at?: { q?: string | null; archived?: boolean }): ConversationsQuery;
