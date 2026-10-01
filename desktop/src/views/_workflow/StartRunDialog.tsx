/**
 * Start a run: the workflow's inputs, then go — at once when nothing is live
 * on the goal, queued behind its live run when `queues` says one is.
 *
 * Also the Adopt form when `adopt` is given: the same inputs, sent with the
 * approval of the adoption gate, because adopting a proposal *is* starting
 * its run with what it needs.
 *
 * A design that begins on events is started by listening (`listen`): the
 * goal's start arms its start events rather than running — adopted or not —
 * and each occurrence runs it on the goal; the inputs asked are then the
 * ones its events do not supply, the host passing only those. A public
 * hook's secret minted by the arming is shown once, here, and the dialog
 * stays open on it until the person has taken it. Once the goal listens, a
 * run by hand is *Run now* at the design's start by hand (`at`).
 */

import { useEffect, useState } from "react";
import { api } from "../../api";
import type { HookSecret, InputDef } from "../../types";
import { Button, Dialog, useToast } from "../../ui";
import { attempt } from "../_work/useAsync";
import { HookSecretNote } from "./HookSecretNote";
import { InputsForm } from "./InputsForm";
import { initialValues, toRequest, validateInputs, type InputValues } from "./workflowForm.mjs";
import { t } from "../../i18n/l10n.mjs";

export function StartRunDialog({
  open,
  goal,
  name,
  inputs,
  adopt,
  queues = false,
  listen = false,
  at = null,
  onClose,
  onDone,
}: {
  open: boolean;
  goal: string;
  /** The workflow's name, for the title. */
  name: string;
  inputs: InputDef[];
  /** When set, this dialog decides the adoption gate rather than starting a run. */
  adopt?: { gate: string | null } | null;
  /** A run is live: this one queues behind it and starts when it ends. */
  queues?: boolean;
  /** The design begins on events: the start arms them — the goal listens — rather than running. */
  listen?: boolean;
  /** A listening goal's run by hand, at this start step (*Run now*). */
  at?: string | null;
  onClose: () => void;
  onDone: () => void;
}) {
  const toast = useToast();
  const [values, setValues] = useState<InputValues>({});
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const [secrets, setSecrets] = useState<HookSecret[]>([]);

  // The form is reset when the dialog opens — not on every refetch of the
  // goal behind it, whose `inputs` is a fresh array each time and would wipe
  // what the person has typed.
  useEffect(() => {
    if (open) {
      setValues(initialValues(inputs));
      setErrors({});
      setSecrets([]);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- reset on open only
  }, [open]);

  const submit = async () => {
    const found = validateInputs(inputs, values);
    setErrors(found);
    if (Object.keys(found).length > 0) return;
    setBusy(true);
    const body = toRequest(inputs, values);
    const did = { queued: false, minted: [] as HookSecret[] };
    const ok = await attempt<unknown>(
      async () => {
        if (adopt) {
          const decided = await api.decide(goal, { approve: true, inputs: body, ...(adopt.gate ? { gate: adopt.gate } : {}) });
          did.minted = decided.secrets ?? [];
          return decided;
        }
        if (at) {
          const made = await api.startGoalRunAt(goal, { inputs: body, start: at });
          did.queued = made.status === "queued";
          return made;
        }
        // Arming answers no run to read: the goal listens, and says the secrets it minted.
        const made = await api.startRun(goal, body);
        did.queued = made.status === "queued";
        did.minted = made.secrets ?? [];
        return made;
      },
      toast.error,
    );
    setBusy(false);
    if (ok) {
      toast.ok(
        adopt
          ? listen
            ? t("workflow-start-run-dialog-adopted-listening")
            : t("workflow-start-run-dialog-adopted-run-has-started")
          : listen
            ? t("workflow-start-run-dialog-listening")
            : did.queued
              ? t("workflow-start-run-dialog-queued-starts-when-live-run-finishes")
              : t("workflow-run-workflow-dialog-run-has-started"),
      );
      onDone();
      // A secret is shown once: the dialog stays until the person has it.
      if (did.minted.length > 0) setSecrets(did.minted);
      else onClose();
    }
  };

  // Once it listens, the goal behind the dialog says so and `listen` moves: the secrets' words are their own.
  if (secrets.length > 0) {
    return (
      <Dialog
        open={open}
        onClose={onClose}
        title={t("workflow-start-run-dialog-secrets-title", { name })}
        description={t("workflow-start-run-dialog-secrets")}
        footer={<Button variant="primary" onClick={onClose}>{t("workflow-turn-on-dialog-done")}</Button>}
      >
        <div className="flex flex-col gap-2">
          {secrets.map((s) => (
            <HookSecretNote key={s.step} secret={s} />
          ))}
        </div>
      </Dialog>
    );
  }

  const title = adopt
    ? listen
      ? t("workflow-start-run-dialog-adopt-listen", { name })
      : t("workflow-start-run-dialog-adopt-start", { name })
    : listen
      ? t("workflow-start-run-dialog-listen", { name })
      : at
        ? t("workflow-start-run-dialog-run-now", { name })
        : t("workflow-start-run-dialog-start", { name });
  const description = adopt
    ? listen
      ? t("workflow-start-run-dialog-adopt-listen-description")
      : t("workflow-start-run-dialog-workflow-agent-proposed-shape-adopting-records")
    : listen
      ? t("workflow-start-run-dialog-listen-description")
      : queues
        ? t("workflow-start-run-dialog-run-live-one-starts-when-finishes")
        : at
          ? t("workflow-start-run-dialog-run-now-description")
          : t("workflow-start-run-dialog-run-starts-once-these-inputs");
  const go = adopt
    ? listen
      ? t("workflow-start-run-dialog-adopt-listen-2")
      : t("workflow-start-run-dialog-adopt-start-2")
    : listen
      ? t("workflow-start-run-dialog-listen-2")
      : queues
        ? t("workflow-run-workflow-dialog-queue-run")
        : at
          ? t("workflow-start-run-dialog-run-now-2")
          : t("workflow-start-run-dialog-start-run");

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={title}
      description={description}
      footer={
        <>
          <Button variant="ghost" disabled={busy} onClick={onClose}>{t("workflow-goal-workflow-tab-cancel")}</Button>
          <Button variant="primary" disabled={busy} onClick={() => void submit()}>
            {busy ? (queues ? t("workflow-start-run-dialog-queuing") : t("workflow-run-workflow-dialog-starting")) : go}
          </Button>
        </>
      }
    >
      {listen && inputs.length === 0 ? (
        <p className="text-2xs text-text-dim">{t("workflow-turn-on-dialog-no-needs")}</p>
      ) : (
        <InputsForm inputs={inputs} values={values} errors={errors} home="goal" onChange={setValues} disabled={busy} />
      )}
    </Dialog>
  );
}
