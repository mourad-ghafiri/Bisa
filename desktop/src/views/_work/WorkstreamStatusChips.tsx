/**
 * The facts a workstream carries, as chips (ide/07): its state, its
 * pull request (a link to the code host), the agents in it, where it stands
 * against its base and its upstream, what is staged (`S`), modified (`M`) and
 * untracked (`?`), conflicts, an operation left half-done. One component for
 * the project's rows and the workstream panel's header, over one model
 * (`workstreamCardModel.mjs`), so the two never disagree about what "+3" means.
 */

import type { Workstream, WorkstreamStatus } from "../../types";
import { Chip, ExternalLink, Tooltip } from "../../ui";
import { cardChips } from "./workstreamCardModel.mjs";

export function WorkstreamStatusChips({
  w,
  s,
  omit = [],
}: {
  /** The record; `null` when only the live status is at hand. */
  w?: Workstream | null;
  s: WorkstreamStatus | null | undefined;
  /** Chip ids the host already shows elsewhere (the panel's own state chip). */
  omit?: readonly string[];
}) {
  const chips = cardChips(w ?? null, s).filter((c) => !omit.includes(c.id));
  if (chips.length === 0) return null;
  return (
    <span className="tnum inline-flex shrink-0 flex-wrap items-center gap-1 text-2xs">
      {chips.map((c) => (
        <Tooltip key={c.id} label={c.title}>
          {c.href ? (
            <ExternalLink href={c.href} className="inline-flex">
              <Chip tone={c.tone}>{c.text}</Chip>
            </ExternalLink>
          ) : (
            <span className="inline-flex">
              <Chip tone={c.tone}>{c.text}</Chip>
            </span>
          )}
        </Tooltip>
      ))}
    </span>
  );
}
