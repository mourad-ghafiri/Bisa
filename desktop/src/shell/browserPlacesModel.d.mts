/** Types for `browserPlacesModel.mjs`, plain JavaScript so `node --test` reads it. */

import type { GoalRow, ProjectRow, WorkflowRow, WorkstreamRef, InboxRow } from "../types";
import type { ChannelListEntry } from "../types.hand";
import type { BrowserHome, BrowserSession } from "./browsersModel.mjs";
import type { PlaceIndex } from "./footerSessionsModel.mjs";

export type OriginKind = "ide" | "goal" | "workflow" | "channel" | "dm" | "conversation" | "workspace";
export interface Origin {
  kind: OriginKind;
  label: string;
}
export declare const ORIGINS: readonly Origin[];

export interface BrowserPlaces {
  places: PlaceIndex;
  channels: Map<string, string>;
  dms: Map<string, string>;
  conversations: Map<string, string>;
  workflows: Map<string, string>;
}

export declare function browserPlaces(
  ws: { goals?: readonly Pick<GoalRow, "id" | "title" | "statement">[]; workstreams?: readonly WorkstreamRef[]; projects?: readonly ProjectRow[]; channels?: readonly ChannelListEntry[]; dms?: readonly ChannelListEntry[]; inbox?: readonly Pick<InboxRow, "key" | "kind" | "title">[] },
  workflows?: readonly Pick<WorkflowRow, "workflow">[],
): BrowserPlaces;
export declare function originOf(home: BrowserHome | null | undefined): Origin;
export declare function placeName(home: BrowserHome | null | undefined, places: BrowserPlaces): string | null;
export declare function whereWords(session: Pick<BrowserSession, "home"> | null | undefined, places: BrowserPlaces): string;
export declare function groupTabs<T extends Pick<BrowserSession, "home">>(sessions: readonly T[]): { kind: OriginKind; label: string; tabs: T[] }[];
export declare function footerBrowserWords(facts: { count: number; headless: number; busy: number; agents?: number; shown: string | null }): string;
