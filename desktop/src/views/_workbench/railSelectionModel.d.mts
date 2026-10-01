import type { RailRow } from "./projectRailModel.mjs";

/** What a person selected in the rail: a group's projects, or one project. */
export type RailSelection = { kind: "group"; label: string; projects: string[] } | { kind: "project"; id: string; label: string };

export declare function selectionOf(row: RailRow | null | undefined): RailSelection | null;
export declare function parseSelection(raw: unknown): RailSelection | null;
export declare function listedSelection(selection: RailSelection | null, listed: Iterable<string>): RailSelection | null;
export declare function sameSelection(a: RailSelection | null, b: RailSelection | null): boolean;
export declare function boardScope(selection: RailSelection | null, nameOf?: (id: string) => string | null | undefined): { projects: Set<string> | null; words: string; all: boolean };
