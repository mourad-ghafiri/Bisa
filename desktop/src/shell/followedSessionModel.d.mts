import type { SessionRow } from "../types";

export function chosenSession(chosen: string | null | undefined, sessions: readonly SessionRow[], workstream: string): SessionRow | null;
export function fallbackSession(sessions: readonly SessionRow[], workstream: string): SessionRow | null;
export function followedSession(chosen: string | null | undefined, sessions: readonly SessionRow[], workstream: string | null | undefined): SessionRow | null;
export function terminalSessionOf(terminals: readonly { key: string; sessionId: string | null }[], doc: string | null | undefined): string | null;
export function followWords(row: Pick<SessionRow, "agent" | "harness">): string;
