/**
 * Turn a library workflow On (03-workflows §Listening): what its start
 * events need that they do not supply — the inputs no event maps and no
 * default fills (`listening_needs`) — and, optionally, what each run it
 * starts may spend; absent, the workspace's default applies. Turning On is
 * no revision of the workflow: the standing is kept apart from it. A public
 * hook start's secret comes back once, here, with its path — the dialog
 * stays open on it until the person has taken it. A ceiling that is no
 * number is said under its field and nothing is sent (`readBudget`).
 */

import { useEffect, useState } from "react";
import { ApiError, api } from "../../api";
import type { HookSecret, WorkflowRow } from "../../types";
import { Button, Dialog, Field, TextInput, failureText, useToast } from "../../ui";
import { runRefusal } from "./runDialogModel.mjs";
import { HookSecretNote } from "./HookSecretNote";
import { InputsForm } from "./InputsForm";
import { listeningFor, neededInputs, readBudget, turnOnBody, type BudgetDraft } from "./listeningModel.mjs";
import { initialValues, toRequest, validateInputs, type InputValues } from "./workflowForm.mjs";
import { t } from "../../i18n/l10n.mjs";

const NO_BUDGET: BudgetDraft = { dollars: "", tokens: "", minutes: "" };

export function TurnOnDialog({ open, row, onClose, onDone }: { open: boolean; row: WorkflowRow; onClose: () => void; onDone?: () => void }) {
  const toast = useToast();
  const needs = neededInputs(row);
  const [values, setValues] = useState<InputValues>({});
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [budget, setBudget] = useState<BudgetDraft>(NO_BUDGET);
  const [budgetErrors, setBudgetErrors] = useState<Partial<Record<keyof BudgetDraft, string>>>({});
  const [busy, setBusy] = useState(false);
  const [secrets, setSecrets] = useState<HookSecret[]>([]);
  useEffect(() => {
    if (open) {
      setValues(initialValues(needs));
      setErrors({});
      setBudget(NO_BUDGET);
      setBudgetErrors({});
      setSecrets([]);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- reset on open only
  }, [open]);

  const submit = async () => {
    const found = validateInputs(needs, values);
    const ceiling = readBudget(budget);
    setErrors(found);
    setBudgetErrors(ceiling.errors);
    if (Object.keys(found).length > 0 || Object.keys(ceiling.errors).length > 0) return;
    setBusy(true);
    try {
      const on = await api.turnOnWorkflow(row.workflow.id, turnOnBody(toRequest(needs, values), ceiling.budget));
      toast.ok(t("workflow-turn-on-dialog-on", { workflow: on.workflow.workflow.name }));
      onDone?.();
      // A secret is shown once: the dialog stays until the person has it.
      if (on.secrets.length > 0) setSecrets(on.secrets);
      else onClose();
    } catch (e) {
      // Refused by name — a step reads its goal, a problem appeared: said in
      // words, and the row the switch drew is read again (`runRefusal`).
      const refusal = runRefusal(e instanceof ApiError ? e.body : undefined, failureText("workflow", "turn-on-dialog-failed", e));
      toast.error(refusal.words);
      if (refusal.reread) {
        onDone?.();
        onClose();
      }
    } finally {
      setBusy(false);
    }
  };

  const taken = secrets.length > 0;
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={t("workflow-turn-on-dialog-title", { workflow: row.workflow.name })}
      description={taken ? t("workflow-turn-on-dialog-secrets") : t("workflow-turn-on-dialog-description", { what: listeningFor(row) })}
      footer={
        taken ? (
          <Button variant="primary" onClick={onClose}>{t("workflow-turn-on-dialog-done")}</Button>
        ) : (
          <>
            <Button variant="ghost" disabled={busy} onClick={onClose}>{t("workflow-goal-workflow-tab-cancel")}</Button>
            <Button variant="primary" disabled={busy} onClick={() => void submit()}>
              {busy ? t("workflow-turn-on-dialog-turning-on") : t("workflow-turn-on-dialog-turn-on")}
            </Button>
          </>
        )
      }
    >
      {taken ? (
        <div className="flex flex-col gap-2">
          {secrets.map((s) => (
            <HookSecretNote key={s.step} secret={s} />
          ))}
        </div>
      ) : (
        <div className="flex flex-col gap-4">
          {needs.length > 0 ? (
            <div className="flex flex-col gap-2">
              <p className="text-2xs text-text-dim">{t("workflow-turn-on-dialog-needs")}</p>
              <InputsForm inputs={needs} values={values} errors={errors} home="workspace" onChange={setValues} disabled={busy} />
            </div>
          ) : (
            <p className="text-2xs text-text-dim">{t("workflow-turn-on-dialog-no-needs")}</p>
          )}
          <Field label={t("workflow-turn-on-dialog-budget")} hint={t("workflow-turn-on-dialog-budget-hint")}>
            <div className="grid gap-2 md:grid-cols-3">
              <TextInput inputMode="decimal" value={budget.dollars} disabled={busy} placeholder={t("workflow-turn-on-dialog-dollars")} aria-label={t("workflow-turn-on-dialog-dollars")} aria-invalid={budgetErrors.dollars !== undefined} onChange={(e) => setBudget({ ...budget, dollars: e.target.value })} />
              <TextInput inputMode="numeric" value={budget.tokens} disabled={busy} placeholder={t("workflow-turn-on-dialog-tokens")} aria-label={t("workflow-turn-on-dialog-tokens")} aria-invalid={budgetErrors.tokens !== undefined} onChange={(e) => setBudget({ ...budget, tokens: e.target.value })} />
              <TextInput inputMode="decimal" value={budget.minutes} disabled={busy} placeholder={t("workflow-turn-on-dialog-minutes")} aria-label={t("workflow-turn-on-dialog-minutes")} aria-invalid={budgetErrors.minutes !== undefined} onChange={(e) => setBudget({ ...budget, minutes: e.target.value })} />
            </div>
            {/* One sentence per field that is no ceiling: nothing is sent while one stands. */}
            {[...new Set(Object.values(budgetErrors))].map((why) => (
              <p key={why} className="mt-1 text-2xs text-danger">{why}</p>
            ))}
          </Field>
        </div>
      )}
    </Dialog>
  );
}
