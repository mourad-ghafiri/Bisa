/**
 * A picture pasted into a text field, named before it is taken (guide/the-
 * desktop.md §Conversations). One hook for every place a paste lands —
 * the composer under every conversation, the New Goal dialog: the paste's
 * files go to `take` as they are; its pictures wait, one at a time, for a
 * name in a `PromptDialog` pre-filled with the suggested one, stem
 * selected, so Enter keeps it; a paste carrying neither files nor text
 * asks the shell, which reads the clipboard's picture the web engine may
 * hold back (`api.pasteboardImage`, ide/01). Text pastes are untouched.
 */

import { useCallback, useState, type ClipboardEvent, type ReactNode } from "react";
import { inDesktopShell, pasteboardImage } from "../api";
import { PromptDialog } from "./Dialog";
import { renameSelection } from "./fileTreeMutations.mjs";
import { extensionOf, imageNameError, pastedImageName, pasteIntake, suggestedName, withExtension } from "./pastedImageModel.mjs";
import { t } from "../i18n/l10n.mjs";

export interface PastedImages {
  /** The text field's paste handler. */
  onPaste: (e: ClipboardEvent<HTMLElement>) => void;
  /** The naming dialog — rendered by the caller, open while a picture waits. */
  dialog: ReactNode;
}

const PNG = "image/png";

export function usePastedImages(take: (files: File[]) => void): PastedImages {
  const [queue, setQueue] = useState<File[]>([]);

  const enqueue = useCallback((pictures: File[]) => {
    if (pictures.length) setQueue((q) => [...q, ...pictures]);
  }, []);

  const onPaste = useCallback(
    (e: ClipboardEvent<HTMLElement>) => {
      const hasText = e.clipboardData.getData("text/plain").length > 0;
      const { pictures, files, askShell } = pasteIntake(Array.from(e.clipboardData.files), hasText);
      if (files.length || pictures.length) {
        e.preventDefault();
        if (files.length) take(files);
        enqueue(pictures);
        return;
      }
      if (!askShell || !inDesktopShell()) return;
      void pasteboardImage().then((bytes) => {
        if (bytes) enqueue([new File([bytes.slice().buffer as ArrayBuffer], pastedImageName(), { type: PNG })]);
      });
    },
    [enqueue, take],
  );

  const head = queue[0] ?? null;
  const offered = head ? suggestedName(head.name) : "";
  const next = () => setQueue((q) => q.slice(1));
  const name = (value: string) => {
    if (!head) return;
    take([new File([head], withExtension(value, extensionOf(head.name) || "png"), { type: head.type || PNG })]);
    next();
  };

  const dialog = (
    <PromptDialog
      open={head !== null}
      onClose={next}
      onSubmit={name}
      title={t("ui-use-pasted-images-name-picture")}
      description={t("ui-use-pasted-images-name-conversation-agents-see-under")}
      label={t("ui-use-pasted-images-file-name")}
      initial={offered}
      selection={head ? renameSelection(offered) : undefined}
      mono
      submitLabel={t("ui-use-pasted-images-attach")}
      validate={(v) => imageNameError(v, [])}
    />
  );

  return { onPaste, dialog };
}
