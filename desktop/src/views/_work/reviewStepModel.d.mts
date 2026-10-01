import type { CodeHostCapabilities, PullRequest, ReviewSummary, ReviewThread, ReviewThreadComment, SessionRow, SessionState } from "../../types";

export type ReviewEvent = "approve" | "request_changes" | "comment";
export type Verdict = "approved" | "changes_requested" | "commented" | "none";
export type Reviewer = { kind: "agent"; agent: string } | { kind: "person"; login: string | null };
export interface ReviewFacts {
  verdict: Verdict;
  /** The latest review an agent gave, and which agent. */
  agent: { review: ReviewSummary; agent: string } | null;
  /** The latest review the connected account gave in person. */
  yours: ReviewSummary | null;
  /** The latest review of every other account, in the order given. */
  others: ReviewSummary[];
  /** Comments still open — the code host's resolvable ones; 0 where it exposes none. */
  openComments: number;
  /** Whether any review stands — an agent's, yours, or another account's. The review is optional either way. */
  given: boolean;
}

export declare const AGENT_REVIEW_MARK: string;
export declare const AGENT_REPLY_MARK: string;
export declare const EVENTS: readonly ReviewEvent[];
export declare function reviewerOf(review: Pick<ReviewSummary, "author" | "body">): Reviewer;
export declare function reviewWords(review: Pick<ReviewSummary, "author" | "body">): string;
/** Who left a reply on a comment: the agent the engine signed it with, else the person. */
export declare function replierOf(reply: Pick<ReviewThreadComment, "author" | "body">): Reviewer;
/** A reply's words without the engine's signature line. */
export declare function replyWords(reply: Pick<ReviewThreadComment, "author" | "body">): string;
/** A reply's author as a row names it: the agent's id, the person's login, *someone*. */
export declare function replierName(reply: Pick<ReviewThreadComment, "author" | "body">): string;
export declare function reviewVerdict(reviews: readonly ReviewSummary[] | null | undefined): Verdict;
export declare function reviewFacts(facts: {
  reviews: readonly ReviewSummary[] | null | undefined;
  viewer: string | null | undefined;
  /** The code host's resolvable comments — its *threads* on the wire. */
  comments: readonly ReviewThread[] | null | undefined;
  caps: CodeHostCapabilities | null | undefined;
}): ReviewFacts;
export declare function stepNote(facts: ReviewFacts): string;
export declare function statusLine(facts: ReviewFacts): string;
export declare function verdictWords(review: Pick<ReviewSummary, "state">): string;
export declare function allowedEvents(
  pr: Pick<PullRequest, "author"> | null | undefined,
  viewer: string | null | undefined,
  caps?: Pick<CodeHostCapabilities, "review_events"> | null,
): ReviewEvent[];

/** How a run ended: its session finished, was aborted elsewhere, failed, was stopped from the step, its review landed, or no session ever showed. */
export type RunEnd = "done" | "aborted" | "failed" | "stopped" | "landed" | "gone";
export type RunKind = "review" | "fix" | "branch" | "check";
/** What the step asked an agent for, when (unix seconds), which agent, and — once seen — how it ended. One run at a time in a checkout. */
export interface ReviewRun {
  kind: RunKind;
  agent: string;
  at: number;
  /** The conversation the ask went into — where the reply is read from. */
  conversation?: string;
  /** Commits beyond the base at the ask, so a fix can say how many it added. */
  ahead?: number;
  /** A fix: how many comments were handed over. */
  count?: number;
  /** A fix of one comment: its id, so its row can say so. */
  comment?: string;
  /** A check fix: the failed run's name, so its row can say so. */
  check?: string;
  /** Recorded by the step the first time it saw the run end, so the outcome outlives the roster. */
  ended?: { at: number; how: RunEnd; reason?: string };
}
/** The run's session as the line draws it. */
export interface RunSession {
  id: string;
  state: SessionState;
  started: number;
  /** When the session entered its current state. */
  since: number;
  subagents: number;
}
export interface RunState {
  run: ReviewRun;
  session: RunSession | null;
  /** No session yet, within the grace. */
  starting: boolean;
  /** The session runs or waits on the person (`isStoppable`) and the run has not ended: Stop is offered. */
  live: boolean;
  done: boolean;
  how: RunEnd | null;
  /** A failure's reason, when the session carried one. */
  reason: string | null;
}
export declare function reviewRun(
  sessions: readonly Pick<SessionRow, "id" | "agent" | "workstream" | "state" | "started" | "since" | "children">[],
  wid: string,
  run: ReviewRun | null | undefined,
  facts: ReviewFacts | null | undefined,
  replied?: boolean,
  now?: number,
): RunState | null;
export declare function activityWords(session: Pick<RunSession, "state" | "subagents"> | null | undefined): string;
export declare function outcomeWords(run: ReviewRun, how: RunEnd, aheadNow?: number | null, reason?: string | null): string | null;
export declare function agentReply<M extends { author: string; content: string; created_at: number; retracted?: boolean }>(
  messages: readonly M[] | null | undefined,
  agentOf: (author: string) => string | null | undefined,
  run: ReviewRun | null | undefined,
): M | null;
export declare function clearsOnLanding(run: ReviewRun, how: RunEnd | null): boolean;
export declare function endWord(how: RunEnd): "done" | "aborted" | "failed";
export declare function runWords(run: ReviewRun): string;
/** A run that edits the checkout — a comment fix or a check fix. */
export declare function isFix(run: ReviewRun): boolean;
/** Why nothing else can be asked now — who is busy and on what — or null when nothing runs. */
export declare function busyWords(state: Pick<RunState, "run" | "done"> | null | undefined, where?: string | null): string | null;
export declare function ownPrNote(pr: Pick<PullRequest, "author"> | null | undefined, viewer: string | null | undefined): string | null;
export declare function needsWords(event: ReviewEvent, body: string | null | undefined): boolean;
export declare function submittedWords(event: ReviewEvent): string;
export declare function commentCount(n: number): string;
export declare function commentRow(comment: ReviewThread): { where: string; author: string; lead: string; replies: number };
export declare function groupWords(group: { path: string; comments: readonly ReviewThread[] }): { path: string; count: string };
export declare function commentsSummary(comments: readonly ReviewThread[] | null | undefined): string;
export declare function unixOf(iso: string | null | undefined): number | null;
export declare function reviewKey(review: ReviewSummary, index: number): string;
