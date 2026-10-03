/**
 * Run a workflow from the library in the workspace — no goal is captured
 * (03-workflows §Runs of the workspace): where it begins, the workflow's
 * inputs, then go. The run starts at once, beside any other run of it, and
 * the dialog lands on the run's page (`#/runs/<rid>`). A workflow whose
 * steps read the goal it serves never reaches here: its card offers no
 * *Run…* (`workflowVerbs`).
 *
 * A workflow that begins on events may also be tried without waiting for
 * one: *Test: as if … happened* begins at that start with a sample of what
 * its event carries — an editable JSON payload the start's input mapping
 * reads — and its run says *test* wherever runs are listed. A workflow only
 * events begin offers the test runs alone (*Test run…* on its card).
 *
 * Where it begins, what it asks, what it sends and how a refusal reads are
 * `runDialogModel.mjs`'s; this is paint. A start the node refuses because
 * the row was older than the workflow — a step reads its goal now, a
 * problem appeared — is said in words and the row is read again, so the
 * card says what the node said.
 */
import { useEffect, useState } from "react";
import { ApiError, api } from "../../api";
import { navigate } from "../../router";
import type { WorkflowRow } from "../../types";
import { Button, Dialog, Field, Select, TextArea, failureText, useToast } from "../../ui";
import { InputsForm } from "./InputsForm";
import { BY_HAND, askedInputs, entryStart, firstEntry, runEntries, runRefusal, runRequest, sampleText } from "./runDialogModel.mjs";
import { initialValues, type InputValues } from "./workflowForm.mjs";
import { t as tr } from "../../i18n/l10n.mjs";

export function RunWorkflowDialog({ open, row, onClose, onDone }: { open: boolean; row: WorkflowRow; onClose: () => void; onDone?: () => void }) {
  const toast = useToast();
  const workflow = row.workflow;
  const [values, setValues] = useState<InputValues>({});
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const entries = runEntries(workflow);
  const [entry, setEntry] = useState<string>(BY_HAND);
  const [payload, setPayload] = useState("");
  const [payloadError, setPayloadError] = useState<string | null>(null);
  const chosen = entries.find((e) => e.id === entry) ?? null;
  const test = entryStart(workflow, entry) !== null;
  const { asked, mapped } = askedInputs(workflow, entry);
  const askedDefs = (workflow.inputs ?? []).filter((d) => asked.includes(d.name));

  const choose = (next: string) => {
    setEntry(next);
    setPayload(sampleText(workflow, next));
    setPayloadError(null);
    setErrors({});
  };
  useEffect(() => {
    if (open) {
      setValues(initialValues(workflow.inputs ?? []));
      setErrors({});
      choose(firstEntry(workflow) ?? BY_HAND);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- reset on open only
  }, [open]);

  const submit = async () => {
    const request = runRequest(workflow, entry, values, payload);
    if (request.kind === "refused") {
      setErrors(request.errors);
      setPayloadError(request.payloadError);
      return;
    }
    setErrors({});
    setPayloadError(null);
    setBusy(true);
    try {
      const made = request.kind === "test" ? await api.testRunWorkflow(workflow.id, request.body) : await api.runWorkflow(workflow.id, request.inputs);
      toast.ok(request.kind === "test" ? tr("workflow-run-workflow-dialog-test-started") : tr("workflow-run-workflow-dialog-run-has-started"));
      onDone?.();
      navigate({ name: "run", id: made.run.id });
      onClose();
    } catch (e) {
      const refusal = runRefusal(e instanceof ApiError ? e.body : undefined, failureText("workflow", "run-workflow-dialog-failed", e));
      toast.error(refusal.words);
      // The row the card drew was older than the workflow: read it again, and nothing here can start.
      if (refusal.reread) {
        onDone?.();
        onClose();
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={tr("workflow-run-workflow-dialog-run", { workflow: workflow.name })}
      description={test ? tr("workflow-run-workflow-dialog-test-description") : tr("workflow-run-workflow-dialog-runs-in-workspace")}
      footer={
        <>
          <Button variant="ghost" disabled={busy} onClick={onClose}>{tr("workflow-goal-workflow-tab-cancel")}</Button>
          <Button variant="primary" disabled={busy || chosen === null} onClick={() => void submit()}>
            {busy ? tr("workflow-run-workflow-dialog-starting") : test ? tr("workflow-run-workflow-dialog-test-run") : tr("workflow-run-workflow-dialog-run-2")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        {entries.some((e) => e.test) && (
          <Field label={tr("workflow-run-workflow-dialog-entry")} hint={chosen?.hint ?? undefined}>
            <Select value={entry} disabled={busy} onChange={(e) => choose(e.currentTarget.value)}>
              {entries.map((e) => (
                <option key={e.id} value={e.id}>
                  {e.label}
                </option>
              ))}
            </Select>
          </Field>
        )}
        {test && (
          <Field label={tr("workflow-run-workflow-dialog-payload")} hint={tr("workflow-run-workflow-dialog-payload-hint")}>
            <TextArea
              rows={6}
              value={payload}
              disabled={busy}
              spellCheck={false}
              className="font-mono text-2xs"
              aria-invalid={payloadError !== null}
              onChange={(e) => setPayload(e.currentTarget.value)}
            />
            {payloadError && <p className="mt-1 text-2xs text-danger">{payloadError}</p>}
          </Field>
        )}
        {askedDefs.length > 0 ? (
          <InputsForm inputs={askedDefs} values={values} errors={errors} home="workspace" onChange={setValues} disabled={busy} />
        ) : (
          <p className="text-2xs text-text-dim">{tr("workflow-run-workflow-dialog-no-inputs")}</p>
        )}
        {test && mapped > 0 && <p className="text-2xs text-text-dim">{tr("workflow-run-workflow-dialog-from-event", { n: mapped })}</p>}
      </div>
    </Dialog>
  );
}
