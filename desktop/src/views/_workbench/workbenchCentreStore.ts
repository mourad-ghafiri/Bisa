/**
 * What the Project IDE's centre shows, per root (ide/09 §Agent Mode,
 * ide/18 §The browser tab): documents and terminals, the conversation, or
 * the Board. **Published, never looked up**: the Workbench says it while it
 * is mounted and withdraws it as it goes, the way `chatTargetStore.ts`
 * publishes the conversation on screen — a reader that derived it would
 * need the mode, the settings' default, the Board's switch and the
 * workstream's project, which only the screen has. The doors that show a
 * browser tab read it at that moment (`shell/browserDoors.showBrowserTab`),
 * so nothing subscribes and there is no hook. One Workbench is mounted at a
 * time, the same assumption `workbenchRoot()` makes.
 */

import type { IdeCentre } from "./ideModeModel.mjs";

interface Root {
  readonly scope: string;
  readonly id: string;
}

let current: { root: Root; centre: IdeCentre } | null = null;

const sameRoot = (a: Root, b: Root) => a.scope === b.scope && a.id === b.id;

/** Say what a root's centre shows; answers the withdrawal, which forgets it only while it is still the one published. */
export function publishWorkbenchCentre(root: Root, centre: IdeCentre): () => void {
  current = { root, centre };
  return () => {
    if (current && sameRoot(current.root, root) && current.centre === centre) current = null;
  };
}

/** What the centre of `root` shows — documents when no Workbench on that root has said. */
export function workbenchCentreFor(root: Root | null): IdeCentre {
  return root && current && sameRoot(current.root, root) ? current.centre : "documents";
}
