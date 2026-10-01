/**
 * Types for `railStyleModel.mjs`, plain JavaScript so `node --test` reads it
 * without a build step.
 */

export type RailAttention = "none" | "accent" | "danger";
export type RailRowKind = "group" | "goal" | "project" | "workstream" | "terminal" | "agent";

export interface RailTreatment {
  wash: "current" | "rest";
  pill: RailAttention;
  ink: "current" | "strong" | "plain" | "dim";
  muted: boolean;
}

export declare const ATTENTIONS: readonly RailAttention[];
export declare function rowTreatment(row: { kind: RailRowKind; current?: boolean; attention?: RailAttention; archived?: boolean; exists?: boolean }): RailTreatment;
export declare const PILL_LEFT_PX: number;
export declare function pillClearance(radiusPx: number): number;
export declare function pillInset(radiusPx: number): number;
export declare function headingSpacing(first: boolean): "none" | "section";
export declare function avatarSize(kind: "project" | "group" | "goal"): number;
