/**
 * A file pasted or dropped onto a terminal is its path, as a Mac terminal
 * types it (ide/06 §The GPU, links and the keyboard): a file copied in the
 * file manager and pasted with ⌘V, a file or several dragged onto the
 * terminal, and a picture with no file behind it — a screenshot — saved to a
 * file of its own first. Claude Code and the other harnesses read such a path
 * as the file itself, an image attached.
 *
 * The web engine hands a paste or a drop a file's name and bytes and never
 * its path, so the shell is asked (`api.pasteboardHolds`, `api.droppedPaths`,
 * `api.pasteImageToTemp`); what is typed, and how a path is quoted, is
 * `pastedPathsModel`'s. Everything goes through `term.paste` — the door a
 * paste takes, bracketed when the program asked for it — never around it.
 * A paste of plain text is left to xterm exactly as before: it is not
 * touched, and not delayed by a question to the shell.
 */

import type { Terminal as XTerminal } from "@xterm/xterm";
import { droppedPaths, inDesktopShell, pasteImageToTemp, pasteboardHolds } from "../api";
import { sayFailure, toaster } from "../ui";
import { pastedImageName } from "../ui/pastedImageModel.mjs";
import { isFileDrop, pathsForDrop } from "../views/_workbench/dropModel.mjs";
import { pasteAsksShell, pasteChoice, typedPaths } from "./pastedPathsModel.mjs";
import { t } from "../i18n/l10n.mjs";

/**
 * Take pastes that carry files or a picture, and drops of files, on `host` —
 * the element the terminal is opened in — for `term`. `alive` answers whether
 * the mount still stands, read when the shell's answer lands. Answers the
 * teardown, which the mount runs with the rest of its own.
 */
export function attachPathInput(host: HTMLElement, term: XTerminal, alive: () => boolean): () => void {
  // Capture, so this runs before xterm's own paste handler on its textarea
  // and can keep it from typing a copied file's bare name.
  const onPaste = (e: ClipboardEvent) => {
    const data = e.clipboardData;
    if (!data || !inDesktopShell()) return;
    const text = data.getData("text/plain");
    if (!pasteAsksShell({ types: data.types, text })) return;
    e.preventDefault();
    e.stopPropagation();
    void pasteFromShell(term, text, alive);
  };
  const onDragOver = (e: DragEvent) => {
    if (!e.dataTransfer || !isFileDrop(e.dataTransfer.types)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
  };
  const onDrop = (e: DragEvent) => {
    if (!e.dataTransfer || !isFileDrop(e.dataTransfer.types)) return;
    e.preventDefault();
    e.stopPropagation();
    const names = Array.from(e.dataTransfer.files).map((f) => f.name);
    void droppedPaths().then((paths) => {
      if (!alive()) return;
      const dropped = pathsForDrop(names, paths);
      if (dropped.length === 0) {
        toaster.info(t("terminal-path-input-drop-no-path"));
        return;
      }
      term.paste(typedPaths(dropped));
      term.focus();
    });
  };
  host.addEventListener("paste", onPaste, true);
  host.addEventListener("dragover", onDragOver);
  host.addEventListener("drop", onDrop);
  return () => {
    host.removeEventListener("paste", onPaste, true);
    host.removeEventListener("dragover", onDragOver);
    host.removeEventListener("drop", onDrop);
  };
}

/** Ask the shell what the clipboard holds, and type what the paste stands for (`pasteChoice`). */
async function pasteFromShell(term: XTerminal, text: string, alive: () => boolean): Promise<void> {
  const choice = pasteChoice(await pasteboardHolds(), text);
  if (!alive()) return;
  if (choice.kind === "paths") {
    term.paste(typedPaths(choice.paths));
    return;
  }
  if (choice.kind === "text") {
    term.paste(choice.text);
    return;
  }
  try {
    const path = await pasteImageToTemp(pastedImageName());
    if (alive()) term.paste(typedPaths([path]));
  } catch (e) {
    toaster.error(sayFailure("terminal", t("terminal-path-input-picture-not-saved"), e));
  }
}
