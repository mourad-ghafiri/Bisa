export type MarkId = "claude-code" | "codex" | "pi" | "omp" | "opencode" | "copilot" | "grok" | "gemini" | "muse-code" | "goose" | "cursor-agent" | "acp" | "custom";

export declare const MARK_IDS: readonly MarkId[];
export declare function markIdOf(id: string | null | undefined): MarkId | null;
