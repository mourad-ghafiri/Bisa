import type { AgentDef, Assignee, AttachmentRef, SessionRow, TeamDef } from "../types";

export declare const CORE_AGENT_IDS: readonly string[];
export declare function respondsTo(agent: Pick<AgentDef, "respond"> | null | undefined): string;
export declare function attachedTo(
  session: Pick<SessionRow, "goal" | "run" | "work_item" | "conversation" | "kind"> | null | undefined,
): { label: string; route: { name: "goal" | "run" | "conversation"; id: string } | null };
export declare const isImplicitMember: (m: Assignee) => boolean;
export declare function storedMembers(team: Pick<TeamDef, "members"> | null | undefined): Assignee[];
export declare function rosterLine(members: readonly Assignee[] | null | undefined): string;
export declare function withoutMember(team: Pick<TeamDef, "members">, member: Assignee): { members: Assignee[] };
export declare function memberFace(
  m: Assignee,
  agents: readonly Pick<AgentDef, "id" | "name" | "pubkey" | "photo">[],
  people: { nameOf: (pubkey: string) => string; photoOf: (pubkey: string) => AttachmentRef | null },
): { label: string; avatarId: string; photo: AttachmentRef | null };

/** Whether a team may be addressed and assigned — false once it is stood down. */
export declare function teamTakesWork(team: Pick<TeamDef, "enabled"> | null | undefined): boolean;
/** The line under a team in a picker: its roster, or that it was stood down. */
export declare function teamOptionLine(team: Pick<TeamDef, "members" | "enabled">): string;
export declare const KIND_TEAM: 33408;
export declare const KIND_SKILL: 33411;
/** Whether a conversation frame says a skill of the library moved. */
export declare function skillMoved(frame: { kind?: number; snapshot?: boolean } | null | undefined): boolean;
/** Whether a conversation frame says a team's record moved. */
export declare function teamMoved(frame: { kind?: number; snapshot?: boolean } | null | undefined): boolean;
/** What a roster screen says of an id its list does not hold, from the single read of that record. */
export declare function absentRecord(read: { data?: unknown; error?: string | null; missing?: boolean; loading?: boolean }): { state: "reading" } | { state: "here" } | { state: "gone" } | { state: "unreadable"; sentence: string };
