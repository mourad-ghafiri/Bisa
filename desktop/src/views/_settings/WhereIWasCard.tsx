/**
 * Settings › Desktop — *Where you were*: the one action over what the app
 * remembers of where a person was. The memory is always on, so there is no
 * switch here; **Forget where I was** asks first, then forgets and opens
 * the window again on the home (`shell/whereIWas.ts` has the hands,
 * `whereIWasModel.mjs` the list and the order).
 *
 * The confirmation starts as it is written, every time it opens: its one
 * box — *Also discard the messages I was writing* — unchecked.
 */

import { useState } from "react";
import { forgetWhereIWas } from "../../shell/whereIWas";
import { Button, Card, Checkbox, ConfirmDialog, Section, useToast } from "../../ui";
import { attempt } from "../_work/useAsync";
import { t } from "../../i18n/l10n.mjs";

export function WhereIWasCard() {
  const toast = useToast();
  const [asking, setAsking] = useState(false);
  const [drafts, setDrafts] = useState(false);
  const [busy, setBusy] = useState(false);

  const ask = () => {
    setDrafts(false);
    setAsking(true);
  };

  const forget = async () => {
    setAsking(false);
    setBusy(true);
    await attempt(
      () => forgetWhereIWas({ drafts }),
      (why) => toast.error(t("settings-where-i-was-card-could-not-forget", { why })),
      (outcome) => {
        // On `forgotten` the window is already opening again; there is nobody left to tell.
        if (outcome === "unsaved") toast.error(t("settings-where-i-was-card-document-not-saved"));
      },
    );
    setBusy(false);
  };

  return (
    <Section title={t("settings-where-i-was-card-where-you-were")} className="mt-4">
      <Card>
        <div className="flex items-start gap-3">
          <p className="min-w-0 max-w-measure flex-1 text-2xs leading-relaxed text-text-dim">{t("settings-where-i-was-card-blurb")}</p>
          <Button size="sm" disabled={busy} onClick={ask}>{t("settings-where-i-was-card-forget")}</Button>
        </div>
      </Card>
      <ConfirmDialog
        open={asking}
        onClose={() => setAsking(false)}
        onConfirm={() => void forget()}
        title={t("settings-where-i-was-card-forget-title")}
        confirmLabel={t("settings-where-i-was-card-forget-confirm")}
        danger
        body={
          <div className="flex flex-col gap-3">
            <p>{t("settings-where-i-was-card-forget-body")}</p>
            <Checkbox label={t("settings-where-i-was-card-also-discard-messages")} hint={t("settings-where-i-was-card-also-discard-messages-hint")} checked={drafts} onChange={setDrafts} />
          </div>
        }
      />
    </Section>
  );
}
