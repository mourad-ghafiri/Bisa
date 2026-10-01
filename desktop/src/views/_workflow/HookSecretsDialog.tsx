/**
 * The secrets an adoption minted, shown once (`hookSecretsStore`): mounted at
 * the app's root, so it stays after the card that decided the adoption has
 * left the screen. *Done* forgets them — nothing reads a secret back.
 */

import { Button, Dialog } from "../../ui";
import { t } from "../../i18n/l10n.mjs";
import { HookSecretNote } from "./HookSecretNote";
import { dismissHookSecrets, useShownHookSecrets } from "./hookSecretsStore";

export function HookSecretsDialog() {
  const secrets = useShownHookSecrets();
  return (
    <Dialog
      open={secrets.length > 0}
      onClose={dismissHookSecrets}
      title={t("workflow-hook-secrets-dialog-title")}
      description={t("workflow-start-run-dialog-secrets")}
      footer={<Button variant="primary" onClick={dismissHookSecrets}>{t("workflow-turn-on-dialog-done")}</Button>}
    >
      <div className="flex flex-col gap-2">
        {secrets.map((s) => (
          <HookSecretNote key={s.path} secret={s} />
        ))}
      </div>
    </Dialog>
  );
}
