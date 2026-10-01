/** Types for `conditionModel.mjs`. */
import type { Condition, InputDef, Step } from "../../../types";

type Group = Extract<Condition, { condition: "all" | "any" | "one" }>;
type Kind = Condition["condition"];

export declare function isGroup(condition: Condition | null | undefined): condition is Group;
export declare function freshCondition(upstream: readonly Step[], inputs: readonly InputDef[]): Condition;
export declare function offeredWith(kind: Kind, upstream: readonly Step[], inputs: readonly InputDef[]): boolean;
export declare function offeredKinds<K extends { condition: string; label: string }>(kinds: readonly K[], current: Kind, depth: number, upstream: readonly Step[], inputs: readonly InputDef[]): K[];
export declare function mayNest(depth: number): boolean;
export declare function conditionOf(kind: Kind, upstream: readonly Step[], inputs: readonly InputDef[], current: Condition): Condition;
export declare function parseValue(text: string): unknown;
export declare function valueText(value: unknown): string;
export declare function hourOf(text: string): number;
