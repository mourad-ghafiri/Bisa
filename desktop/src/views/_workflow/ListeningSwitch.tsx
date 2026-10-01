/**
 * A library workflow's On/Off, in the designer's header (03-workflows
 * §Listening): *Off*; *On — begins on the schedule … · next Mon 09:00*;
 * *Can't turn on: 2 problems*; *Paused: a run it started failed*, with
 * *Listen again*. Turning On opens `TurnOnDialog` — the inputs its events do
 * not supply, a per-run budget; turning Off is at once. Nothing is drawn
 * for a workflow with nothing to hear, and a goal's own design has none:
 * its goal listens. The words are `listeningModel.mjs`'s; the listeners —
 * when it next comes due — are read while it is On. A secret *Listen again*
 * mints is handed to `hookSecretsStore`, which shows it once.
 */

import { useState } from "react";
import { api } from "../../api";
import type { ListenerView, WorkflowRow } from "../../types";
import { Button, ICON, Switch, useToast } from "../../ui";
import { attempt } from "../_work/useAsync";
import { TurnOnDialog } from "./TurnOnDialog";
import { mintedBy } from "./hookSecretsModel.mjs";
import { showHookSecrets } from "./hookSecretsStore";
import { againBody, switchState } from "./listeningModel.mjs";
import { t } from "../../i18n/l10n.mjs";

const TONE_TEXT = { quiet: "text-text-dim", ok: "text-ok", warn: "text-warn", danger: "text-danger" } as const;

export function ListeningSwitch({ row, listeners, onChanged }: { row: WorkflowRow; listeners: readonly ListenerView[]; onChanged: () => void }) {
  const toast = useToast();
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);
  // A goal's own design listens through its goal, never through a switch of its own.
  if (row.workflow.origin.origin === "goal") return null;
  const state = switchState(row, [...listeners]);
  if (state.state === "none") return null;
  const on = state.state === "on" || state.state === "paused";

  const turnOff = async () => {
    setBusy(true);
    await attempt(() => api.turnOffWorkflow(row.workflow.id), toast.error, () => {
      toast.ok(t("workflow-workflow-card-turned-off", { workflow: row.workflow.name }));
      onChanged();
    });
    setBusy(false);
  };
  const again = async () => {
    setBusy(true);
    await attempt(() => api.turnOnWorkflow(row.workflow.id, againBody(row.listening)), toast.error, (on) => {
      toast.ok(t("workflow-listening-switch-listening-again"));
      // A public hook drawn while it was paused has its secret minted now: shown once, by the dialog at the app's root.
      showHookSecrets(mintedBy(on));
      onChanged();
    });
    setBusy(false);
  };

  return (
    <div className="flex min-w-0 max-w-md items-center gap-2" data-listening-switch={state.state}>
      <ICON.signal size={13} aria-hidden className={TONE_TEXT[state.tone]} />
      <Switch
        checked={on}
        disabled={busy || state.toggle === null}
        label={state.words}
        className={`min-w-0 text-2xs ${TONE_TEXT[state.tone]}`}
        onChange={(checked) => {
          if (checked && state.toggle === "on") setAsking(true);
          else if (!checked && state.toggle === "off") void turnOff();
        }}
      />
      {state.again && (
        <Button size="sm" variant="ghost" disabled={busy} onClick={() => void again()}>
          <ICON.restart size={12} aria-hidden />
          {t("workflow-listening-switch-listen-again")}
        </Button>
      )}
      <TurnOnDialog open={asking} row={row} onClose={() => setAsking(false)} onDone={onChanged} />
    </div>
  );
}
