/** Types for `workflowCardModel.mjs`. */
import type { WorkflowRow } from "../../types";
import type { WorkflowVerbs } from "./workflowVerbs.mjs";

export type CardTone = "quiet" | "accent" | "danger" | "ok";
export interface CardStatus {
  tone: CardTone;
  /** A name in `ICON`. */
  icon: string;
  words: string;
  /** A run of it goes — in the workspace, or a goal's: the dot pulses. */
  live: boolean;
}
/** The mark beside the state line while it listens: *On*, or *Paused*. */
export interface OnMark {
  words: string;
  tone: "ok" | "warn";
}
export type CardMenuId = "open" | "run" | "turn_on" | "turn_off" | "restart" | "stop" | "delete";
export interface CardMenuItem {
  id: CardMenuId;
  label: string;
  danger?: boolean;
  separatorBefore?: boolean;
}
export declare function cardStatus(row: WorkflowRow): CardStatus;
export declare function onMark(row: WorkflowRow | null | undefined): OnMark | null;
export declare function cardMeta(row: WorkflowRow): string[];
export declare function cardMenu(verbs: WorkflowVerbs): CardMenuItem[];
