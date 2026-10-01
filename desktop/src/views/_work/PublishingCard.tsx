/**
 * The project's publishing policy, under About › Settings — where a push or a
 * pull request is decided about, beside the repository it leaves from. The
 * select edits the view's draft; the toolbar's Save writes it
 * (`PATCH /projects/{pid}`), like everything else on the view.
 */
import type { PublishPolicy } from "../../types";
import { Chip, GATE_ICON, ICON, Select } from "../../ui";
import { PUBLISH_LABEL, PUBLISH_MEANING, PUBLISH_POLICIES, PUBLISH_TONE } from "./publishPolicy.mjs";
import { t } from "../../i18n/l10n.mjs";

const OPTION_LABEL: Record<PublishPolicy, string> = {
  gated: t("work-new-project-dialog-gated-human-approves-each-push"),
  manual: t("work-new-project-dialog-manual-never-publishes-from-here"),
  auto: t("work-new-project-dialog-automatic-pushes-without-asking"),
};

export function PublishingCard({
  policy,
  drafted,
  onEdit,
}: {
  /** What the project holds. */
  policy: PublishPolicy;
  /** What the person picked, or null. */
  drafted: PublishPolicy | null;
  onEdit: (next: PublishPolicy) => void;
}) {
  const shown = drafted ?? policy;
  const icon = shown === "auto" ? ICON.warn : shown === "manual" ? ICON.person : GATE_ICON.publish;
  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex flex-wrap items-center gap-2">
        <Chip tone={PUBLISH_TONE[shown]} icon={icon}>
          {PUBLISH_LABEL[shown]}
        </Chip>
        <Select aria-label={t("work-publishing-card-publishing-policy")} value={shown} onChange={(e) => onEdit(e.target.value as PublishPolicy)} className="max-w-xs">
          {PUBLISH_POLICIES.map((p) => (
            <option key={p} value={p}>
              {OPTION_LABEL[p]}
            </option>
          ))}
        </Select>
        {drafted !== null && drafted !== policy && <Chip tone="accent">{t("work-connection-card-changed")}</Chip>}
      </div>
      <p className="text-2xs text-text-dim">{PUBLISH_MEANING[shown]}</p>
    </div>
  );
}
