/** Types for `spawnStepModel.mjs`. */
import type { InputDef, Step } from "../../../types";
import type { MappingRow } from "./startForm.mjs";

type Spawn = Extract<Step, { kind: "spawn" }>;

export declare function givenRows(step: Pick<Spawn, "inputs"> | null | undefined, asked: readonly InputDef[]): MappingRow[];
export declare function give(step: Spawn, input: string, template: string | null): Spawn;
export declare function notAskedFor(step: Pick<Spawn, "inputs"> | null | undefined, asked: readonly InputDef[]): string[];
export declare function leftOut(step: Pick<Spawn, "inputs"> | null | undefined, asked: readonly InputDef[]): string[];
/** The step on another workflow; `asked` is `null` while that workflow's inputs are not known. */
export declare function onWorkflow(step: Spawn, workflow: string | null, asked: readonly InputDef[] | null): Spawn;
