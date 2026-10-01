/**
 * A parallel gateway: every flow out of it at once. It has nothing to set —
 * the form says what it does in words, how many paths it takes, and where
 * they meet again: the step they flow into joins them, by its own join
 * (every flow that can still arrive, by default).
 */

import type { Step } from "../../../types";
import { t } from "../../../i18n/l10n.mjs";

type Parallel = Extract<Step, { kind: "parallel" }>;

export function ParallelStepForm({ step }: { step: Parallel }) {
  const paths = (step.then ?? []).filter((f) => !f.branch).length;
  return (
    <div className="flex flex-col gap-2 text-2xs text-text-dim">
      <p>{t("workflow-parallel-step-form-takes-paths", { n: paths })}</p>
      <p>{t("workflow-parallel-step-form-join-in-words")}</p>
    </div>
  );
}
