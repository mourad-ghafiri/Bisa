/**
 * Which keymap scopes are live for a key (ide/15 §Dispatch), read from the
 * element the key landed on: `workbench` while the IDE is on a root, `tabs`
 * while a root's strip is on screen, `editor` inside Monaco, `document`
 * inside the editor, a rendering or a conflict document — wherever one is
 * drawn, on any route — `terminal` inside a terminal panel, `browser`
 * inside a browser tab's body, `files` inside a Files tree's frame,
 * `designer` while the Workflow Designer is on screen
 * (`[data-designer-screen]`). One function, so
 * the window listener and a terminal's key reservation read the same scopes;
 * the caller says whether the IDE is on a root (`shortcuts.workbenchRoot`),
 * so this module reads the DOM alone — which scopes follow from what it read
 * is `keyContextsModel.scopesOf`.
 */

import { WITHIN, scopesOf } from "./keyContextsModel.mjs";
import type { WithinFact } from "./keyContextsModel.mjs";

/** Whether the Workflow Designer is on screen — the `designer` scope's fact. */
export function designerOnScreen(): boolean {
  return document.querySelector("[data-designer-screen]") !== null;
}

/** The scopes live for a key that landed on `target`, `global` left implicit; `root` — the IDE is on a root. */
export function contextsOf(target: EventTarget | null, root: boolean): string[] {
  const node = target as HTMLElement | null;
  const within: Partial<Record<WithinFact, boolean>> = {};
  for (const fact of Object.keys(WITHIN) as WithinFact[]) within[fact] = Boolean(node?.closest?.(WITHIN[fact]));
  return scopesOf({
    root,
    strip: Boolean(document.querySelector("[data-tab-strip]")),
    designer: Boolean(document.querySelector("[data-designer-screen]")),
    within,
  });
}
