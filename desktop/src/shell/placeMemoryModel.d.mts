/** Types for `placeMemoryModel.mjs`. */
import type { Route, RouteName } from "../routeModel.mjs";

/** Where a person was: the last place, each section's, and the query each path was left with. */
export interface Places {
  v: number;
  /** The workspace the memory is for, once known. */
  owner: string | null;
  /** The last place stood on, as a hash — where a launch opens. */
  last: string | null;
  /** Each section's last place, by the section's route name. */
  sections: Record<string, string>;
  /** The query each path was left with, oldest first; `""` for a path left with none. */
  queries: readonly (readonly [path: string, query: string])[];
}

/** A query's pairs, split for the route it is on. */
export interface SplitQuery {
  state: [string, string][];
  pane: [string, string][];
  carried: [string, string][];
}

export declare const PLACES_KEY: "bisa.view.places";
export declare const PLACES_VERSION: number;
export declare const MAX_PATHS: number;
export declare const MAX_QUERY: number;
export declare const REMEMBERED: Readonly<Record<RouteName, readonly string[]>>;
export declare const PANE_KEYS: readonly string[];
export declare const ONE_SHOT: readonly string[];
export declare const CLASSIFIED: readonly string[];

export declare function emptyPlaces(owner?: string | null): Places;
export declare function routeOf(hash: string): Route | null;
export declare function pathOf(hash: string): string;
export declare function splitQuery(name: string, query: string): SplitQuery;
export declare function keptQuery(name: string, query: string): string;
export declare function rememberedQuery(places: Places, path: string): string | undefined;
export declare function knowsPath(places: Places, path: string): boolean;
export declare function arrive(places: Places, hash: string, how?: { exact?: boolean }): { places: Places; land: string; known: boolean };
export declare function launchHash(places: Places): string | null;
export declare function indexHash(places: Places, sectionKey: string): string;
export declare function sectionHash(places: Places, sectionKey: string, current: string): string;
export declare function forgetPath(places: Places, path: string): Places;
export declare function adoptPlaces(places: Places, owner: string | null | undefined): Places;
export declare function parsePlaces(raw: unknown): Places | undefined;
