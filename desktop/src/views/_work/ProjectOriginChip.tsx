/**
 * Where a project was born, said on the project — `made here`, `from goal …`,
 * or `by a step of …` — each a link to the thing that made it.
 *
 * Origin is history, not a relation: it is recorded at creation by the
 * engine (never accepted from a caller), survives the deletion of what it
 * names, and moves nothing. Attachment answers "who works in it"; this
 * answers "how did it come to exist", which is why the two render apart.
 */
import { href } from "../../router";
import { Chip, ICON } from "../../ui";
import type { ProjectOrigin } from "../../types";
import { madeByStepWords, stepOf } from "./projectOriginModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export function ProjectOriginChip({
  origin,
  goalTitle,
  workflowName,
}: {
  origin: ProjectOrigin;
  /** The goal's label, when the caller has it; the id's tail otherwise. */
  goalTitle?: (id: string) => string;
  /** The workflow's name, when the caller has it. */
  workflowName?: (id: string) => string | undefined;
}) {
  const title = (id: string) => goalTitle?.(id) ?? t("work-project-origin-chip-goal", { slice: id.slice(-6) });
  if (origin.origin === "workspace") {
    return (
      <Chip tone="quiet" icon={ICON.person} title={t("work-project-origin-chip-created-hand-from-workspace")}>{t("work-goal-inspector-made-here")}</Chip>
    );
  }
  if (origin.origin === "goal") {
    // A step of the goal's own design names itself; the chip still
    // points at the goal, since the design is the goal's.
    const by = stepOf(origin);
    return (
      <a href={href({ name: "goal", id: origin.goal })} className="inline-flex min-w-0">
        <Chip tone="quiet" icon={ICON.goal} title={by ? t("work-project-origin-chip-created-step-goal-s-own-design", { step: by.step }) : t("work-project-origin-chip-created-from-goal")} className="hover:bg-surface-2">
          {t("work-project-origin-chip-from-goal", { goal: title(origin.goal) })}
          {by && <span className="font-mono text-text-dim">{t("work-project-origin-chip-step", { step: by.step })}</span>}
        </Chip>
      </a>
    );
  }
  const wf = workflowName?.(origin.workflow) ?? t("work-project-origin-chip-workflow-tail", { tail: origin.workflow.slice(-6) });
  return (
    <a href={href({ name: "workflow", id: origin.workflow })} className="inline-flex min-w-0">
      <Chip
        tone="quiet"
        icon={ICON.workflow}
        title={madeByStepWords(origin, title)}
        className="hover:bg-surface-2"
      >{t("work-project-origin-chip-step-2", { wf })}</Chip>
    </a>
  );
}
