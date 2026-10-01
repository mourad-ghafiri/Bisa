/**
 * Types for `projectForm.mjs`, which is plain JavaScript so `node --test` can
 * import it without a build step. This file is the only reason TypeScript
 * never has to read it.
 */

import type { ImportStats, NewProjectBody, PublishPolicy } from "../../types";

/** Every wire `kind` the create route accepts. */
export type WireKind = NewProjectBody["kind"];

/**
 * The wire kinds that *bring a project into existence*. `link` is not one:
 * nothing is created, so nothing here can return it.
 */
export type ArrivalKind = Exclude<WireKind, "link">;

/** The three ways a project comes into existence, as the dialog offers them. */
export type Provenance = "new" | "clone" | "import";

/** Where an imported folder ends up living. */
export type Placement = "copy" | "link";

/** Which input a field-level error belongs beside. */
export type ProjectField = "slug" | "name" | "url" | "path" | "form";

export declare const PROVENANCE: readonly Provenance[];
export declare const PLACEMENT: readonly Placement[];

/**
 * Overloaded so the import case keeps its narrow answer: the caller builds a
 * body with a `path`, and `"new" | "clone"` is not a shape that has one.
 */
export declare function bodyKind(
  provenance: "import",
  placement: Placement,
): "import" | "adopt";
export declare function bodyKind(provenance: Provenance, placement: Placement): ArrivalKind;
export declare function slugFromName(name: string | null | undefined): string;
export declare function nameFromPath(from: string | null | undefined): string;
/** The input each refusal of a creation is about, by the refusal's message id (`ErrorBody.text.id`) — never by its words. */
export declare const FIELD_OF_REFUSAL: Readonly<Record<string, Exclude<ProjectField, "form">>>;
/** The input a refusal is about, by its id; the form's when the table holds none, or there is no id. */
export declare function fieldFor(refusal: string | null | undefined): ProjectField;
/** The inputs the dialog shows for the way in. */
export declare function shownFields(provenance: Provenance): ProjectField[];
/** What a failed creation is: the node's sentence, the id it travels as, and the answer's status when there was one. */
export interface CreationFailure {
  message: string;
  refusal?: string | null;
  status?: number | null;
}
/** Where the node's refusal is said: beside the input it is about when the form shows it, else on the form. */
export declare function refusalPlace(failure: CreationFailure, provenance: Provenance): Partial<Record<ProjectField, string>>;
/** The commit button's words: what is about to happen. */
export declare function primaryLabel(provenance: Provenance, placement: Placement): string;
/** The account select's first row: the host's own choice, named when the node suggested one. */
export declare function hostChoiceWords(suggested: string | null | undefined): string;
export type DialogMode = "create" | "import";
export declare function provenancesFor(mode: DialogMode): Provenance[];
/** The sentence under Publishing when *gated* is chosen and no door fixed a goal to ask — null otherwise. */
export declare function publishCaveat(publish: string, hasGoal: boolean): string | null;
export declare function importSummary(stats: ImportStats | null | undefined): string | null;
/** The body of `POST /projects` for what the dialog holds — the kind's own keys by name. */
export declare function creationBody(form: {
  provenance: Provenance;
  placement: Placement;
  slug: string;
  name: string;
  publish: PublishPolicy;
  tags: readonly string[];
  gitConfig: Record<string, string> | null;
  url: string;
  path: string;
}): Exclude<NewProjectBody, { kind: "attach" }>;
/** The workspace's `git.committer`, in the registry's words. */
export type CommitterPolicy = "inherit" | "pin" | "ask";
export declare const COMMITTER_POLICIES: readonly CommitterPolicy[];
/** How the dialog shows the repository's git config: a footnote, or the identity fields open. */
export type GitConfigSection = "note" | "open";
export declare function gitConfigSection(policy: CommitterPolicy, globalResolved: boolean): GitConfigSection;
export declare function committerNote(policy: CommitterPolicy, ident: string): string;
/** The `git_config` a creation body carries — every key set, or nothing. */
export declare function creationGitConfig(write: { set: Record<string, string>; unset: string[] }): Record<string, string> | null;
