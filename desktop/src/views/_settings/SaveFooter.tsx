/**
 * The footer of an explicit-save form in Settings — *Who commits*, your
 * profile, a list of security rules: one shape, right-aligned under the
 * form, so a person finds Save where they found it on the last panel.
 *
 * While the form holds edits it says *Unsaved changes* beside Discard and
 * Save, and it tells the rail (`unsavedStore`), which asks before leaving the
 * panel — a draft that vanished with a click on the rail was a draft lost.
 */

import { Button } from "../../ui";
import { useUnsavedForm } from "./unsavedStore";
import { t } from "../../i18n/l10n.mjs";

export function SaveFooter({
  form,
  dirty,
  saving,
  canSave = true,
  blockedReason,
  saveLabel = t("settings-decisions-panel-save"),
  onSave,
  onDiscard,
  submit = false,
}: {
  /** The form's name for the rail: unique on the screen. */
  form: string;
  dirty: boolean;
  saving: boolean;
  /** False while what is typed cannot be saved; `blockedReason` says why. */
  canSave?: boolean;
  blockedReason?: string;
  saveLabel?: string;
  /** Omitted when the footer sits in a `<form>` and Save submits it (`submit`). */
  onSave?: () => void;
  onDiscard?: () => void;
  submit?: boolean;
}) {
  useUnsavedForm(form, dirty);
  const blocked = dirty && !canSave;
  return (
    <div className="flex flex-wrap items-center justify-end gap-2 border-t border-hairline pt-3">
      {dirty && (
        <span role="status" className="text-2xs text-text-dim">
          {t("settings-save-footer-unsaved")}
        </span>
      )}
      {dirty && onDiscard && (
        <Button size="sm" variant="ghost" disabled={saving} onClick={onDiscard}>
          {t("settings-global-git-panel-discard")}
        </Button>
      )}
      <Button
        size="sm"
        variant="primary"
        type={submit ? "submit" : "button"}
        disabled={!dirty || !canSave || saving}
        disabledReason={blocked ? blockedReason : undefined}
        onClick={submit ? undefined : onSave}
      >
        {saving ? t("settings-connectors-panel-saving") : saveLabel}
      </Button>
    </div>
  );
}
