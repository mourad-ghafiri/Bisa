export type Occupant = "files" | "git" | "agents" | "about" | "workstreams";
export type GitView = "changes" | "branches" | "history" | "stashes";
export type AboutView = "project" | "checkout" | "settings";
/** The Changes view's layout: the changed files as folders, or as a flat list. */
export type ChangesLayout = "tree" | "list";
/** The Changes view's filter: every changed file, or one standing. */
export type ChangesFilter = "all" | "staged" | "unstaged" | "tracked" | "untracked" | "modified";
export type RemoteLayout = "tree" | "list";
import type { PatchView } from "../_work/patchViewModel.mjs";
/** The occupants that have views of their own. */
export type ViewedOccupant = "git" | "about";
/** Every remembered choice: the occupants' views, the Changes and Remotes layouts, and a patch's view. */
export type PanelViews = { git: GitView; about: AboutView; changes: ChangesLayout; changesFilter: ChangesFilter; remotes: RemoteLayout; patch: PatchView };
export type ViewChoice = keyof PanelViews;

export declare const RAIL_GROUPS: readonly (readonly Occupant[])[];
export declare const OCCUPANTS: readonly Occupant[];
export declare const OCCUPANT_LABEL: Readonly<Record<Occupant, string>>;
export declare const OCCUPANT_COMMAND: Readonly<Record<Occupant, string>>;
export declare const GIT_VIEWS: readonly GitView[];
export declare const GIT_VIEW_LABEL: Readonly<Record<GitView, string>>;
export declare const ABOUT_VIEWS: readonly AboutView[];
export declare const ABOUT_VIEW_LABEL: Readonly<Record<AboutView, string>>;
export declare const CHANGES_LAYOUTS: readonly ChangesLayout[];
export declare const CHANGES_LAYOUT_LABEL: Readonly<Record<ChangesLayout, string>>;
export declare const CHANGE_FILTERS: readonly ChangesFilter[];
export declare const CHANGE_FILTER_LABEL: Readonly<Record<ChangesFilter, string>>;
export declare const REMOTE_LAYOUTS: readonly RemoteLayout[];
export declare const REMOTE_LAYOUT_LABEL: Readonly<Record<RemoteLayout, string>>;
export declare const DEFAULT_VIEWS: Readonly<PanelViews>;
export declare function isOccupant(v: unknown): v is Occupant;
export declare function isViewOf(choice: string, view: unknown): boolean;
export declare function parseViews(value: unknown): PanelViews;
export declare function availableOccupants(root: { scope: string; hasProject: boolean; centre?: "documents" | "conversation" }): Occupant[];
export type AboutBody = "project" | "work_item" | "goal" | "none";
export declare function aboutBody(root: { scope: string; hasProject: boolean }): AboutBody;
export declare function resolveOccupant(wanted: unknown, available: readonly Occupant[]): Occupant;
export declare function railGroups(): Occupant[][];
/** Whose scrollport the panel body is: the occupant's own, or the body scrolling as one. */
export type OccupantScroll = "own" | "panel";
export declare const OCCUPANT_SCROLL: Readonly<Record<Occupant, OccupantScroll>>;
export declare function occupantScroll(tab: string): OccupantScroll;
export declare function pressOccupant(
  state: { open: boolean; tab: Occupant },
  target: Occupant,
): { open: boolean; tab: Occupant };
export declare function recallFor(
  state: { tab: Occupant; root: string | null; byRoot: Readonly<Record<string, Occupant>> },
  nextRoot: string,
): { tab: Occupant; root: string | null };
export declare function parseRememberedTabs(value: unknown): Record<string, Occupant>;
