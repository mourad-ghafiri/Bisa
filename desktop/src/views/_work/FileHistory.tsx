/**
 * The commits that touched one file, newest first, following renames —
 * `git log --follow` behind `GET /git/history`. Read-only, and small on
 * purpose: the commit graph (I6) is where a whole repository's history lives,
 * and a row here opens the commit there.
 */

import { api } from "../../api";
import { Button, EmptyState, ErrorNote, ICON, RelativeTime, SkeletonRows, Tooltip } from "../../ui";
import { useAsync } from "./useAsync";
import { t } from "../../i18n/l10n.mjs";

/** How many commits one file's history reads at once. */
const HISTORY_CAP = 50;

export function FileHistory({ wid, path, onOpenCommit }: { wid: string; path: string; onOpenCommit?: (sha: string) => void }) {
  const history = useAsync((s) => api.gitHistory(wid, path, HISTORY_CAP, s), [wid, path]);
  if (history.loading && !history.data) return <SkeletonRows rows={3} />;
  if (history.error && !history.data) return <ErrorNote error={history.error} retry={history.reload} />;
  const commits = history.data?.commits ?? [];
  if (commits.length === 0) {
    return <EmptyState title={t("work-file-history-no-commit-has-touched-path-yet")} hint={t("work-file-history-first-commit-what-starts-history")} className="py-3" action={null} />;
  }
  return (
    <div className="flex flex-col gap-1">
      <ol className="flex flex-col divide-y divide-border rounded-control border border-border">
        {commits.map((c) => (
          <li key={c.id} className="group flex items-center gap-2 px-2 py-1 text-2xs">
            <span className="shrink-0 font-mono text-accent-ink">{c.short}</span>
            <span className="min-w-0 flex-1 truncate text-text" title={c.subject}>
              {c.subject}
            </span>
            <span className="shrink-0 truncate text-text-dim" title={c.email}>
              {c.author}
            </span>
            <RelativeTime at={c.timestamp} className="shrink-0 text-text-dim" />
            {onOpenCommit && (
              <Tooltip label={t("work-file-history-open-commit-under-history")}>
                <span className="row-actions anim inline-flex">
                  <Button size="sm" variant="ghost" className="h-5 px-1" aria-label={t("work-file-history-open-under-history", { short: c.short })} onClick={() => onOpenCommit(c.id)}>
                    <ICON.inspect size={12} aria-hidden />
                  </Button>
                </span>
              </Tooltip>
            )}
          </li>
        ))}
      </ol>
      {commits.length >= HISTORY_CAP && <p className="text-2xs text-text-dim">{t("work-file-history-last-commits-whole-history", { cap: HISTORY_CAP })}</p>}
    </div>
  );
}
