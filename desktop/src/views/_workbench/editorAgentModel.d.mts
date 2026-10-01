import type { AgentDef, ContextRef } from "../../types";

export interface ToolbarAnchors {
  start: { top: number; left: number } | null;
  end: { bottom: number; left: number } | null;
}

export declare const GENERAL_AGENT: string;
export declare function rangeLabel(path: string, start: number, end: number): string;
export declare function editContent(instruction: string, target: string): string;
export declare function askContent(question: string, target: string): string;
export declare function messageBody(input: {
  mode: "ask" | "edit";
  agentId: string;
  text: string;
  target: string;
  chips: readonly ContextRef[];
}): { content: string; mentions: string[]; context: ContextRef[] };
export declare function toolbarPlacement(input: {
  anchors: ToolbarAnchors;
  box: { width: number; height: number };
  bar: { width: number; height: number };
}): { top: number; left: number; above: boolean } | null;
export declare function agentChoices(
  agents: readonly AgentDef[],
  remembered: string | null | undefined,
  fallback: { id: string; name: string } | null,
): AgentDef[];
/** The wanted agent when it is among the choices, else the first choice, else null. */
export declare function chosenAgent<T extends { id: string }>(choices: readonly T[], wanted: string | null | undefined): T | null;
export declare function sentWords(mode: "ask" | "edit", agentName: string): string;
