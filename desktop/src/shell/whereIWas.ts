/**
 * *Forget where I was* (Settings › Desktop): the window's own hands for
 * `whereIWasModel.mjs`, which has the order and the list.
 *
 * What is unsaved is saved first, as on the way out (`useCloseGuard.ts`).
 * Then every memory of *where* is forgotten — the places and what each
 * screen kept (`viewMemoryStore`), the workstream the Project IDE was last
 * in, the conversation each pane was last on — and, when asked, the
 * messages being written; a note's unsaved text is a document's and stays.
 * Then the memories are sealed and the window stands on the home and opens
 * again: what a store or a screen still holds in its hands — the reads and
 * the threads kept for the window's life, a filter a mounted screen shows —
 * goes with the window, and what a screen hands over on its way out is
 * taken by nobody, so nothing forgotten can be written back. The furniture
 * that is no place stays where it is: nothing here names it.
 */

import { errorFields, log } from "../log";
import { draftKey as noteDraftKey } from "../notes/notesModel.mjs";
import { replace } from "../router";
import { forgetEveryDraft } from "../ui";
import { PICK_KEY } from "../views/_studio/conversationSurfaceModel.mjs";
import { forgetEveryRead } from "../views/_work/keptReadsStore";
import { anyDirty, flushAll } from "../views/_workbench/editorRegistry";
import { forgetLastRoot } from "../views/_workbench/lastRootStore";
import { homeRoute } from "./navOrderStore";
import { forgetPref, webStorage } from "./storedPrefModel.mjs";
import { forgetEveryMemory, sealMemories } from "./viewMemoryStore";
import { forgetFlow, type ForgetAsked, type ForgetOutcome } from "./whereIWasModel.mjs";

const flow = forgetFlow({
  dirty: anyDirty,
  save: flushAll,
  forget: {
    memories: () => {
      forgetEveryMemory();
      // What the window kept of the node's answers, and of every thread.
      forgetEveryRead();
    },
    root: forgetLastRoot,
    // One memory for every owner's pick — the IDE's roots, the designer's workflows, the notes and the drawings.
    conversations: () => forgetPref(webStorage(), PICK_KEY),
    // Every note's unsaved text is parked under the same prefix, a key a note.
    drafts: () => void forgetEveryDraft([noteDraftKey("")]),
  },
  failed: (what, e) => log.warn("shell", "a memory could not be forgotten; the rest were", { what, ...errorFields(e) }),
  seal: sealMemories,
  leave: () => {
    replace(homeRoute());
    window.location.reload();
  },
});

/**
 * Forget where the person was. `unsaved` is a document that could not be
 * saved: nothing was forgotten and the window stays, for the caller to say
 * so. On `forgotten` the window is already opening again.
 */
export async function forgetWhereIWas(asked: ForgetAsked): Promise<ForgetOutcome> {
  const outcome = await flow.run(asked);
  log.info("shell", "forget where I was", { outcome, drafts: asked.drafts === true });
  return outcome;
}
