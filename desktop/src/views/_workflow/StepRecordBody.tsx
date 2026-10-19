/**
 * What a step's record says once it ran: the error it failed with, the
 * answer a person gave, and the output it produced — drawn the same way
 * under a goal's step row (`views/_goal/StepRow.tsx`) and in the panel's
 * *This run* section (`Inspector.tsx`), so a run reads alike on both.
 *
 * The output is long and read seldom: with `fold`, it sits under a kit
 * fold (`SectionHeader` + `useCollapsed`) remembered under that name;
 * without one it is drawn open, as a row that was just expanded wants.
 */

import type { Answer } from "../../types";
import { LinkedText, SectionHeader, useCollapsed } from "../../ui";
import { answerSummary } from "../../askModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export function StepRecordBody({ error, answer, output, fold }: { error: string | null | undefined; answer: Answer | null | undefined; output: unknown; fold?: string }) {
  const said = answerSummary(answer);
  const hasOutput = output !== null && output !== undefined;
  const [folded, toggle] = useCollapsed(fold ?? "", false);
  if (!error && !said && !hasOutput) return null;
  const pre = hasOutput ? <LinkedText as="pre" className="max-h-64 overflow-auto rounded-control bg-surface-2 p-2 font-mono text-2xs" text={JSON.stringify(output, null, 2)} /> : null;
  return (
    <div className="flex flex-col gap-2 text-2xs">
      {error && <p className="text-danger">{error}</p>}
      {said && <p className="text-text-dim">{t("goal-step-row-answered", { said })}</p>}
      {pre && fold ? (
        <div className="flex flex-col gap-1.5">
          <SectionHeader flush title={t("workflow-inspector-output")} open={!folded} onToggle={toggle} />
          {!folded && pre}
        </div>
      ) : (
        pre
      )}
    </div>
  );
}
