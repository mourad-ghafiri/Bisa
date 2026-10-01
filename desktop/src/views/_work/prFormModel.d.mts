import type { CheckRun, CodeHostCapabilities, MergeStrategy } from "../../types";

export function controlsFor(caps: CodeHostCapabilities | null | undefined): string[];
export function mergeStrategies(caps: CodeHostCapabilities | null | undefined): MergeStrategy[];
export function defaultStrategy(caps: CodeHostCapabilities | null | undefined, preferred?: string | null): MergeStrategy | null;
export function prActionLabel(state: string, noun?: string): string;
export function splitHandles(text: string): string[];
export function prRequest(
  caps: CodeHostCapabilities | null | undefined,
  form: { title: string; body: string; draft: boolean; reviewers: string; labels: string },
): { title: string; body?: string; draft?: boolean; reviewers?: string[]; labels?: string[] };
export function checksSummary(checks: CheckRun[] | null | undefined): { text: string; tone: "ok" | "danger" | "dim" };
