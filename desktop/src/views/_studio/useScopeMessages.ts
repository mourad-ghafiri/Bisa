/**
 * Messages for one conversation scope: load, page backwards, post, react,
 * retract — and stay live without polling.
 *
 * The window is bounded (`PAGE` newest, older on request) so a long-running
 * channel can't grow the DOM without limit. A conversation frame for this
 * scope that names its event reads that one message and merges it in; a
 * frame that names none (a snapshot), or the bus coming back, re-reads the
 * newest page: the server is the authority on what exists, the frame only
 * says where to look.
 *
 * **A thread comes back with what it had** (`threadsStore.ts`,
 * `threadCacheModel.mjs`): what the hook holds is kept for the life of the
 * window, so a thread that remounts draws its messages at once — the older
 * pages a person had read back to included — and is not *loading* while the
 * newest page is read behind them. That page **joins** what is held rather
 * than replacing it: inside its span the page is the truth, and what is
 * older stays, so the message a person was reading does not leave the page
 * under them. A write on a message above the newest page reads the page that
 * holds it as well.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { ApiError, api } from "../../api";
import { useConversationEvents } from "../../bus";
import { errorFields, log } from "../../log";
import { useReloadOnReconnect } from "../../ui/useReloadOnReconnect";
import type { ArtifactRef, AttachmentRef, ContextRef, ConversationFrame, MessageRow, ReactionRow } from "../../types";
import { readMessages, writeMessage, type ScopeHome, type ScopeKind } from "./scope";
import { joinAround, joinNewest, joinReactions, nudgeRead, threadKey } from "./threadCacheModel.mjs";
import { keepThread, threadOf, type KeptThread } from "./threadsStore";
import { olderThan } from "./pageCursorModel.mjs";
import { t } from "../../i18n/l10n.mjs";

const PAGE = 60;

/** A thread with nothing in it — one object, so a thread that holds nothing is the same thread between renders. */
const EMPTY: KeptThread = Object.freeze({ messages: [], reactions: [], hasOlder: false });

/** What the hook holds, and the thread it is of: a scope changed in place never draws the last one's messages. */
interface Held {
  readonly of: string | null;
  readonly thread: KeptThread;
}

interface ScopePage {
  messages: MessageRow[];
  reactions: ReactionRow[];
}

interface ScopeMessages {
  messages: MessageRow[];
  reactions: ReactionRow[];
  loading: boolean;
  error: string | null;
  hasOlder: boolean;
  loadingOlder: boolean;
  loadOlder: () => Promise<void>;
  reload: () => Promise<void>;
  post: (
    content: string,
    mentions: string[],
    attachments: AttachmentRef[],
    replyTo?: string,
    context?: ContextRef[],
    artifacts?: ArtifactRef[],
  ) => Promise<void>;
  react: (messageId: string, emoji: string) => Promise<void>;
  unreact: (reactionId: string) => Promise<void>;
  /** Read one message again in place — after its artifact's bytes arrived, so `present` is true where it is drawn. */
  refreshMessage: (messageId: string) => Promise<void>;
  retract: (messageId: string) => Promise<void>;
}

function mergeById(older: readonly MessageRow[], newer: readonly MessageRow[]): MessageRow[] {
  const seen = new Map<string, MessageRow>();
  for (const m of [...older, ...newer]) seen.set(m.id, m);
  return [...seen.values()].sort((a, b) => a.created_at - b.created_at);
}

/** The newest page, joined to what the thread holds. */
function withNewest(shown: KeptThread, page: ScopePage): KeptThread {
  const joined = joinNewest({ shown: shown.messages, page: page.messages, pageSize: PAGE, hasOlder: shown.hasOlder });
  return {
    messages: joined.messages,
    reactions: joinReactions({ shown: shown.reactions, page: page.reactions, messages: joined.messages, landed: page.messages }),
    hasOlder: joined.hasOlder,
  };
}

/** A page read again where it stands, laid into what the thread holds. */
function withAround(shown: KeptThread, page: ScopePage): KeptThread {
  const messages = joinAround({ shown: shown.messages, page: page.messages });
  return { messages, reactions: joinReactions({ shown: shown.reactions, page: page.reactions, messages, landed: page.messages }), hasOlder: shown.hasOlder };
}

export function useScopeMessages(scope: string | null, kind: ScopeKind, home: ScopeHome = null): ScopeMessages {
  const host = home?.host ?? null;
  const key = scope ? threadKey(kind, scope, host) : null;
  const [held, setHeld] = useState<Held>(() => ({ of: key, thread: threadOf(key) ?? EMPTY }));
  const [loading, setLoading] = useState(() => !threadOf(key));
  const [loadingOlder, setLoadingOlder] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** The newest-page read a nudge started: a later one supersedes it. */
  const live = useRef<AbortController | null>(null);
  /** The one-message reads asked for and not yet landed, each on its own: none cancels another. */
  const ones = useRef(new Set<AbortController>());
  /** The line those reads stand in — frame order — so rows of one second join the thread as they were posted. */
  const queue = useRef<Promise<void>>(Promise.resolve());

  // The thread on screen: what is held while it is this scope's, else what
  // this scope had — the store hands the same thread back until it changes.
  const thread = held.of === key ? held.thread : (threadOf(key) ?? EMPTY);
  const shown = useRef(thread);
  shown.current = thread;
  const current = useRef(key);
  current.current = key;

  /** Change what is held of this scope's thread. An answer that lands after the scope changed is another thread's: dropped. */
  const change = useCallback(
    (next: (thread: KeptThread) => KeptThread) => {
      if (current.current !== key) return;
      setHeld((prev) => {
        const base = prev.of === key ? prev.thread : (threadOf(key) ?? EMPTY);
        const thread = next(base);
        return prev.of === key && thread === base ? prev : { of: key, thread };
      });
    },
    [key],
  );

  // What is held is kept for the window's life, on every change.
  useEffect(() => {
    keepThread(held.of, held.thread);
  }, [held]);

  /** Read the newest page and join it; the page, or `null` when the read was abandoned or failed. */
  const fetchNewest = useCallback(
    async (signal?: AbortSignal): Promise<ScopePage | null> => {
      if (!scope) {
        change(() => EMPTY);
        setLoading(false);
        return null;
      }
      try {
        const page = await readMessages(kind, scope, undefined, PAGE, signal, host ? { host } : null);
        if (signal?.aborted) return null;
        change((thread) => withNewest(thread, page));
        setError(null);
        return page;
      } catch (e) {
        if (signal?.aborted) return null;
        setError(e instanceof ApiError ? e.message : t("studio-use-scope-messages-could-not-load-messages"));
        return null;
      } finally {
        if (!signal?.aborted) setLoading(false);
      }
    },
    [change, host, kind, scope],
  );

  useEffect(() => {
    const ctrl = new AbortController();
    // A thread that has something kept is drawn at once and read behind it.
    setLoading(!threadOf(key));
    void fetchNewest(ctrl.signal);
    return () => ctrl.abort();
  }, [fetchNewest, key]);

  // Live: a frame that names a message reads that one message — a reply
  // landing is one row, not a page of sixty — and anything else reads the
  // newest page (`threadCacheModel.nudgeRead`): a snapshot, a reaction, a
  // retraction, a hosted scope, the bus coming back (what was posted while
  // the node was away reached this page by no frame).
  //
  // The two kinds of read are held apart. A later page read supersedes an
  // earlier one; a one-message read is superseded by nothing — a reply longer
  // than one message lands as several frames in a row, and cancelling the
  // read of the first for the read of the second would leave a hole in the
  // thread that no later frame fills. One that fails reads the page instead.
  const readNewest = useCallback(() => {
    if (!scope) return;
    live.current?.abort();
    const ctrl = new AbortController();
    live.current = ctrl;
    void readMessages(kind, scope, undefined, PAGE, ctrl.signal, host ? { host } : null)
      .then((page) => {
        if (!ctrl.signal.aborted) change((thread) => withNewest(thread, page));
      })
      .catch((e: unknown) => {
        // The page a nudge asked for did not come: the thread keeps what it shows, and the next frame, the bus coming back or Reload reads it again.
        if (!ctrl.signal.aborted) log.debug("view", "a live read of a thread's newest page did not arrive", { scope, ...errorFields(e) });
      });
  }, [change, host, kind, scope]);
  const nudge = useCallback(
    (frame?: ConversationFrame) => {
      if (!scope) return;
      const plan = nudgeRead(frame, host != null);
      if (plan.read === "page") {
        readNewest();
        return;
      }
      // One after another, in the order the frames came: two messages stamped
      // in the same second are told apart by nothing but the order they
      // landed in, so the rows join the thread in that order.
      const ctrl = new AbortController();
      ones.current.add(ctrl);
      const id = plan.id;
      queue.current = queue.current
        .then(async () => {
          if (ctrl.signal.aborted) return;
          const r = await api.message(id, ctrl.signal);
          if (!ctrl.signal.aborted) change((thread) => ({ ...thread, messages: mergeById(thread.messages, [r.message]) }));
        })
        .catch(() => {
          // The one row could not be read: the newest page holds it, and whatever landed beside it.
          if (!ctrl.signal.aborted) readNewest();
        })
        .finally(() => {
          ones.current.delete(ctrl);
        });
    },
    [change, host, readNewest, scope],
  );
  useConversationEvents(nudge, scope ?? undefined);
  useReloadOnReconnect(() => nudge());

  // Leaving the thread — the hook unmounting, or its scope changing in place — ends every live read of it: none lands in another thread.
  useEffect(
    () => () => {
      live.current?.abort();
      for (const ctrl of ones.current) ctrl.abort();
      ones.current.clear();
    },
    [host, kind, scope],
  );

  const loadOlder = useCallback(async () => {
    const messages = shown.current.messages;
    if (!scope || loadingOlder || messages.length === 0) return;
    setLoadingOlder(true);
    try {
      const oldest = messages[0]!;
      const known = new Set(messages.map((m) => m.id));
      const page = await readMessages(kind, scope, olderThan(oldest, host !== null), PAGE, undefined, host ? { host } : null);
      const fresh = page.messages.filter((m) => !known.has(m.id)).length;
      change((thread) => {
        const seen = new Map<string, ReactionRow>();
        for (const r of [...page.reactions, ...thread.reactions]) seen.set(r.id, r);
        return {
          messages: mergeById(page.messages, thread.messages),
          reactions: [...seen.values()],
          // Nothing new came back: we are at the start of the conversation.
          hasOlder: fresh > 0 && page.messages.length >= PAGE,
        };
      });
    } catch (e) {
      setError(e instanceof ApiError ? e.message : t("studio-use-scope-messages-could-not-load-older-messages"));
    } finally {
      setLoadingOlder(false);
    }
  }, [change, host, kind, loadingOlder, scope]);

  const reload = useCallback(async () => {
    setLoading(true);
    await fetchNewest();
  }, [fetchNewest]);

  /**
   * After a write on one message: the newest page is read, and — when the
   * message stands above it, in an older page a person had read back to —
   * the page that holds it, so the mark or the retraction shows where it was
   * made instead of waiting for a page that will never bring it.
   */
  const readAfter = useCallback(
    async (messageId: string) => {
      const page = await fetchNewest();
      if (!scope || !page || page.messages.some((m) => m.id === messageId)) return;
      const target = shown.current.messages.find((m) => m.id === messageId);
      if (!target) return;
      try {
        // One second later than the message, on either wire: the page begins with it.
        const around = await readMessages(kind, scope, { at: target.created_at + 1 }, PAGE, undefined, host ? { host } : null);
        change((thread) => withAround(thread, around));
      } catch (e) {
        setError(e instanceof ApiError ? e.message : t("studio-use-scope-messages-could-not-load-messages"));
      }
    },
    [change, fetchNewest, host, kind, scope],
  );

  const post = useCallback(
    async (
      content: string,
      mentions: string[],
      attachments: AttachmentRef[],
      replyTo?: string,
      context?: ContextRef[],
      artifacts?: ArtifactRef[],
    ) => {
      if (!scope) return;
      await writeMessage(
        kind,
        scope,
        {
          content,
          mentions: mentions.length ? mentions : undefined,
          // Absent rather than `[]` when there are none, matching how mentions
          // are sent: the node's default is already "no files".
          attachments: attachments.length ? attachments : undefined,
          artifacts: artifacts && artifacts.length ? artifacts : undefined,
          reply_to: replyTo,
          context: context && context.length ? context : undefined,
        },
        undefined,
        host ? { host } : null,
      );
      await fetchNewest();
    },
    [fetchNewest, host, kind, scope],
  );

  const react = useCallback(
    async (messageId: string, emoji: string) => {
      if (host) await api.reactHosted(host, messageId, emoji);
      else await api.react(messageId, emoji);
      await readAfter(messageId);
    },
    [readAfter, host],
  );

  const unreact = useCallback(
    async (reactionId: string) => {
      // The message the mark is on, read before the write takes the mark away.
      const on = shown.current.reactions.find((r) => r.id === reactionId)?.target_id;
      // A hosted reaction's row id is made of the message, the author and the
      // emoji (`hostedModel.mjs`); the host knows it by the message alone.
      if (host) await api.retractHosted(host, reactionId.split(":")[0] ?? reactionId);
      else await api.retractReaction(reactionId);
      if (on) await readAfter(on);
      else await fetchNewest();
    },
    [fetchNewest, readAfter, host],
  );

  const retract = useCallback(
    async (messageId: string) => {
      if (host) await api.retractHosted(host, messageId);
      else await api.retractMessage(messageId);
      await readAfter(messageId);
    },
    [readAfter, host],
  );

  const refreshMessage = useCallback(
    async (messageId: string) => {
      const r = await api.message(messageId);
      change((thread) => ({ ...thread, messages: mergeById(thread.messages, [r.message]) }));
    },
    [change],
  );

  return {
    messages: thread.messages as MessageRow[],
    reactions: thread.reactions as ReactionRow[],
    loading,
    error,
    hasOlder: thread.hasOlder,
    loadingOlder,
    loadOlder,
    reload,
    post,
    react,
    unreact,
    retract,
    refreshMessage,
  };
}
