/**
 * Types for `footerSessionsModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

import type { ProjectRow, SessionRow, SessionState, WorkstreamRef } from "../types";
import type { Liveness, TerminalSessionState } from "./terminalsModel.mjs";

type TerminalScope = TerminalSessionState["scope"];

/** One lookup for where a session stands, from the workspace index. */
export interface PlaceIndex {
  workstreams: Map<string, { project: string | null; projectId: string | null; workstream: string }>;
  /** A project's name by id. */
  projects: Map<string, string>;
  /** A project by slug — what the data directory files it under. */
  projectSlugs: Map<string, { id: string; name: string }>;
  /** A goal's label and the workflow its run follows. */
  goals: Map<string, { label: string; workflow: string | null; workflowName: string | null }>;
}

export interface GoalPlace {
  id: string;
  label: string;
  workflow?: string | null;
  workflowName?: string | null;
}

export interface FooterTerminalRow {
  key: string;
  scope: TerminalScope;
  id: string;
  harness: string | null;
  liveness: Liveness;
  open: boolean;
  word: string;
  /** Where the tab is rooted, compactly — *Bisa › feat/a*. */
  place: string;
}

export interface FooterHarnessRow {
  id: string;
  /** The session's kind — a worker, a design wake, a one-shot ask, a terminal a tab claims. */
  kind: string;
  agent: string | null;
  harness: string | null;
  workstream: string | null;
  state: SessionState;
  /** When the session was registered, for how long it has stood. */
  started: number | null;
  /** The tab that claims this session, when a person opened it in a terminal. */
  terminalKey: string | null;
  /** Where the session stands — its workstream's place, its project, its folder, or *not in a checkout*. */
  place: string;
  /** The roster row itself, for the words the footer puts on it (`sessionOriginModel.originOf`). */
  row: SessionRow;
}

export declare function isOpen(liveness: Liveness | null | undefined): boolean;
export declare function placeIndex(ws: { workstreams?: readonly WorkstreamRef[]; projects?: readonly ProjectRow[]; goals?: readonly GoalPlace[] }): PlaceIndex;
export declare function emptyPlaceIndex(): PlaceIndex;
export declare function placeWords(scope: string, id: string, index: PlaceIndex): string;
export declare function footerSessions(
  terminals: readonly TerminalSessionState[] | null | undefined,
  sessions: readonly SessionRow[] | null | undefined,
  index?: PlaceIndex,
): { terminals: FooterTerminalRow[]; harnesses: FooterHarnessRow[]; openTerminals: number };
