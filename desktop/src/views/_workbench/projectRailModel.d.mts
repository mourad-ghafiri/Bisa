import type { Project, ProjectRow, SessionRow, WorkstreamRef, WorkstreamStatus, Workstream } from "../../types";
import type { TerminalSessionState, Liveness } from "../../shell/terminalsModel.mjs";
import type { Activity } from "./workstreamActivityModel.mjs";
import type { Pulse } from "./workstreamPulseModel.mjs";
import type { WorkstreamPort } from "./portsModel.mjs";
import type { Effort, SessionState } from "../../types";
import type { IconName } from "../../ui/icons";

export declare const UNGROUPED: string;
export type RailTab = "workspace" | "goals" | "workflows";
export declare const RAIL_TABS: readonly RailTab[];
export declare const RAIL_TAB_LABEL: Readonly<Record<RailTab, string>>;
export declare const RAIL_TAB_ICON: Readonly<Record<RailTab, IconName>>;
/** The turn in which each tab yields its word in a rail too narrow — Workflows first, Workspace last. */
export declare const RAIL_TAB_FOLD: Readonly<Record<RailTab, number>>;
/** Which tab a project belongs to — the one its origin names. */
export declare function tabOf(project: Pick<Project, "origin"> | null | undefined): RailTab;
export declare function isRailTab(v: unknown): v is RailTab;

export type RailRow =
  | { kind: "group"; id: string; label: string; /** What the heading folds: `group:<name>`, `ungrouped`, `workflow:<id>`. */ under: string; depth: number; collapsed: boolean; count: number; projects: string[] }
  | { kind: "goal"; id: string; label: string; /** `goal:<id>`. */ under: string; depth: number; collapsed: boolean; count: number; projects: string[] }
  | {
      kind: "project";
      id: string;
      project: Project;
      path: string;
      exists: boolean;
      goals: string[];
      under: string;
      depth: number;
      collapsed: boolean;
      activity: Activity;
      workstreams: number;
      current: boolean;
      /** The loudest workstream's line, on a collapsed project alone. */
      pulse: Pulse | null;
    }
  | {
      kind: "workstream";
      id: string;
      project: string;
      workstream: Workstream;
      status: WorkstreamStatus | null;
      primary: boolean;
      label: string;
      exists: boolean;
      activity: Activity;
      /** Distinct harness ids of the sessions here, first seen first — the row's harness glyphs. */
      harnesses: string[];
      /** One per open shell tab, dim when not live — the row's shell glyphs. */
      shellGlyphs: { harness: string | null; live: boolean }[];
      current: boolean;
      depth: number;
      collapsed: boolean;
      sessions: number;
      /** The live line under the row; null when nothing stands here. */
      pulse: Pulse | null;
    }
  | { kind: "terminal"; id: string; workstream: string; project: string; label: string; /** What the tab is about — launched, or typed into the shell; null for a plain shell. */ harness: string | null; liveness: Liveness; openedAt: number | null; exitedAt: number | null; depth: number }
  | {
      kind: "agent";
      id: string;
      workstream: string;
      project: string;
      label: string;
      harness: string;
      /** The model the session runs on, as the harness names it; null until it says. */
      model: string | null;
      /** The effort the session runs at, fitted to its model; null when the row carries none. */
      effort: Effort | null;
      agent: string | null;
      state: SessionState;
      /** The state's sentence, or a sub-agent's description. */
      activity: string;
      since: number;
      /** The session's registration instant, for total/completed-in time; null for a sub-agent. */
      started: number | null;
      workItem: string | null;
      goal: string | null;
      gateId: string | null;
      /** How many sub-agents nest under this session (a top-level agent row). */
      childCount?: number;
      /** The terminal tab a session a person opened in a terminal lives in; `null` for an engine session. */
      terminalKey?: string | null;
      /** The session a sub-agent row nests under; `null` for a session. */
      parent: string | null;
      depth: number;
    };

export declare function collapseKey(row: RailRow): string | null;
export declare function railRows(input: {
  tab?: RailTab;
  projects: ProjectRow[];
  workstreams: WorkstreamRef[];
  statuses?: readonly WorkstreamStatus[];
  sessions?: readonly SessionRow[];
  terminals?: readonly TerminalSessionState[];
  ports?: readonly WorkstreamPort[];
  goals?: { id: string; label: string }[];
  workflows?: { id: string; label: string }[];
  collapsed?: Set<string> | ((key: string) => boolean);
  current?: { scope: string; id: string } | null;
  filter?: string;
  now?: number;
  /** The person's manual `rail.order`; unlisted rows keep the default order. */
  order?: import("./railOrderModel.mjs").RailOrder;
}): RailRow[];
export declare function groupNames(projects: readonly ProjectRow[]): string[];
export declare function sectionOf(project: Pick<Project, "origin" | "group" | "id">): { tab: RailTab; kind: "group" | "goal"; id: string; under: string };
export declare function revealPlan(project: Pick<Project, "origin" | "group" | "id">): { tab: RailTab; expand: string[] };
export declare function rowIndexOfProject(rows: readonly RailRow[], pid: string): number;
export declare function projectOf(projects: readonly ProjectRow[], archivedProjects: readonly ProjectRow[], pid: string | null | undefined): ProjectRow | null;
export declare const ARCHIVED_NO_WORKSTREAM: string;
export declare function newWorkstreamOffer(project: Pick<Project, "name" | "archived"> | null | undefined): { label: string; disabled: boolean; reason: string | null };
export declare function rowIndexOfWorkstream(rows: readonly RailRow[], wid: string): number;
/** Where an import lands, in one sentence: under the goal a door named, else the Workspace tab (the Workflows tab says why). Nothing is guessed. */
export declare function importLanding(tab: RailTab, goalLabel: string | null): string;
export declare function railCounts(input: { projects: readonly ProjectRow[] }): Record<RailTab, number>;
/** A rail row as the generic tree draws it; `row` is the rail row itself. */
export interface RailTreeRow {
  id: string;
  depth: number;
  label: string;
  expandable: boolean;
  expanded: boolean;
  row: RailRow;
}
export declare function treeRowsOf(rows: readonly RailRow[], isCollapsed: (key: string) => boolean): RailTreeRow[];
export declare function attentionRank(state: SessionState | string | null | undefined): number;
export declare const RAIL_TAB_KEY: string;
export declare const RAIL_ARCHIVED_KEY: string;
export declare function railTabOf(raw: unknown): RailTab;
export declare function currentProjectOf<P extends { project: { id: string } }>(current: { scope: string; id: string } | null | undefined, workstreams: readonly { workstream: { id: string; project: string } }[], projects: readonly P[]): P | null;
