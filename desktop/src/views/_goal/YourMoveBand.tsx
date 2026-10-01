/**
 * Everything owed to a person on this goal — or on this run of the
 * workspace — in one band under the header: every open question and gate
 * as an answerable card — the very card the Inbox shows, so answering here
 * or there is one act, decided through each ask's own home. The node's
 * `needs_actions_for` feeds both, live gates or the asks rebuilt from the
 * run, so a question from a previous session shows here too. Hidden when
 * nothing is owed; folded to one line when more than two things are.
 */
import type { InputDef, NeedsAction } from "../../types";
import { Button, ICON, useCollapsed } from "../../ui";
import { NeedsActionCard } from "../_studio/NeedsAction";
import { namesAdoption } from "./proposalRouting.mjs";
import { t } from "../../i18n/l10n.mjs";

const YOUR_MOVE_ATTR = "data-your-move";

export function YourMoveBand({
  actions,
  adoptInputs,
  onResolved,
}: {
  actions: NeedsAction[];
  /** The proposed workflow's inputs, asked for beside an adoption's Approve. */
  adoptInputs: InputDef[] | null;
  onResolved: () => void;
}) {
  const [collapsed, toggle] = useCollapsed("goal.yourmove", false);
  if (actions.length === 0) return null;
  const many = actions.length > 2;
  return (
    <section
      {...{ [YOUR_MOVE_ATTR]: "" }}
      aria-label={t("goal-your-move-band-move")}
      className="sticky top-0 z-10 max-h-[40vh] overflow-y-auto border-b border-warn/40 bg-surface/95 px-4 py-3 backdrop-blur"
    >
      <div className="mb-2 flex items-center gap-2">
        <ICON.waiting size={13} aria-hidden className="text-warn" />
        <h3 className="text-2xs font-semibold tracking-wide text-warn uppercase">{t("goal-your-move-band-move")}</h3>
        <span className="tnum text-2xs text-text-dim">
          {actions.length === 1 ? t("goal-your-move-band-one-thing-waits") : t("goal-your-move-band-things-wait", { actions: actions.length })}
        </span>
        {many && (
          <Button size="sm" variant="ghost" className="ml-auto" onClick={toggle}>
            {collapsed ? t("goal-your-move-band-show") : t("goal-your-move-band-fold")}
          </Button>
        )}
      </div>
      {!(many && collapsed) && (
        <div className="flex flex-col gap-2">
          {actions.map((a, i) => (
            <NeedsActionCard
              key={a.gate_id ?? `durable:${a.step ?? i}`}
              action={a}
              inputs={namesAdoption(a.subject) ? (adoptInputs ?? []) : undefined}
              onResolved={onResolved}
            />
          ))}
        </div>
      )}
    </section>
  );
}

/** Bring the band into view — the step list's "decide it" gesture. */
export function scrollToYourMove(): void {
  document.querySelector(`[${YOUR_MOVE_ATTR}]`)?.scrollIntoView({ behavior: "smooth", block: "start" });
}
