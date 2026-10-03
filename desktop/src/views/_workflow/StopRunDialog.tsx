/**
 * *Stop* asks first. A stopped run is cancelled and its sessions end — a
 * restart starts a new one, it never resumes this one — so the Runs pane and
 * the run's page confirm the way *Stop every run* does (`WorkflowCard`),
 * and the stop itself is the one hook's (`useRunVerbs`).
 */

import { ConfirmDialog } from "../../ui";
import { t } from "../../i18n/l10n.mjs";

export function StopRunDialog({ run, onClose, onStop }: { run: string | null; onClose: () => void; onStop: (run: string) => void }) {
  return (
    <ConfirmDialog
      open={run !== null}
      onClose={onClose}
      title={t("workflow-stop-run-dialog-title")}
      body={t("workflow-stop-run-dialog-body")}
      confirmLabel={t("workflow-stop-run-dialog-confirm")}
      danger
      onConfirm={() => {
        onClose();
        if (run !== null) onStop(run);
      }}
    />
  );
}
