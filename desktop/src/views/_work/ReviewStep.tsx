/**
 * The Review step's surface (ide/08) — one place, from the moment the branch
 * exists: before a pull request it reviews the **branch** against its base;
 * after one, the **pull request**. Optional either way: it never holds the
 * merge. Top to bottom:
 *
 * 1. one sentence on where the review stands (`statusLine`);
 * 2. **Agent** — `AgentReviewRequest`: an agent from a plain `Select`, an
 *    optional line of words, *Review with <agent>*; the request goes into
 *    the workstream's conversation as a mention, and the run is **followed
 *    right here** (`ReviewRunLine` under the control) — starting, what the
 *    agent is doing, how it ended and what it said — never in another panel;
 * 3. **You** (pull request only) — one optional box and the buttons the
 *    code host takes: **Approve** · **Submit review** · **Request changes**;
 * 4. **Comments** (pull request only) — the reviewers' inline comments on the
 *    code host, grouped by file, open first, with the count on the header.
 *    Each open comment's row offers, on hover, its **three hands**: *Fix
 *    with ▾* — an agent of your choice for **this** comment (`AgentMenu`,
 *    the last one used first), *Reply* — your own words on the thread, sent
 *    as *Reply* or *Reply and resolve* — and *Resolve* (*Reopen* once
 *    resolved). The header's *Fix all N open ▾* hands every open one to one
 *    agent. An agent handed a comment answers on the thread itself and
 *    resolves what it addressed (`fixPrompt`, the `pr_thread_reply` tool);
 *    its replies read as its own (`replierOf`). A fix run is followed at the
 *    top of the section;
 * 5. **Reviews by others** — one line per other account.
 *
 * **One agent at a time in the checkout**: while any run is live — a
 * review, a fix, a check — every other hand to an agent gives way to who is
 * busy (`busyWords`); replying and resolving stay yours to do. A comment is
 * the code host's *thread* on the wire (`ReviewThread`); the desktop says
 * *comment*. The facts are `reviewStepModel.mjs`'s (`reviewFacts`, read
 * once by the lifecycle and passed in); the data is the lifecycle's
 * (`usePullRequest`); the run is the lifecycle's too (`useReviewRun`, handed
 * in as `run`), because the run moves what the lifecycle reads. This file
 * draws and asks and never polls.
 */

import { useState, type ReactNode } from "react";
import { api } from "../../api";
import type { CodeHostCapabilities, PullRequest, ReviewSummary, ReviewThread, ReviewThreadComment } from "../../types";
import { Button, Chip, FoldedText, ICON, SectionHeader, SkeletonRows, TextArea, Tooltip, cn, useCollapsed, useToast } from "../../ui";
import { DEFAULT_AGENT, FIX_MENU_LABEL, draftKeys, fixAllLabel, fixLabel, reviewButtons, wordsReason } from "./agentReviewModel.mjs";
import type { ReviewTarget } from "./agentReviewModel.mjs";
import { AgentMenu } from "./AgentMenu";
import { AgentReviewRequest } from "./AgentReviewRequest";
import { useSessionDraft } from "./gitPanelStore";
import { commentsByFile, fixPrompt, reviewTone } from "./prReviewModel.mjs";
import { ReasonLine } from "./ReasonLine";
import { ReviewGiven, When } from "./ReviewGiven";
import { ReviewRunLine } from "./ReviewRunLine";
import {
  allowedEvents,
  busyWords,
  commentRow,
  commentsSummary,
  groupWords,
  ownPrNote,
  replierOf,
  replyWords,
  reviewKey,
  reviewWords,
  statusLine,
  submittedWords,
  verdictWords,
  type ReviewEvent,
  type ReviewFacts,
} from "./reviewStepModel.mjs";
import { attempt } from "./useAsync";
import type { ReviewRunControl } from "./useReviewRun";
import { t } from "../../i18n/l10n.mjs";

/** One part of the step — *Agent*, *You*, *Comments* — its title, a check once done, and what sits under it. */
function Row({ n, title, done, children }: { n: number; title: string; done: boolean; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1" data-part={n}>
      <div className="flex items-center gap-1.5">
        {done && <ICON.check size={12} aria-hidden className="shrink-0 text-ok" />}
        <span className={cn("text-2xs font-semibold tracking-wide uppercase", done ? "text-text-dim" : "text-text")}>{title}</span>
      </div>
      <div className="flex flex-col gap-1.5">{children}</div>
    </div>
  );
}

/** One reply on a comment: who — an agent by its id, a person by login — when, and the words without the signature line. */
function ReplyRow({ reply, storeKey }: { reply: ReviewThreadComment; storeKey: string }) {
  const who = replierOf(reply);
  return (
    <li className="flex flex-col gap-0.5">
      <div className="flex items-center gap-2">
        {who.kind === "agent" && <ICON.agent size={11} aria-hidden className="shrink-0 text-accent-ink" />}
        <span className={cn("font-mono", who.kind === "agent" ? "text-accent-ink" : "text-text-dim")}>{who.kind === "agent" ? who.agent : (who.login ?? t("work-review-step-someone"))}</span>
        <When iso={reply.created_at} />
      </div>
      <FoldedText text={replyWords(reply)} storeKey={storeKey} />
    </li>
  );
}

/** Your reply on a comment: the words, and whether the thread closes with them. */
function ReplyBox({ canResolve, sending, onSend, onCancel }: { canResolve: boolean; sending: boolean; onSend: (body: string, resolve: boolean) => void; onCancel: () => void }) {
  const [body, setBody] = useState("");
  const empty = body.trim() === "";
  return (
    <div className="mt-1 flex flex-col gap-1.5 pl-6">
      <TextArea value={body} rows={2} autoFocus placeholder={t("work-review-step-reply-code-host")} onChange={(e) => setBody(e.target.value)} />
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" variant="primary" disabled={sending || empty} onClick={() => onSend(body.trim(), false)}>
          {sending ? t("work-review-notes-sending") : t("work-review-step-reply")}
        </Button>
        {canResolve && (
          <Button size="sm" variant="ghost" disabled={sending || empty} onClick={() => onSend(body.trim(), true)}>
            <ICON.check size={11} aria-hidden />
            <span className="ml-1">{t("work-review-step-reply-resolve")}</span>
          </Button>
        )}
        <Button size="sm" variant="ghost" disabled={sending} onClick={onCancel}>{t("work-agent-editor-cancel")}</Button>
        {empty && <ReasonLine>{t("work-review-step-reply-needs-words")}</ReasonLine>}
      </div>
    </div>
  );
}

/**
 * One comment, folded to a row; open, every reply with its words. Hover, on
 * an open comment: *Fix with ▾*, *Reply*, *Resolve* — or, while an agent
 * runs in this checkout, who is busy where the menu would be.
 */
function CommentRow({
  comment,
  pid,
  run,
  isOpen,
  canReply,
  canResolve,
  remembered,
  onFix,
  onReply,
  onResolve,
}: {
  comment: ReviewThread;
  pid: string | null;
  /** The checkout's one run — busy, and which comment it is fixing. */
  run: ReviewRunControl;
  /** The pull request is open: hands can be given. */
  isOpen: boolean;
  canReply: boolean;
  canResolve: boolean;
  /** The agent the row's menu leads with: the last one a comment was handed to. */
  remembered: string;
  onFix: (agent: string) => void;
  onReply: (body: string, resolve: boolean) => Promise<boolean>;
  onResolve: (resolved: boolean) => void;
}) {
  const [folded, toggle] = useCollapsed(`ide.pr.comment.${comment.id}`, true);
  const [replying, setReplying] = useState(false);
  const [sending, setSending] = useState(false);
  const row = commentRow(comment);
  const fixing = run.busy && run.state?.run.kind === "fix" && run.state.run.comment === comment.id;
  const busy = run.busy || run.asking ? busyWords(run.state, row.where) : null;
  const send = async (body: string, resolve: boolean) => {
    setSending(true);
    const went = await onReply(body, resolve);
    setSending(false);
    if (went) setReplying(false);
  };
  return (
    <li className={cn("group rounded-control border px-2 py-1", comment.is_resolved ? "border-border/60" : "border-border")}>
      <div className="flex items-center gap-2">
        <button
          type="button"
          aria-expanded={!folded}
          aria-label={folded ? t("work-review-step-show-replies") : t("work-review-step-fold-replies")}
          onClick={toggle}
          className="anim flex h-4 w-4 shrink-0 items-center justify-center rounded text-text-dim hover:bg-surface-2 hover:text-text"
        >
          {folded ? <ICON.collapsed size={11} aria-hidden /> : <ICON.expanded size={11} aria-hidden />}
        </button>
        <span className="tnum shrink-0 text-text-dim">{row.where}</span>
        <Chip tone={fixing ? "accent" : comment.is_resolved ? "quiet" : "warn"}>{fixing ? t("work-checks-step-fixing") : comment.is_resolved ? t("work-review-step-comment-resolved") : t("work-review-step-comment-open")}</Chip>
        {comment.is_outdated && <Chip tone="quiet">{t("work-review-step-outdated")}</Chip>}
        {folded && (
          <span className={cn("min-w-0 flex-1 truncate", comment.is_resolved ? "text-text-dim" : "text-text")} title={row.lead}>
            <span className="font-mono text-text-dim">{row.author}</span> {row.lead}
            {row.replies > 0 && <span className="text-text-dim"> · +{row.replies}</span>}
          </span>
        )}
        {!folded && <span className="flex-1" />}
        <span className="row-actions anim inline-flex items-center gap-1">
          {!comment.is_resolved && isOpen && !fixing && busy && (
            <span className="max-w-40 truncate text-text-dim" title={busy}>
              {busy}
            </span>
          )}
          {!comment.is_resolved && isOpen && !busy && <AgentMenu label={FIX_MENU_LABEL} remembered={remembered} pid={pid} hint={fixLabel(remembered)} onPick={onFix} />}
          {canReply && !replying && (
            <Tooltip label={t("work-review-step-reply-comment-own-words-code-host")}>
              <Button size="sm" variant="ghost" aria-label={t("work-review-step-reply")} onClick={() => setReplying(true)}>
                <ICON.reply size={11} aria-hidden />
              </Button>
            </Tooltip>
          )}
          {canResolve && (
            <Tooltip label={comment.is_resolved ? t("work-review-step-reopen-comment-code-host") : t("work-review-step-mark-comment-resolved-code-host")}>
              <Button size="sm" variant="ghost" aria-label={comment.is_resolved ? t("work-review-step-reopen") : t("work-review-notes-resolve")} onClick={() => onResolve(!comment.is_resolved)}>
                {comment.is_resolved ? <ICON.undo size={11} aria-hidden /> : <ICON.check size={11} aria-hidden />}
              </Button>
            </Tooltip>
          )}
        </span>
      </div>
      {(!folded || replying) && (
        <ul className="mt-1 flex flex-col gap-1.5 pl-6">
          {comment.comments.map((c, i) => (
            <ReplyRow key={i} reply={c} storeKey={`ide.pr.comment.${comment.id}.${i}`} />
          ))}
        </ul>
      )}
      {replying && <ReplyBox canResolve={canResolve && !comment.is_resolved} sending={sending} onSend={(body, resolve) => void send(body, resolve)} onCancel={() => setReplying(false)} />}
    </li>
  );
}

/** One optional box, and the buttons the code host takes — Approve · Submit review · Request changes. */
function YourReviewForm({
  wid,
  pr,
  viewer,
  caps,
  onSubmitted,
}: {
  wid: string;
  pr: PullRequest;
  viewer: string | null;
  /** The host's capabilities — which verdicts it takes at all. */
  caps: CodeHostCapabilities | null;
  onSubmitted: () => void;
}) {
  const toast = useToast();
  const allowed = allowedEvents(pr, viewer, caps);
  const buttons = reviewButtons(allowed);
  const [body, setBody] = useState("");
  const [sending, setSending] = useState<ReviewEvent | null>(null);
  const note = ownPrNote(pr, viewer);
  const submit = async (event: ReviewEvent) => {
    if (sending || wordsReason(event, body)) return;
    setSending(event);
    try {
      await api.workstreamPrReview(wid, { event, body: body.trim() || undefined });
      toast.ok(submittedWords(event));
      setBody("");
      onSubmitted();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setSending(null);
    }
  };
  // The one reason worth a line: the plainest button that is off for want of words.
  const reason = buttons.map((b) => wordsReason(b.event, body)).find((r) => r !== null) ?? null;
  return (
    <div className="flex flex-col gap-1.5">
      <TextArea value={body} rows={2} placeholder={t("work-review-step-word-review-optional-approval")} onChange={(e) => setBody(e.target.value)} />
      <div className="flex flex-wrap items-center gap-2">
        {buttons.map((b) => (
          <Button
            key={b.event}
            size="sm"
            variant={b.primary ? "primary" : "ghost"}
            className={b.danger ? "text-danger" : undefined}
            disabled={sending !== null || wordsReason(b.event, body) !== null}
            onClick={() => void submit(b.event)}
          >
            {sending === b.event ? t("work-review-notes-sending") : b.label}
          </Button>
        ))}
      </div>
      {note ? <ReasonLine>{note}</ReasonLine> : reason ? <ReasonLine>{reason}</ReasonLine> : null}
    </div>
  );
}

export function ReviewStep({
  wid,
  scope,
  pid,
  target,
  pr,
  aheadOfBase,
  caps,
  viewer,
  facts,
  data,
  run,
  onChanged,
  onOpenChanges,
}: {
  wid: string;
  /** The checkout's session scope (`rootKey("workstream", wid)`): where the agents chosen and the run are kept. */
  scope: string;
  /** The project, for its default agent. */
  pid: string | null;
  /** What is reviewed: the branch against its base, or the pull request. */
  target: ReviewTarget;
  /** The pull request, once there is one. */
  pr: PullRequest | null;
  /** Commits beyond the base — a branch with none has nothing to review yet. */
  aheadOfBase: number | null;
  caps: CodeHostCapabilities | null;
  /** The connected account's login, or null while unknown. */
  viewer: string | null;
  /** Who has reviewed and where that leaves the step — `reviewFacts`. */
  facts: ReviewFacts;
  /** The code host's reviews and comments; `null` while unread, and while there is no pull request. */
  data: { reviews: ReviewSummary[]; threads: ReviewThread[] } | null;
  /** The agent run — the lifecycle's, one at a time: asked, stopped and dismissed here, followed by the lifecycle's reads. */
  run: ReviewRunControl;
  /** The code host's side moved — a review, a comment, a reply — so it should be read again. */
  onChanged: () => void;
  /** Git › Changes — where the first commit is made. */
  onOpenChanges?: () => void;
}) {
  const toast = useToast();
  // The remembered fixer: the last agent a comment was handed to, starting on the reviewer.
  const keys = draftKeys(scope);
  const [reviewer] = useSessionDraft<string>(keys.agent, DEFAULT_AGENT);
  const [fixer, setFixer] = useSessionDraft<string>(keys.fixAgent, reviewer);
  const kind = run.state?.run.kind ?? null;
  // The line follows the run where its kind belongs: under Agent for a review, under the Comments header for a fix; a check's is the Checks step's.
  const reviewLine =
    run.state && (kind === "review" || kind === "branch") ? <ReviewRunLine scope={scope} state={run.state} reply={run.reply} aheadOfBase={aheadOfBase} onStop={run.stop} onDismiss={run.dismiss} /> : null;
  const fixLine =
    run.state && kind === "fix" ? (
      <ReviewRunLine scope={scope} state={run.state} reply={run.reply} aheadOfBase={aheadOfBase} onStop={run.stop} onDismiss={run.dismiss} onOpenChanges={onOpenChanges} />
    ) : null;
  const fixing = run.busy && kind === "fix" ? run.state?.run : null;
  const [yoursAgain, setYoursAgain] = useState(false);
  const [commentsFolded, toggleComments] = useCollapsed("ide.pr.comments", false);
  const isOpen = pr?.state === "open";

  // The branch, before a pull request: the agent request alone, and its run.
  if (target.kind === "branch") {
    const nothing = aheadOfBase === 0;
    return (
      <section aria-label={t("work-conflict-view-review")} className="flex flex-col gap-2 text-2xs">
        <p className="text-text-dim">{t("work-review-step-optional-ask-agent-review-branch-against", { base: target.base })}</p>
        <Row n={1} title={t("work-review-step-agent")} done={false}>
          <AgentReviewRequest
            scope={scope}
            target={target}
            disabled={nothing}
            busy={run.busy}
            reason={nothing ? t("work-review-step-nothing-review-yet-no-commit-beyond", { base: target.base }) : null}
            onAsk={run.ask}
          />
          {reviewLine}
          {nothing && onOpenChanges && (
            <button type="button" className="self-start text-2xs text-accent-ink underline underline-offset-2" onClick={onOpenChanges}>{t("work-new-workstream-dialog-open-git-changes")}</button>
          )}
        </Row>
      </section>
    );
  }

  const noun = target.noun ?? t("work-publish-outcome-banner-pull-request");
  const comments = data?.threads ?? [];
  const groups = commentsByFile(comments);
  const open = facts.openComments;
  const canResolve = caps?.review_threads === true;
  const canReply = caps?.review_thread_replies === true && isOpen;
  const openOnes = comments.filter((c) => !c.is_resolved);

  const setResolved = (id: string, resolved: boolean) => void attempt(() => api.workstreamPrResolveThread(wid, id, resolved), toast.error, onChanged);
  /** Your reply on a comment — and the thread resolved with it when asked; the code host is read again after. */
  const reply = (id: string, body: string, resolve: boolean) =>
    attempt(
      () => api.workstreamPrReplyThread(wid, id, body, resolve),
      toast.error,
      () => {
        toast.ok(resolve ? t("work-review-step-replied-comment-resolved") : t("work-review-step-replied-comment"));
        onChanged();
      },
    );
  /** Hand comments to an agent — one, or every open one — remember the agent, and follow it under the header. */
  const fix = (which: ReviewThread[], agent: string) => {
    if (run.busy || run.asking || which.length === 0 || !pr) return;
    setFixer(agent);
    void run.ask("fix", agent, fixPrompt(pr, which, noun), { count: which.length, comment: which.length === 1 ? which[0]?.id : undefined });
  };
  const busy = run.busy || run.asking ? busyWords(run.state) : null;
  const yoursGiven = facts.yours && !yoursAgain;

  return (
    <section aria-label={t("work-conflict-view-review")} className="flex flex-col gap-2 text-2xs">
      <p className="text-text-dim">{data === null ? t("work-pr-lifecycle-reading-code-host") : statusLine(facts)}</p>

      <Row n={1} title={t("work-review-step-agent")} done={facts.agent !== null}>
        {data === null ? (
          <SkeletonRows rows={1} />
        ) : (
          <AgentReviewRequest scope={scope} target={target} facts={facts} disabled={!isOpen} busy={run.busy} reason={isOpen ? null : t("work-review-step-pull-request-no-longer-open")} onAsk={run.ask} />
        )}
        {reviewLine}
      </Row>

      <Row n={2} title={t("work-review-step-words")} done={facts.yours !== null}>
        {data === null ? (
          <SkeletonRows rows={1} />
        ) : yoursGiven && facts.yours ? (
          <>
            <div className="flex items-center gap-2">
              <span className="text-text">{t("work-review-step-words")}</span>
              <span className="text-text-dim">{t("work-agent-review-request-reviewed")}</span>
              <span className="flex-1" />
              {isOpen && (
                <Button size="sm" variant="ghost" onClick={() => setYoursAgain(true)}>{t("work-agent-review-request-review-again")}</Button>
              )}
            </div>
            <ReviewGiven review={facts.yours} words={reviewWords(facts.yours)} verdict={verdictWords(facts.yours)} storeKey={`ide.pr.review.${reviewKey(facts.yours, 1)}`} />
          </>
        ) : isOpen && pr ? (
          <YourReviewForm
            wid={wid}
            pr={pr}
            viewer={viewer}
            caps={caps}
            onSubmitted={() => {
              setYoursAgain(false);
              onChanged();
            }}
          />
        ) : (
          <p className="text-text-dim">{t("work-review-step-pull-request-no-longer-open")}</p>
        )}
      </Row>

      {(comments.length > 0 || data === null) && (
        <div>
          <SectionHeader
            title={t("work-review-step-comments")}
            count={comments.length}
            showZero
            open={!commentsFolded || fixLine !== null}
            onToggle={toggleComments}
            trailing={<span className="text-text-dim">{data === null ? t("work-review-step-reading") : commentsSummary(comments)}</span>}
            alwaysAction
            action={
              open > 0 && isOpen && !fixing ? (
                busy ? (
                  <span className="max-w-48 truncate text-text-dim" title={busy}>
                    {busy}
                  </span>
                ) : (
                  <AgentMenu
                    label={fixAllLabel(open)}
                    remembered={fixer}
                    pid={pid}
                    hint={t("work-review-step-hand-every-open-comment-one-agent", { fixer })}
                    onPick={(agent) => fix(openOnes, agent)}
                  />
                )
              ) : undefined
            }
          />
          {(!commentsFolded || fixLine !== null) &&
            (data === null ? (
              <SkeletonRows rows={2} />
            ) : (
              <div className="flex flex-col gap-2 pl-2">
                {fixLine}
                {groups.map((g) => {
                  const words = groupWords(g);
                  return (
                    <div key={g.path || "general"} className="flex flex-col gap-1">
                      <div className="flex items-center gap-2">
                        <span className="min-w-0 truncate font-mono text-text-dim" title={words.path}>
                          {words.path}
                        </span>
                        <span className="shrink-0 text-3xs text-text-dim">{words.count}</span>
                      </div>
                      <ul className="flex flex-col gap-1">
                        {g.comments.map((c) => (
                          <CommentRow
                            key={c.id}
                            comment={c}
                            pid={pid}
                            run={run}
                            isOpen={isOpen}
                            canReply={canReply}
                            canResolve={canResolve}
                            remembered={fixer}
                            onFix={(agent) => fix([c], agent)}
                            onReply={(body, resolve) => reply(c.id, body, resolve)}
                            onResolve={(resolved) => setResolved(c.id, resolved)}
                          />
                        ))}
                      </ul>
                    </div>
                  );
                })}
              </div>
            ))}
        </div>
      )}

      {facts.others.length > 0 && (
        <Row n={4} title={t("work-review-step-reviews-others")} done={false}>
          <ul className="flex flex-col gap-1" aria-label={t("work-review-step-reviews-others")}>
            {facts.others.map((r, i) => (
              <li key={reviewKey(r, i)} className="flex flex-col gap-0.5">
                <div className="flex items-center gap-2">
                  <span className="min-w-0 truncate font-mono text-text">@{r.author ?? t("work-review-step-someone")}</span>
                  <Chip tone={reviewTone(r.state)}>{verdictWords(r)}</Chip>
                  <span className="flex-1" />
                  <When iso={r.submitted_at} />
                </div>
                {r.body.trim() && <FoldedText text={r.body} storeKey={`ide.pr.review.${reviewKey(r, i)}`} className="text-text-dim" />}
              </li>
            ))}
          </ul>
        </Row>
      )}
    </section>
  );
}
