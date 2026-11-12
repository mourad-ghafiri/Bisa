import type { AgentDef, Assignee, AttachmentRef, SessionRow, TeamDef } from "../types";

export declare const CORE_AGENT_IDS: readonly string[];
export declare function respondsTo(agent: Pick<AgentDef, "respond"> | null | undefined): string;
/** Where a session's ask is answered — the Inbox row it lives under — or `null` when nothing waits on you. */
export declare function answerOf(
  session: Pick<SessionRow, "state" | "goal" | "conversation" | "workstream"> | null | undefined,
): { item: string | null } | null;
/** Whether the detail column sits under the roster (the narrow layout) rather than beside it. */
export declare function detailStacked(roster: { right: number } | null | undefined, detail: { left: number } | null | undefined): boolean;
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
/** The ids a roster picker never offers: the two core agents. */
export declare function rosterNotOffered(agents: readonly Pick<AgentDef, "id" | "origin">[] | null | undefined): string[];
/** The line under an agent in a picker, saying first that it is disabled when it is. */
export declare function agentOptionLine(agent: Pick<AgentDef, "enabled"> | null | undefined, line: string): string;
/** The one name of a member the workspace no longer has. */
export declare function goneMemberName(kind: "agent" | "human" | "team", id: string): string;
/** Whether a delete was asked of this record — the ask is the id it was made of. */
export declare function asksDelete(asked: string | null | undefined, record: { id: string } | null | undefined): boolean;
/** The team a pane draws: the list's copy, or what the node answered a write of this pane since. */
export declare function shownTeam(listed: TeamDef, answered: TeamDef | null | undefined): TeamDef;
/** Where a waiting session's *Answer* goes: its Inbox row for a gate, where it was asked otherwise. */
export declare function answerPlace<D>(
  session: Pick<SessionRow, "state" | "goal" | "conversation" | "workstream"> | null | undefined,
  door: D | null | undefined,
): { inbox: string | null } | { door: D } | null;
/** The sessions an abort is still on its way for. */
export declare function stillStopping(asked: readonly string[], stoppable: readonly { id: string }[]): string[];
/** Whether a conversation frame says a definition's record moved. */
export declare function definitionMoved(frame: { snapshot?: boolean } | null | undefined): boolean;
/** Whether a conversation frame says a record was removed. */
export declare function definitionRemoved(frame: { snapshot?: boolean; removed?: boolean } | null | undefined): boolean;
/** Whether an engine fact moves the MCP registry a screen holds. */
export declare function mcpMoved(payload: { type?: string } | null | undefined): boolean;
/** One window's word that the MCP registry changed here. */
export declare function registryChanges(): { changed: () => void; listen: (heard: () => void) => () => void };
/** The catalog rows in flight, with one more or one fewer. */
export declare function busyRows(busy: readonly string[], key: string, on: boolean): string[];
/** The switch a record offers — its words and the one-field body it sends — or `null` for a core agent. */
export declare function switchOf(
  kind: "agent" | "team",
  record: { id: string; name?: string; enabled?: boolean; origin?: unknown } | null | undefined,
): { on: boolean; label: string; doing: string; done: string; body: { enabled: boolean } } | null;
