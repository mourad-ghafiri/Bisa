/** Types for `agentDraftModel.mjs`. */
import type { AgentDef, AttachmentRef, ModelPlan } from "../../types";

/** An agent as the editor holds it. */
export interface AgentDraft {
  name: string;
  /** The agent's picture — an attachment by content hash (ide/14 §Photos). */
  photo: AttachmentRef | null;
  description: string;
  system_prompt: string;
  /** The harness that runs it — `null` while none is picked: a machine with none installed. */
  harness: string | null;
  /** Ordered list plus the strategy for choosing among it. */
  plan: ModelPlan;
  /** Let the Decision-Making Agent decide for this agent — one of the three things a core agent may change. */
  decision_making: boolean;
  respond: "owner_only" | "members";
  /** Library and registry **ids**. Order is the order a launch delivers them. */
  skills: string[];
  mcps: string[];
  tags: string[];
}

/** The body of a new agent: what it has, and nothing it lacks. */
export interface NewAgentBody {
  name: string;
  photo?: AttachmentRef;
  system_prompt: string;
  harness: string;
  models?: ModelPlan;
  description?: string;
  decision_making?: boolean;
  respond: "owner_only" | "members";
  skills: string[];
  mcps: string[];
  tags: string[];
}

/** What a save sends: a patch of the agent, or the creation of a new one. */
export type AgentSave = { kind: "patch"; id: string; body: Record<string, unknown> } | { kind: "create"; body: NewAgentBody };

/** What a core agent may change, in the order the body names them. */
export declare const CORE_FIELDS: readonly string[];
export declare function emptyDraft(harness?: string | null): AgentDraft;
/** The harness a new agent starts on: the first installed here, or null. */
export declare function startingHarness(harnesses: readonly { id: string; installed?: boolean }[] | null | undefined): string | null;
/** What the harness picker offers: the installed ones, and the draft's own when it is not among them. */
export declare function harnessChoices(harnesses: readonly { id: string; label: string; installed?: boolean }[] | null | undefined, current: string | null | undefined): { id: string; label: string; installed: boolean }[];
export declare function maySaveAgent(d: Pick<AgentDraft, "name" | "system_prompt" | "harness">, busy: boolean): boolean;
export declare function draftOf(agent: AgentDef): AgentDraft;
export declare function saveOf(d: AgentDraft, agent: Pick<AgentDef, "id"> | null, core: boolean): AgentSave;
