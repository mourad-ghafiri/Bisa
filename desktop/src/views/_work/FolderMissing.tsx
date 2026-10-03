/**
 * A checkout whose folder is not on disk, said once for every surface that
 * reads it — the Files tree, the Git occupant and About › Checkout. Not a
 * fault in red: the folder is the workstream's to lay down, so this is an
 * open door to its panel, where the checkout is made, with the path it is
 * waiting at.
 */

import { showRightPanel } from "../_workbench/rightPanelStore";
import { Button, EmptyState, ICON, cn } from "../../ui";
import { t } from "../../i18n/l10n.mjs";

export function FolderMissing({ path, onOpen, className }: { path?: string | null; onOpen?: () => void; className?: string }) {
  return (
    <EmptyState
      icon={ICON.folder}
      title={t("work-folder-missing-title")}
      hint={t("work-folder-missing-hint")}
      className={cn("py-6", className)}
      action={
        <>
          {path && <code className="basis-full break-all font-mono text-2xs text-text-dim">{path}</code>}
          {/* The workstream's own panel on this root, unless the caller knows a nearer way there. */}
          <Button size="sm" onClick={onOpen ?? (() => showRightPanel("workstreams"))}>
            <ICON.workstream size={12} aria-hidden />
            {t("work-folder-missing-open")}
          </Button>
        </>
      }
    />
  );
}
