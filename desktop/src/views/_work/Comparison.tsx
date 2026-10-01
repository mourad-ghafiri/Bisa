/**
 * The two whole texts of one file's change compared in the code editor,
 * read-only, unchanged regions folded — two columns, or one with the lines
 * interleaved — under the sentence a binary file, an empty side or a cut
 * calls for (`sidesWords`). A binary file is the sentence alone: there is
 * no text to draw. The patch document draws one for a changed file (ide/04
 * §The Changes view), the commit document for one file of a commit (ide/05
 * §Click to inspect); the sides come from `GET /git/sides` or `GET
 * /git/commit/{sha}/sides`, the same shape past the keys.
 */

import { DiffEditor } from "../../ui";
import { sidesWords } from "./patchViewModel.mjs";
import type { PatchSides } from "./patchViewModel.mjs";

export function Comparison({ path, sides, inline }: { path: string; sides: PatchSides; inline: boolean }) {
  const words = sidesWords(sides);
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2">
      {words && (
        <p className="text-2xs text-text-dim" role="status">
          {words}
        </p>
      )}
      {!sides.binary && <DiffEditor original={sides.original ?? ""} modified={sides.modified ?? ""} path={path} readOnly inline={inline} foldUnchanged className="min-h-0 flex-1 rounded-control border border-border" />}
    </div>
  );
}
