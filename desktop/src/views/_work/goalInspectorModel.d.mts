/** Types for `goalInspectorModel.mjs`. */

import type { BudgetSpent, Goal, GoalMode, GuidanceInfo, RunSummary, WorkflowRun, Workstream } from "../../types";
import type { IconName } from "../../ui/icons";
import type { Route } from "../../router";

export type Panel = "details" | "work" | "projects" | "files" | "properties";

export declare const PANELS: readonly Panel[];
export declare const PROPERTIES: "properties";
export declare function panelsFor(facts?: { properties?: boolean }): readonly Panel[];
export declare function panelOf(raw: unknown, offered?: readonly Panel[]): Panel;
export declare const PANEL_ICON: Readonly<Record<Panel, IconName>>;
export declare const PANEL_FOLD: Readonly<Record<Panel, number>>;
export interface PanelTab {
  id: Panel;
  label: string;
  icon: IconName;
  fold: number;
  count: number | null;
}
export declare function panelTabs(offered: readonly Panel[], counts?: { work?: number }): PanelTab[];
export declare function propertiesReveal(showing: { kind: string | null | undefined; panel: unknown }, selection: unknown): { aux: "inspector"; auxId: null; insp: "properties" } | null;
export declare function propertiesScrollName(selection: unknown): string;
/** Whether a work item is still at work: claimed, in progress, or in review. */
export declare function isRunning(item: { state?: { state?: string } | null } | null | undefined): boolean;
/** The rows the goal made first, the rest after, in a stable order. */
export declare function projectsFirst<R>(rows: readonly R[] | null | undefined, madeByGoal: (row: R) => boolean): R[];
export declare function spendWords(spent: Pick<BudgetSpent, "tokens" | "usd_cents" | "wall_clock_secs"> | null | undefined): string;
export declare function runCardWords(run: Pick<WorkflowRun, "id" | "workflow" | "outcome" | "cancelled">, runs: readonly RunSummary[] | null | undefined): string;
export declare const PROJECTS_NAMED: number;
export declare function projectsWords(rows: readonly { project: { name: string }; workstreams?: number }[] | null | undefined): { names: string; counts: string } | null;
export declare function designLine(mode: GoalMode | string, guidance: Pick<GuidanceInfo, "design_enabled" | "phase"> | null | undefined): string | null;

export interface OriginChip {
  id: "mode" | "parent" | "born" | "run";
  label: string;
  /** `mode` for the goal's mode, whose glyph is `goalMode.MODE_ICON`'s; else a key of `ICON`. */
  icon: "mode" | IconName;
  title?: string;
  /** Where the chip leads, when it is a door. */
  route?: Route;
}
export declare function originChips(goal: Pick<Goal, "origin">, run: Pick<WorkflowRun, "id" | "workflow"> | null | undefined, mode: GoalMode | string): OriginChip[];
export declare function workstreamWord(workstream: Pick<Workstream, "id" | "name" | "kind">): string;
