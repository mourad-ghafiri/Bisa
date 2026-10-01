/**
 * A file git has never seen, drawn as the patch git would write for it
 * (ide/04 §The Changes view). `GET /git/diff` answers an untracked path with
 * no patch — a GET does not stage, and git cannot diff a file it has not
 * seen without staging it — so the document reads the file and builds the
 * unified diff of a new file itself: `--- /dev/null`, every line an
 * addition, the no-newline marker where git would put one. The one patch
 * surface (`HunkDiff`) then reads a new file exactly as it reads a committed
 * one. Plain `.mjs`, so `node --test` reads it.
 */

import { formatSize } from "../../ui/fileTreeModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The `@@` range git writes: a count of one is left off. */
function range(start, count) {
  return count === 1 ? `${start}` : `${start},${count}`;
}

/**
 * The unified diff of a new file: the header git writes, then one hunk of
 * additions — or the header alone for an empty file, which has no hunk.
 * A trailing newline ends the last line and is not a line of its own; a
 * `\r` stays on its line, as git keeps it.
 * @param {string} path
 * @param {string} text
 * @returns {string}
 */
export function newFilePatch(path, text) {
  const header = [`diff --git a/${path} b/${path}`, t("work-new-file-patch-new-file-mode-100644"), "--- /dev/null", `+++ b/${path}`];
  if (text === "") return header.join("\n") + "\n";
  const ends = text.endsWith("\n");
  const lines = (ends ? text.slice(0, -1) : text).split("\n");
  const body = [`@@ -0,0 +${range(1, lines.length)} @@`, ...lines.map((l) => `+${l}`)];
  if (!ends) body.push("\\ No newline at end of file");
  return [...header, ...body].join("\n") + "\n";
}

/**
 * What the document says above a new file's patch: that git has never seen
 * it and that it is staged whole; a binary file says its size instead of a
 * patch; a file cut at the read cap says the first part is what is shown.
 * @param {{ binary?: boolean, size?: number, truncated?: boolean } | null | undefined} file
 * @returns {{ sentence: string, binary: boolean, cut: boolean }}
 */
export function newFileWords(file) {
  const binary = file?.binary === true;
  if (binary) {
    return {
      sentence: t("work-new-file-patch-new-binary-file-git-has-never", { file: formatSize(file?.size ?? 0) }),
      binary,
      cut: false,
    };
  }
  return {
    sentence: t("work-new-file-patch-new-file-git-has-never-seen"),
    binary,
    cut: file?.truncated === true,
  };
}
