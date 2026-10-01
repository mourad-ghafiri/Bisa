/**
 * The pull request a workstream opened — or merged through — as one card
 * (ide/08) under the *Open pull request* step: the number and title as the
 * door to the code host, who and where, its state, and — when the code host
 * reports check runs — the checks' one-line summary as a chip. The card is
 * the record; the checks' own step under *Checks* is where a failed run is
 * handed to an agent (`ChecksStep`), the reviews and the comments are the
 * *Review* step's (`ReviewStep`), the merge the *Merge* step's one act. Each
 * surface sits under the step it belongs to, so the spine reads in the order
 * the work happens.
 */

import { openExternal } from "../../api";
import { errorFields, log } from "../../log";
import type { CheckRun, CodeHostCapabilities, PullRequest } from "../../types";
import { Chip } from "../../ui";
import { prNounCap } from "./codeHostWords.mjs";
import { checksSummary } from "./prFormModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export function PullRequestCard({
  pr,
  checks,
  caps,
  codeHost,
}: {
  pr: PullRequest;
  /** Check runs; `null` while unread or when the code host reports none. */
  checks: CheckRun[] | null;
  caps: CodeHostCapabilities | null;
  /** The code host behind origin, by kind — for the noun (a *merge request* on GitLab). */
  codeHost: string | null;
}) {
  const summary = checks ? checksSummary(checks) : null;

  return (
    <section aria-label={`${prNounCap(codeHost)} #${pr.number}`} className="flex flex-col gap-1.5 rounded-card border border-border p-2 text-2xs">
      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          className="font-mono text-accent-ink underline underline-offset-2"
          title={pr.url}
          onClick={() => void openExternal(pr.url).catch((e: unknown) => log.warn("shell", "the machine's browser could not be opened", { url: pr.url, ...errorFields(e) }))}
        >
          #{pr.number}
        </button>
        <span className="min-w-0 flex-1 truncate text-text" title={pr.title}>
          {pr.title}
        </span>
        <Chip tone={pr.state === "merged" ? "ok" : pr.state === "open" ? "neutral" : "quiet"}>{pr.is_draft ? t("work-pull-request-card-draft") : pr.state}</Chip>
        {pr.mergeable === false && <Chip tone="warn">{t("work-pull-request-card-conflicts")}</Chip>}
        {caps?.check_runs && summary && (
          <Chip tone={summary.tone === "danger" ? "danger" : summary.tone === "ok" ? "ok" : "quiet"} title={t("work-pull-request-card-checks-each-run-door-fix-failed")}>
            {summary.text}
          </Chip>
        )}
      </div>
      <div className="flex items-center gap-2 font-mono text-text-dim">
        <span className="min-w-0 truncate">
          {pr.head} → {pr.base}
        </span>
        {pr.author && <span className="shrink-0">{t("work-pull-request-card-words", { author: pr.author })}</span>}
      </div>
    </section>
  );
}
