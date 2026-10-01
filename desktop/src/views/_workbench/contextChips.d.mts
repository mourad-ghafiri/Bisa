import type { ContextRef } from "../../types";

export const MAX_CONTEXT_BYTES: number;
export function fileChip(path: string): ContextRef;
export function selectionChip(path: string, start: number, end: number, text: string, maxLines?: number): ContextRef;
export function hunkChip(path: string, staged: boolean, hunkText: string, id: string): ContextRef;
export function terminalChip(session: string, lines: string[], maxLines?: number): ContextRef;
export function workItemChip(id: string): ContextRef;
export function commitChip(id: string): ContextRef;
export type PageRef = { kind: "file"; path: string } | { kind: "url"; url: string };
export function filePage(path: string): PageRef;
export function urlPage(url: string): PageRef;
export function pageWords(page: PageRef): string;
export function annotationChip(page: PageRef, selector: string, excerpt: string, note: string): ContextRef;
export function chipLabel(ref: ContextRef): string;
export function sameChip(a: ContextRef, b: ContextRef): boolean;
export function attach(refs: ContextRef[], ref: ContextRef): ContextRef[];
export function contextBytes(refs: ContextRef[]): number;
export function fitsBudget(refs: ContextRef[]): boolean;
export type FrameChip =
  | { kind: "project"; label: string }
  | { kind: "goal"; label: string; id: string };
export function frameChips(project: { name: string; slug: string } | null, goals: { id: string; label: string }[]): FrameChip[];
