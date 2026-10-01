/**
 * A public hook's secret, shown once — at turn-on, at an adoption, after a
 * rotation — with the path it guards: the one moment a person can copy it.
 * The caller outside sends it as `X-Bisa-Token`, or signs the body with it
 * (`X-Hub-Signature-256`). Nothing keeps it on screen after this.
 */

import type { HookSecret } from "../../types";
import { Button, ICON, copyText, useToast } from "../../ui";
import { t } from "../../i18n/l10n.mjs";

export function HookSecretNote({ secret }: { secret: HookSecret }) {
  const toast = useToast();
  return (
    <div className="flex flex-col gap-1.5 rounded-control border border-warn/40 bg-warn-soft px-2 py-1.5 text-2xs">
      <p className="flex items-center gap-1.5 font-medium text-warn">
        <ICON.key size={12} aria-hidden />
        {t("workflow-hook-secret-note-copy-now")}
      </p>
      <p className="text-text-dim">{t("workflow-hook-secret-note-path", { path: secret.path })}</p>
      <div className="flex items-center gap-2">
        <code className="min-w-0 flex-1 truncate rounded-control bg-surface px-1.5 py-0.5 font-mono text-text">{secret.secret}</code>
        <Button
          size="sm"
          variant="ghost"
          aria-label={t("workflow-hook-secret-note-copy")}
          onClick={() => void copyText(secret.secret).then((ok) => (ok ? toast.ok(t("workflow-hook-secret-note-copied")) : toast.error(t("workflow-hook-secret-note-not-copied"))))}
        >
          <ICON.copy size={12} aria-hidden />
        </Button>
      </div>
      <p className="text-text-dim">{t("workflow-hook-secret-note-headers")}</p>
    </div>
  );
}
