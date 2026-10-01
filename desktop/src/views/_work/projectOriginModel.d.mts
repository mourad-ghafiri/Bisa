import type { ProjectOrigin, ProjectRow } from "../../types";

export interface StepRefLike {
  run: string;
  step: string;
  workflow: string;
}

/** A row holding a project, or a project itself. */
export type ProjectLike = ProjectRow | { origin: ProjectOrigin };

export declare function stepOf(origin: ProjectOrigin | null | undefined): StepRefLike | null;
export declare function madeByAgentStep(origin: ProjectOrigin | null | undefined): boolean;
export declare function madeByGoal(origin: ProjectOrigin | null | undefined, goal: string): boolean;
export declare function bornWords(origin: ProjectOrigin | null | undefined): string;
export declare function madeByStepWords(origin: Extract<ProjectOrigin, { origin: "step" }>, goalTitle: (goal: string) => string): string;
export declare function projectsMadeByStep<T extends ProjectLike>(projects: readonly T[] | null | undefined, run: string, stepId: string): T[];
export declare function projectsMadeByWorkflow<T extends ProjectLike>(projects: readonly T[] | null | undefined, workflow: string): T[];
