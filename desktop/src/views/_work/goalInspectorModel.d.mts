/** Types for `goalInspectorModel.mjs`. */

import type { BudgetSpent, Goal, GoalMode, GuidanceInfo, RunSummary, WorkflowRun, Workstream } from "../../types";
import type { IconName } from "../../ui/icons";
import type { Route } from "../../router";

export type Panel = "details" | "work" | "projects" | "files";

export declare const PANELS: readonly Panel[];
export declare function panelOf(raw: unknown): Panel;
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
