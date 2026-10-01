import type { AgentDef, Effort, ModelInfo, Step } from "../../../types";

type AgentStep = Extract<Step, { kind: "agent" }>;

export declare function stepHarnesses(
  step: Pick<AgentStep, "harness" | "assignee"> | null | undefined,
  agents: readonly Pick<AgentDef, "id" | "harness">[] | null | undefined,
): string[];

export declare function stepEffortHint(
  step: { effort?: unknown } | null | undefined,
  facts?: { available?: readonly Effort[]; known?: boolean },
): string;

/** What a harness answered when asked for its models. */
export interface HarnessAnswer {
  harness: string;
  efforts: Effort[];
  models: ModelInfo[];
}

export declare function harnessesFrom(text: unknown): string[];
export declare function harnessText(step: Pick<AgentStep, "harness"> | null | undefined): string;
export declare function stepEfforts(answers: readonly (HarnessAnswer | null | undefined)[] | null | undefined, model: string | null | undefined): { available: Effort[]; known: boolean };
export declare function answersFor<A>(asked: string, read: { asked: string; answers: A[] } | null | undefined): A[];
