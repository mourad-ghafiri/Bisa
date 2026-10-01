/**
 * The one owner of the Review step's agent run (ide/08): what was asked, the
 * session it became, how it ended, and the agent's reply — followed **in the
 * step**, never by leaving it for another panel.
 *
 * The run is the checkout's session draft (`draftKeys(scope).run`), so a tab
 * switch keeps it. Its session is found on the roster (`useSessions`,
 * bus-driven) by `reviewRun`; the first time the run is seen to end, its end
 * is **recorded into the draft**, so the outcome outlives the roster
 * dropping the session. A pull request's review that lands clears its run
 * instead: the review row is the outcome. The reply is the agent's latest
 * message in the checkout's conversation after the ask, read when a run exists
 * and on every conversation frame, its author named through the workspace's
 * pubkey map.
 *
 * The owner hears when the agent **moved** something — `onMoved("replied")`
 * once per new reply, `onMoved("ended")` once at the end — and reads the
 * code host and the checkout again: a review may have landed, a comment may
 * have been resolved, commits may be on the branch. The lifecycle owns the
 * run for exactly that reason: it owns the reads the run must move.
 *
 * One run at a time: `ask` refuses while one has not ended; `stop` aborts a
 * session that runs or waits and records *stopped*; `dismiss` clears an
 * ended run. A session idle between turns ends the run once its reply is in
 * the thread (or after the grace with none), so a finished agent never holds
 * the step.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../../api";
import { errorFields, log } from "../../log";
import { useConversationEvents } from "../../bus";
import { useReloadOnReconnect } from "../../ui/useReloadOnReconnect";
import { stopSession, useSessions } from "../../shell/sessionsStore";
import { useWorkspace } from "../../shell/useWorkspaceData";
import type { MessageRow } from "../../types";
import { useToast } from "../../ui";
import { draftKeys } from "./agentReviewModel.mjs";
import { useSessionDraft } from "./gitPanelStore";
import { agentReply, clearsOnLanding, reviewRun } from "./reviewStepModel.mjs";
import { ensureConversation } from "../_workbench/conversationsStore";
import type { ReviewFacts, ReviewRun, RunKind, RunState } from "./reviewStepModel.mjs";
import { attempt } from "./useAsync";
import { t } from "../../i18n/l10n.mjs";

/** What a fix run carries beyond the ask: how many comments, which one when it is one, or the failed check's name. */
export interface AskExtras {
  count?: number;
  comment?: string;
  check?: string;
}

export interface ReviewRunControl {
  /** The run as drafted, or none. */
  run: ReviewRun | null;
  /** The run against the roster — its session, whether it is starting, live, or how it ended. */
  state: RunState | null;
  /** The agent's latest reply after the ask, from the checkout's conversation. */
  reply: MessageRow | null;
  /** A request is on its way to the thread. */
  asking: boolean;
  /** A run is starting or live: nothing else can be asked. */
  busy: boolean;
  /** Post the request as a mention and record the run. Resolves to whether it went. */
  ask: (kind: RunKind, agent: string, content: string, extras?: AskExtras) => Promise<boolean>;
  /** Stop the agent mid-run — an action, then the same or another agent can be asked again. */
  stop: () => void;
  /** Clear an ended run's outcome from the step. */
  dismiss: () => void;
}

const now = () => Math.floor(Date.now() / 1000);

/** The newest page of the thread is plenty: a reply is the last thing the agent said. */
const REPLY_PAGE = 20;

/** Why the agent's side moved: it replied in the thread, or its run ended. */
export type RunMove = "replied" | "ended";

export function useReviewRun({
  wid,
  pid,
  scope,
  facts,
  aheadOfBase,
  onMoved,
}: {
  wid: string;
  /** The checkout's project — a review is asked in a conversation about the checkout, which needs it. */
  pid: string | null;
  /** The checkout's session scope (`rootKey("workstream", wid)`) — where the run is kept. */
  scope: string;
  /** Who has reviewed — a pull request's review landing ends a review run. */
  facts: ReviewFacts | null;
  /** Commits beyond the base now — remembered at the ask so a fix can say how many it added. */
  aheadOfBase: number | null;
  /** The agent moved something — a reply landed, the run ended: the code host and the checkout may have changed. */
  onMoved?: (why: RunMove) => void;
}): ReviewRunControl {
  const toast = useToast();
  const ws = useWorkspace();
  const keys = draftKeys(scope);
  const [run, setRun] = useSessionDraft<ReviewRun | null>(keys.run, null);
  const sessions = useSessions();

  // The reply: the conversation's newest page while a run exists, re-read on every conversation frame.
  const [messages, setMessages] = useState<readonly MessageRow[] | null>(null);
  const at = run?.at ?? null;
  const conversation = run?.conversation ?? null;
  const read = useCallback(() => {
    if (at === null || conversation === null) return;
    api
      .conversationMessages(conversation, undefined, REPLY_PAGE)
      .then((page) => setMessages(page.messages))
      .catch((e: unknown) => {
        // A missed page is not worth a word: the next frame reads again.
        log.debug("review", "the reply page could not be read; the next frame reads again", { conversation, ...errorFields(e) });
      });
  }, [conversation, at]);
  useEffect(() => {
    if (at === null) setMessages(null);
    else read();
  }, [at, read]);
  useConversationEvents(read);
  useReloadOnReconnect(read);
  const reply = useMemo(() => agentReply(messages, (author) => ws.agentByPubkey(author)?.id ?? null, run), [messages, ws, run]);

  // The run against the roster and the thread: its session, whether Stop is
  // offered (the session runs or waits — never idle), and how it ended.
  const state = reviewRun(sessions, wid, run, facts, reply !== null);
  const how = state?.how ?? null;
  const recorded = state?.run.ended !== undefined;

  // The end, once seen, is the run's to keep — and the owner's to read after.
  useEffect(() => {
    if (!state || how === null || recorded) return;
    if (clearsOnLanding(state.run, how)) setRun(null);
    else setRun({ ...state.run, ended: { at: now(), how, ...(state.reason ? { reason: state.reason } : {}) } });
    onMoved?.("ended");
    // Written once, the moment `how` is decided: `state` is remade every
    // render and `onMoved` is the owner's callback.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [how, recorded]);

  // A new reply is the agent having done something: the owner reads again, once per reply.
  const lastReply = useRef<string | null>(null);
  const replyId = reply?.id ?? null;
  useEffect(() => {
    if (replyId === null || replyId === lastReply.current) return;
    const first = lastReply.current === null;
    lastReply.current = replyId;
    // The first reply seen on mount may be an old one already acted on; only a change moves the owner.
    if (!first) onMoved?.("replied");
    // Once per reply: `onMoved` is the owner's callback, read as it stands when the reply lands.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [replyId]);
  useEffect(() => {
    lastReply.current = null;
  }, [at]);

  const [asking, setAsking] = useState(false);
  const busy = state !== null && !state.done;

  // The ask goes into the conversation this checkout is on — a new one
  // about it when there is none — never to a session.
  const ask = async (kind: RunKind, agent: string, content: string, extras: AskExtras = {}) => {
    if (asking || busy || !pid) return false;
    setAsking(true);
    let target: string | null = null;
    try {
      return await attempt(
        async () => {
          target = (await ensureConversation(wid, pid)).id;
          await api.postConversationMessage(target, { content, mentions: [agent] });
        },
        toast.error,
        () => setRun({ kind, agent, at: now(), ...(target ? { conversation: target } : {}), ...(aheadOfBase !== null ? { ahead: aheadOfBase } : {}), ...extras }),
      );
    } finally {
      setAsking(false);
    }
  };

  // Stop is an act on a session that runs or waits (`state.live`) — the line
  // offers it then and only then; a run with no such session ends on its own.
  const stop = () => {
    const id = state?.session?.id;
    if (!run || !state?.live || !id) return;
    // The store's one door: a session the node no longer has ended already, and the run with it.
    void attempt(() => stopSession(id), toast.error, () => {
      toast.ok(t("work-use-review-run-stopped-ask-again-same-agent-another"));
      setRun({ ...run, ended: { at: now(), how: "stopped" } });
    });
  };

  const dismiss = () => setRun(null);

  return { run, state, reply, asking, busy, ask, stop, dismiss };
}
