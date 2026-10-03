/**
 * The bar at the top of About's *Checkout* and *Settings* views (ide/04):
 * how many changes wait, **Discard**, and the one **Save** that writes them
 * all — sticky under the About header, so it is there however far a person
 * scrolled. On the Settings view, *Approve on this machine* when a synced
 * script waits and nothing is drafted. The words are
 * `projectSettingsDraftModel.toolbarWords`'s.
 */

import { Button, ICON, cn } from "../../ui";
import { toolbarWords } from "./projectSettingsDraftModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export function SettingsToolbar({
  count,
  valid,
  saving,
  onSave,
  onDiscard,
  awaitingApproval = false,
  onApprove,
  /** What a save is refused for, in a sentence, when it is. */
  problem = null,
  whole = true,
}: {
  count: number;
  valid: boolean;
  saving: boolean;
  onSave: () => void;
  onDiscard: () => void;
  awaitingApproval?: boolean;
  onApprove?: () => void;
  problem?: string | null;
  /** Every read the count compares against has landed; until then *All saved* is not said. */
  whole?: boolean;
}) {
  const words = toolbarWords(count, saving, whole);
  const dirty = count > 0;
  return (
    <div className="sticky top-7 z-10 -mx-2 flex items-center gap-2 border-b border-hairline bg-surface px-2 py-1.5">
      <span className={cn("flex min-w-0 items-center gap-1 text-2xs", dirty ? "text-text" : "text-text-dim")}>
        {dirty ? <ICON.edit size={12} aria-hidden className="shrink-0" /> : <ICON.ok size={12} aria-hidden className="shrink-0 text-ok" />}
        <span className="truncate">{words.status}</span>
      </span>
      {dirty && !valid && problem && <span className="min-w-0 truncate text-2xs text-danger">{problem}</span>}
      <span className="flex-1" />
      {dirty && (
        <Button size="sm" variant="ghost" disabled={saving} onClick={onDiscard}>{t("work-settings-toolbar-discard")}</Button>
      )}
      {dirty ? (
        <Button size="sm" variant="primary" disabled={saving || !valid} onClick={onSave}>
          {words.save}
        </Button>
      ) : awaitingApproval && onApprove ? (
        <Button size="sm" variant="primary" disabled={saving} onClick={onApprove}>{t("work-settings-toolbar-approve-machine")}</Button>
      ) : null}
    </div>
  );
}
