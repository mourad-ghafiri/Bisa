/**
 * *Remove <project>?* — the one question every surface asks before a project
 * goes: the rail's menu and the project's own page. Archive it, forget it, or
 * delete its folder, each saying what it does (`ChoiceDialog`); the words,
 * the order and what is held are `removeProjectModel.mjs`'s.
 *
 * This asks and nothing else: the caller runs the act it is handed, with its
 * own toasts and navigation. A folder going for good — deleting to the Trash
 * is off here — is asked about once more, since nothing brings it back.
 */

import { useState } from "react";
import { useResolvedSettings } from "../../shell/settingsStore";
import { boolOf } from "../../shell/settingsModel.mjs";
import { ChoiceDialog, ConfirmDialog } from "../../ui";
import { asksAgain, forGoodWords, removeChoices, type RemoveAct } from "./removeProjectModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export interface RemovableProject {
  name: string;
  archived: boolean;
  /** Adopted, not created here: its folder is never deleted from here. */
  adopted: boolean;
  /** Where the folder is, when the caller knows. */
  path?: string | null;
  /** How many checkouts it holds, when the caller knows. */
  checkouts?: number;
}

export function RemoveProjectDialog({
  project,
  onClose,
  onChoose,
}: {
  /** The project being asked about; null while nothing is. */
  project: RemovableProject | null;
  onClose: () => void;
  /** The act chosen — and, for a folder going for good, confirmed — with how the folder goes here. */
  onChoose: (act: RemoveAct, trash: boolean) => void;
}) {
  const { resolved } = useResolvedSettings(null);
  const trash = boolOf(resolved, "editor.delete.trash", true);
  const [confirming, setConfirming] = useState(false);
  const words = forGoodWords({ path: project?.path, checkouts: project?.checkouts });
  const close = () => {
    setConfirming(false);
    onClose();
  };
  return (
    <>
      <ChoiceDialog
        open={project !== null && !confirming}
        onClose={close}
        title={project ? t("work-remove-project-dialog-remove", { project: project.name }) : ""}
        choices={
          project
            ? removeChoices({ archived: project.archived, adopted: project.adopted, trash, checkouts: project.checkouts }).map((choice) => ({
                ...choice,
                onSelect: () => {
                  if (asksAgain(choice.id, trash)) setConfirming(true);
                  else onChoose(choice.id, trash);
                },
              }))
            : []
        }
      />
      <ConfirmDialog
        open={project !== null && confirming}
        onClose={close}
        danger
        title={words.title}
        confirmLabel={words.confirmLabel}
        body={<p className="break-words">{words.body}</p>}
        onConfirm={() => {
          setConfirming(false);
          onChoose("delete", trash);
        }}
      />
    </>
  );
}
