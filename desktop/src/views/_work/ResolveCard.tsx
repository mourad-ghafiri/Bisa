/**
 * The Resolve card (ide/04 §Conflicts, continued): above whichever Git
 * view shows, whenever git has left a merge, a rebase, a cherry-pick or a
 * revert half-done in the checkout — started here or in a terminal. It
 * names the operation from the node's facts (*Merging feature/login into
 * main*, *Rebasing feature onto main — commit 3 of 7*), says in one sentence
 * what is happening and, as two swatches, which side is which; opens *What
 * is a conflict?* for anyone who has never met one; lists the conflicted
 * files as a checklist with their kinds — each a door to the conflict
 * document — ticking each as it settles; and holds the three verbs that end
 * it: **Continue** (off with the count while files are conflicted), **Skip**
 * (never for a merge), **Abort**. Each is consented in one dialog with the
 * `SafetyNote`. The facts are `operationModel.mjs`'s and
 * `conflictSidesModel.mjs`'s; the verbs run through `gitOps`.
 */
import { useState } from "react";
import { api } from "../../api";
import type { GitInProgress, GitStatusInfo } from "../../types";
import { Button, Chip, ConfirmDialog, ICON, Tooltip, WorkingDot } from "../../ui";
import { rootKey } from "../_workbench/workbenchModel.mjs";
import { whatIsAConflict } from "./conflictSidesModel.mjs";
import type { Side } from "./conflictSidesModel.mjs";
import * as ops from "./gitOps";
import { useGitSession } from "./gitPanelStore";
import { abortConsent, continueConsent, operationVerbs, operationWords, skipConsent } from "./operationModel.mjs";
import { SafetyNote } from "./SafetyNote";
import { useAsync } from "./useAsync";
import { t } from "../../i18n/l10n.mjs";

type Verb = "continue" | "skip" | "abort";

/**
 * One side's mark and name, as the legend and the conflict document draw it.
 * The sides are identities, so the mark is a neutral glyph (`SIDE_ICON`), not
 * a colour: the accent stays for what waits on the person.
 */
export function SideSwatch({ side, dim = false }: { side: Side; dim?: boolean }) {
  const Mark = ICON[side.icon];
  return (
    <span className="inline-flex min-w-0 items-center gap-1.5" title={side.role}>
      <Mark size={12} aria-hidden className="shrink-0 text-text-dim" />
      <span className={`min-w-0 truncate ${dim ? "text-text-dim" : "font-medium text-text"}`}>{side.name}</span>
      <span className="shrink-0 text-text-dim">— {side.key === "mine" ? t("work-resolve-card-mine") : t("work-resolve-card-theirs")}</span>
    </span>
  );
}

export function ResolveCard({ wid, status, onOpenPath }: { wid: string; status: GitStatusInfo; onOpenPath: (path: string) => void }) {
  const inProgress = status.in_progress ?? null;
  const scope = rootKey("workstream", wid);
  const session = useGitSession(scope);
  // The rows a write answered with are the freshest; else the listing, re-read
  // whenever a write lands. The facts are read with them: a continue that
  // stopped on the next commit changes the step.
  const listing = useAsync((s) => (inProgress ? api.gitFiles(wid, s) : Promise.resolve(null)), [wid, inProgress, session.stale]);
  const facts = useAsync((s) => (inProgress ? api.gitOperation(wid, s) : Promise.resolve(null)), [wid, inProgress, session.stale]);
  const [pending, setPending] = useState<Verb | null>(null);
  const [explaining, setExplaining] = useState(false);
  if (!inProgress) return null;
  const files = session.files ?? listing.data?.files ?? [];
  const words = operationWords(inProgress, facts.data ?? null, session.operation, files);
  const verbs = operationVerbs(inProgress, words.files, session.busy !== null);
  const consent = pending === "continue" ? continueConsent(inProgress, facts.data ?? null) : pending === "skip" ? skipConsent(inProgress) : pending === "abort" ? abortConsent(inProgress) : null;
  const run = (verb: Verb, what: GitInProgress) => {
    setPending(null);
    if (verb === "continue") void ops.continueOp(scope, wid, what);
    else if (verb === "skip") void ops.skipOp(scope, wid, what);
    else void ops.abort(scope, wid, what);
  };
  const working = session.busy === "continue" || session.busy === "skip" || session.busy === "abort";
  const left = words.checklist.filter((r) => !r.settled).length;
  const done = words.checklist.length - left;
  return (
    <div className="flex flex-col gap-2 rounded-control border border-warn/40 bg-warn-soft/40 px-2.5 py-2 text-2xs" role="region" aria-label={t("work-resolve-card-resolve-conflicts")}>
      <div className="flex items-center gap-2">
        {working ? <WorkingDot title={`${session.busy}…`} /> : <ICON.merge size={13} aria-hidden className="shrink-0 text-warn" />}
        <span className="min-w-0 truncate font-semibold text-text">{words.title}</span>
        {words.step && (
          <Chip tone="warn" title={t("work-resolve-card-where-operation-stopped")}>
            {words.step}
          </Chip>
        )}
        <span className="flex-1" />
        <span className="shrink-0 text-text-dim">{words.line}</span>
      </div>
      <p className="text-text-dim">{words.explain}</p>
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1">
        <SideSwatch side={words.sides.mine} />
        <SideSwatch side={words.sides.theirs} />
        <span className="flex-1" />
        <button type="button" className="anim text-text-dim underline decoration-dotted underline-offset-2 hover:text-text" aria-expanded={explaining} onClick={() => setExplaining((v) => !v)}>
          {explaining ? t("work-resolve-card-hide") : t("work-resolve-card-what-conflict")}
        </button>
      </div>
      {explaining && (
        <ul className="flex flex-col gap-1 rounded-control bg-surface-2/50 px-2.5 py-2 leading-relaxed text-text-dim">
          {whatIsAConflict(inProgress).map((s) => (
            <li key={s}>{s}</li>
          ))}
        </ul>
      )}
      {words.checklist.length > 0 && (
        <div className="flex flex-col gap-1">
          {words.checklist.length > 1 && (
            <div className="flex items-center gap-2" aria-label={t("work-resolve-card-files-settled", { done, checklist: words.checklist.length })}>
              <div className="h-1 flex-1 overflow-hidden rounded-full bg-surface-2">
                <div className="h-full rounded-full bg-ok transition-[width]" style={{ width: `${(done / words.checklist.length) * 100}%` }} />
              </div>
              <span className="tnum shrink-0 text-text-dim">
                {done} / {words.checklist.length}
              </span>
            </div>
          )}
          <ul className="flex flex-col gap-0.5">
            {words.checklist.map((row) => (
              <li key={row.path} className="flex items-center gap-1.5">
                {row.settled ? <ICON.check size={12} aria-hidden className="shrink-0 text-ok" /> : <ICON.warn size={12} aria-hidden className="shrink-0 text-warn" />}
                {row.settled ? (
                  <span className="min-w-0 truncate font-mono text-text-dim line-through decoration-text-dim/50">{row.path}</span>
                ) : (
                  <Tooltip label={t("work-resolve-card-open-conflict-document-walks-through")}>
                    <button type="button" onClick={() => onOpenPath(row.path)} className="anim min-w-0 truncate rounded-control font-mono text-text underline decoration-dotted underline-offset-2 hover:text-accent">
                      {row.path}
                    </button>
                  </Tooltip>
                )}
                <Chip tone={row.settled ? "ok" : "warn"}>{row.short}</Chip>
              </li>
            ))}
          </ul>
        </div>
      )}
      <div className="flex flex-wrap items-center gap-2">
        <Tooltip label={verbs.continue.reason ?? t("work-resolve-card-go-next-step")}>
          <span className="inline-flex">
            <Button size="sm" variant="primary" disabled={verbs.continue.disabled} onClick={() => setPending("continue")}>
              {verbs.continue.label}
            </Button>
          </span>
        </Tooltip>
        {verbs.skip && (
          <Tooltip label={verbs.skip.reason ?? t("work-resolve-card-leave-stopped-commit-out-go")}>
            <span className="inline-flex">
              <Button size="sm" variant="ghost" disabled={verbs.skip.disabled} onClick={() => setPending("skip")}>
                {verbs.skip.label}
              </Button>
            </span>
          </Tooltip>
        )}
        <span className="flex-1" />
        <Tooltip label={verbs.abort.reason ?? t("work-resolve-card-back-where-branch-when-started-nothing")}>
          <span className="inline-flex">
            <Button size="sm" variant="ghost" className="hover:text-danger" disabled={verbs.abort.disabled} onClick={() => setPending("abort")}>
              {verbs.abort.label}
            </Button>
          </span>
        </Tooltip>
      </div>
      <ConfirmDialog
        open={consent !== null}
        onClose={() => setPending(null)}
        onConfirm={() => pending && run(pending, inProgress)}
        title={consent?.title ?? ""}
        body={
          <div className="flex flex-col gap-2">
            <p>{consent?.body ?? ""}</p>
            <SafetyNote kind="tree" />
          </div>
        }
        confirmLabel={consent?.confirm ?? t("work-commit-action-dialogs-continue")}
        danger={consent?.danger ?? false}
      />
    </div>
  );
}
