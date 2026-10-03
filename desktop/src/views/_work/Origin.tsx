/**
 * Where a definition came from, said on the definition itself.
 *
 * `builtin: bool` used to answer this, and it could only ever say "seeded".
 * It could not name *which* bundled definition an object was, and it could
 * not tell the one agent that must always exist from the catalog's, which
 * need not. `origin` says all three things, so the badge says all three: the
 * platform's own agent, an entry installed from the catalog under a named
 * slug, and something written here. Collapsing them back into one chip would
 * throw away exactly the distinction the type was introduced to carry.
 *
 * Provenance is recorded, never accepted — the node sets `origin` at creation
 * and preserves it from the stored record on every update, like `created_at`.
 * So a badge here is a fact about history and not a lock: an installed
 * definition is yours to edit like any other one. The two core agents are the
 * exception, and it is an exception about *permanence*, which is why they
 * read differently from the other two origins.
 */

import { CORE_AGENT_IDS, catalogSlug, type AgentOrigin } from "../../types";
import { Chip } from "../../ui";
import { t } from "../../i18n/l10n.mjs";

/** What a core agent is for, in the one line a detail pane has room for. */
export const CORE_AGENT_PURPOSE =
  t("work-origin-one-platform-s-own-agents-general");

/** Why a field on the core agent's editor is read and not written. */
export const CORE_AGENT_LOCKED =
  t("work-origin-fixed-platform-s-own-agent-only");

/**
 * Why the core agent has no Delete and no Disable, said where the buttons
 * would otherwise be. A control that exists only to refuse you is worse than
 * no control, so nothing is rendered disabled; this sentence stands in for
 * both.
 */
export const CORE_AGENT_PERMANENT = t("work-origin-core-agents-permanent", { agents: CORE_AGENT_IDS.join(" and ") });

/**
 * One chip for the three origins.
 *
 * `id` is the object's own id where the caller has it. A catalog entry
 * installs under its slug, so the slug is redundant next to an id that
 * already shows it and worth spelling out where it does not — that is the
 * whole reason the prop exists.
 */
export function OriginChip({ origin, id }: { origin: AgentOrigin; id?: string }) {
  if (origin === "core") {
    return (
      <Chip tone="neutral" title={CORE_AGENT_PURPOSE}>{t("work-origin-platform-agent")}</Chip>
    );
  }
  const slug = catalogSlug(origin);
  if (slug !== null) {
    return (
      <Chip
        tone="quiet"
        title={t("work-origin-installed-from-catalog-entry-editing-here", { slug })}
      >
        {slug === id ? t("work-origin-catalog") : t("work-origin-catalog-slug", { slug })}
      </Chip>
    );
  }
  return (
    <Chip tone="quiet" title={t("work-origin-written-workspace-nothing-catalog-answers")}>{t("work-origin-created-here")}</Chip>
  );
}
