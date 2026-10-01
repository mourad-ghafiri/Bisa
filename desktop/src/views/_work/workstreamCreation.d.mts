export declare const BRANCH_SLUG_CHARS: number;
export declare const TAIL_PLACEHOLDER: string;
export type SourceKind = "new" | "branch" | "remote" | "tag" | "pr";
export declare const SOURCES: ReadonlyArray<{ id: SourceKind; label: string; hint: string }>;
export type OpenCheck =
  | { ok: true; mode: "branch" | "copy" }
  | { ok: false; reason: "loading" | "missing" | "unborn"; remedy: string | null };
export declare function canOpenWorkstream(
  project: { exists: boolean } | null | undefined,
  gitStatus: { git: boolean; exists: boolean; head?: string | null } | null | undefined,
): OpenCheck;
export declare function sanitizeRefComponent(s: string | null | undefined, fallback: string): string;
export declare function branchNameFor(kind: string, slug: string): string;
export declare function typedBranchName(typed: string | null | undefined): string;
/** What the dialog holds for a source: the fields of that source only. */
export type SourceFields =
  | { kind: "new"; name?: string; start?: string }
  | { kind: "branch"; branch?: string }
  | { kind: "remote"; remote?: string; branch?: string }
  | { kind: "tag"; tag?: string; newTag?: boolean; tagName?: string; tagAt?: string; branch?: string }
  | { kind: "pr"; pr?: number | null; head?: string };
/** What a door presets: a branch row, a tag row, a ref chip, a pull request. */
export type SourcePreset =
  | { kind: "new"; start?: string }
  | { kind: "branch"; branch: string }
  | { kind: "remote"; remote: string; branch: string }
  | { kind: "tag"; tag: string }
  | { kind: "pr"; pr: number };
export declare function previewBranch(input: { label?: string; project?: string; source?: SourceFields }): { branch: string; derived: boolean };
export declare function sourceBody(f: SourceFields): { source: import("../../types").WorkstreamSource; problem: null } | { source: null; problem: string };
/** Whether the dialog shows the label's field: a copy, or a new branch. */
export declare function labelShown(git: boolean, f: SourceFields): boolean;
/** The body of `POST /projects/{pid}/workstreams` for what the dialog holds, or the one problem with it. */
export declare function openBody(form: { git: boolean; label?: string; fields: SourceFields; base?: string; defaultBranch?: string | null }): { body: import("../../types").NewWorkstreamBody; problem: null } | { body: null; problem: string };
export declare function takenBranches(
  workstreams: ReadonlyArray<{ name?: string | null; kind: { kind: string; branch?: string }; state: { state: string } }>,
  primaryBranch: string | null | undefined,
): Map<string, string>;
export declare function fieldsOf(preset: SourcePreset | null | undefined): SourceFields;
export declare function openWords(f: SourceFields | null, branch: string): string;
