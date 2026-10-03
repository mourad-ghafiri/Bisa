/**
 * What an agent remembers — owner view, read-only.
 *
 * Records link to each other with `[[slug]]`. Anything nothing points at is
 * surfaced as an orphan for review and never deleted automatically: a memory
 * the graph forgot is exactly the kind of thing a human should look at before
 * it disappears.
 */

import { useState } from "react";
import { api } from "../../api";
import type { RecallRecord } from "../../types";
import { Chip, EmptyState, ErrorNote, ICON, RelativeTime, SectionHeader, SkeletonRows, Tooltip } from "../../ui";
import { useAsync } from "./useAsync";
import { t } from "../../i18n/l10n.mjs";

function Record({ r, known }: { r: RecallRecord; known: Set<string> }) {
  const [open, setOpen] = useState(false);
  return (
    <li className="border-b border-hairline py-1.5 last:border-0">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        className="anim flex w-full items-center gap-2 rounded-sm text-left hover:bg-surface-2"
      >
        <ICON.collapsed
          size={11}
          aria-hidden
          className={`anim w-3 shrink-0 text-text-dim ${open ? "rotate-90" : ""}`}
        />
        <span className="truncate font-mono text-2xs">{r.slug}</span>
        {r.orphan && <Chip tone="warn">{t("work-recall-panel-orphan")}</Chip>}
        <RelativeTime at={r.updated_at} className="ml-auto" />
      </button>
      {open && (
        <div className="mt-1 pl-5">
          <pre className="max-h-56 overflow-auto rounded-control bg-surface-2 p-2 font-mono text-2xs whitespace-pre-wrap">
            {r.value}
          </pre>
          {r.links.length > 0 && (
            <p className="mt-1 flex flex-wrap items-center gap-1 text-2xs text-text-dim">
              {t("work-recall-panel-links-to")}
              {r.links.map((l) =>
                known.has(l) ? (
                  <Chip key={l} tone="quiet">
                    {l}
                  </Chip>
                ) : (
                  <Tooltip key={l} label={t("work-recall-panel-no-record-slug-link-points-nothing")}>
                    <span>
                      <Chip tone="danger" icon={ICON.warn}>
                        {l}
                      </Chip>
                    </span>
                  </Tooltip>
                ),
              )}
            </p>
          )}
        </div>
      )}
    </li>
  );
}

export function RecallPanel({ agent }: { agent: string }) {
  const { data, error, loading, reload } = useAsync((s) => api.recall(agent, s), [agent]);

  if (loading) return <SkeletonRows rows={5} />;
  if (error) return <ErrorNote error={error} retry={reload} />;

  const records = data?.records ?? [];
  if (records.length === 0) {
    return (
      <EmptyState
        icon={ICON.agent}
        title={t("work-recall-panel-nothing-remembered-yet")}
        hint={t("work-recall-panel-recall-fills-agent-works-only-can")}
        action={null}
      />
    );
  }

  const known = new Set(records.map((r) => r.slug));
  const rooted = records.filter((r) => !r.orphan);
  const orphans = records.filter((r) => r.orphan);

  return (
    <div className="flex flex-col gap-4">
      <ul>
        {rooted.map((r) => (
          <Record key={r.slug} r={r} known={known} />
        ))}
      </ul>
      {orphans.length > 0 && (
        <section>
          {/* `SectionHeader` has no tone; the wrapper recolours its label so
              an orphan list still reads as the warning it is. */}
          <div className="[&_span]:text-warn">
            <SectionHeader flush title={t("work-recall-panel-orphans")} count={orphans.length} />
          </div>
          <p className="mb-1 text-2xs text-text-dim">{t("work-recall-panel-nothing-links-these-they-kept-until")}</p>
          <ul>
            {orphans.map((r) => (
              <Record key={r.slug} r={r} known={known} />
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}
