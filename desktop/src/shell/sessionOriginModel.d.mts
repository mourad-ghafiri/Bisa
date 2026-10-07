/**
 * Types for `sessionOriginModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

import type { AgentDef, SessionOrigin, SessionRow } from "../types";
import type { ChannelListEntry } from "../types.hand";
import type { Route } from "../routeModel.mjs";
import type { GoalPlace, PlaceIndex } from "./footerSessionsModel.mjs";

/** The place index with what naming a session needs beside places. */
export interface OriginIndex extends PlaceIndex {
  /** An agent's display name by id. */
  agents: Map<string, string>;
  /** A channel's name by id. */
  channels: Map<string, string>;
  /** The direct messages' ids. */
  dms: Set<string>;
  /** A harness's label by id. */
  harnessLabels: Record<string, string>;
  /** A person's name by pubkey. */
  nameOf: (pubkey: string) => string;
}

/** What the index is built from — the shell's lists, as `useWorkspace` holds them. */
export interface OriginSources {
  workstreams?: readonly unknown[];
  projects?: readonly unknown[];
  goals?: readonly GoalPlace[];
  agents?: readonly Pick<AgentDef, "id" | "name">[];
  channels?: readonly ChannelListEntry[];
  dms?: readonly ChannelListEntry[];
  nameOf?: (pubkey: string) => string;
}

/** One reading of a row: who, what for, where, where it opens, its kind's word. */
export interface SessionOriginWords {
  title: string;
  origin: string;
  place: string;
  door: Route | null;
  kindWord: string;
}

/** A row as this model reads it — a `SessionRow`, or an older node's without `origin` and `cwd`. */
export type RowLike = Partial<SessionRow> & { id?: string; kind?: string };

export declare const KINDS: readonly string[];
export declare const ORIGINS: readonly string[];
export declare function kindWord(kind: string | null | undefined): string;
export declare function originIndex(ws: OriginSources, harnessLabels?: Record<string, string>): OriginIndex;
export declare function emptyOriginIndex(): OriginIndex;
export declare function originOfRow(row: RowLike | null | undefined): SessionOrigin;
export declare function titleOf(row: RowLike, index?: OriginIndex): string;
export declare function placeOf(row: RowLike, index?: OriginIndex): string;
export declare function originWords(row: RowLike, index?: OriginIndex): string;
export declare function doorOf(row: RowLike, index?: OriginIndex): Route | null;
export declare function originOf(row: RowLike, index?: OriginIndex): SessionOriginWords;
export { emptyPlaceIndex } from "./footerSessionsModel.mjs";
