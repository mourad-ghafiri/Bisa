import type { FileScope } from "./types";

/**
 * What the IDE roots at: a checkout, a work item or a goal — the wire's
 * `FileScope` but `run`. A run in the workspace keeps its own folder, which
 * the Files API also answers; the IDE reaches it through the run's work items.
 */
export type WorkbenchScope = Exclude<FileScope, "run">;
export type Route =
  | { name: "pulse" }
  | { name: "inbox" }
  | { name: "goals" }
  | { name: "goal"; id: string }
  | { name: "workflows" }
  | { name: "workflow"; id: string }
  /** One run by its id — a run of the workspace's page (a goal's run is its goal's). */
  | { name: "run"; id: string }
  | { name: "projects" }
  | { name: "workbench"; scope: WorkbenchScope; id: string }
  | { name: "channels" }
  | { name: "channel"; id: string }
  | { name: "messages" }
  | { name: "dm"; id: string }
  /** A channel or a direct channel on a workspace this node is a guest of (14-collaboration). */
  | { name: "hosted_channel"; host: string; id: string }
  | { name: "hosted_dm"; host: string; id: string }
  | { name: "conversation"; id: string }
  | { name: "agents" }
  | { name: "agent"; id: string }
  | { name: "teams" }
  | { name: "settings" };
export type RouteName = Route["name"];
export type SearchPatch = Record<string, string | number | undefined | null>;

export declare const WORKBENCH_SCOPES: readonly WorkbenchScope[];
export declare const ROUTE_TABLE: ReadonlyArray<readonly [string, RouteName]>;
export declare const ROUTE_NAMES: readonly RouteName[];
export declare function splitHash(hash: string): { path: string; query: string };
export declare function parse(hash: string, home: Route): Route;
export declare function queryOf(search?: SearchPatch | null): string;
export declare function applySearchPatch(query: string, patch?: SearchPatch | null): string;
export declare function href(route: Route, search?: SearchPatch): string;
export declare function section(route: Route): RouteName;
/** The index of a route's section — its list. */
export declare function indexRouteOf(route: Route): Route;
