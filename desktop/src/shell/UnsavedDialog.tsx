/**
 * The one question a departure asks over unsaved work (ide/03 §Tabs): the
 * IDE's tabs, the Notes editor and the Draw editor draw this, never a
 * question of their own — *Cancel* (the safe way out), *Don't save* (the
 * loss, said plainly), *Save* (primary). The words are the caller's: the
 * workbench's `guardWords` over its tabs, a panel's `leaveWords` over its
 * record. Three buttons, the kit's ceiling for a footer.
 */

import { GUARD_VERBS } from "../views/_workbench/closeGuardModel.mjs";
import { Button, Dialog } from "../ui";

export function UnsavedDialog({
  open,
  words,
  onSave,
  onDiscard,
  onCancel,
  saving = false,
}: {
  open: boolean;
  words: { title: string; description: string; note: string } | null;
  onSave: () => void;
  onDiscard: () => void;
  onCancel: () => void;
  /** A save in flight: *Save* waits, so the question is not answered twice. */
  saving?: boolean;
}) {
  return (
    <Dialog
      open={open}
      onClose={onCancel}
      title={words?.title ?? ""}
      description={words?.description}
      width="max-w-md"
      footer={
        <>
          <Button variant="ghost" onClick={onCancel}>
            {GUARD_VERBS.cancel}
          </Button>
          <Button onClick={onDiscard} disabled={saving}>
            {GUARD_VERBS.discard}
          </Button>
          <Button variant="primary" onClick={onSave} disabled={saving}>
            {GUARD_VERBS.save}
          </Button>
        </>
      }
    >
      <p className="max-w-measure text-2xs leading-relaxed text-text-dim">{words?.note}</p>
    </Dialog>
  );
}
