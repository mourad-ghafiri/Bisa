/**
 * The head a library designer was left with in `conflict` or `rejected`
 * (`designerSession.parked`): no save on the way out would carry it, so it
 * is parked here for the life of the window and handed back, once, to the
 * next designer opened on the same workflow (`designerSession.openWithMine`).
 * Bounded like the goals' drafts; nothing reaches the node or the memory.
 */

import { Lru } from "../../shell/lru.mjs";
import type { NewWorkflowBody } from "../../types";
import { MAX_DRAFTS } from "./designDraftModel.mjs";

const parked = new Lru<NewWorkflowBody>(MAX_DRAFTS);

/** Keep `body` for the workflow, replacing what was kept. */
export function parkDesign(workflow: string, body: NewWorkflowBody): void {
  parked.set(workflow, body);
}

/** What was kept for the workflow, taken: a second take finds nothing. */
export function takeParkedDesign(workflow: string): NewWorkflowBody | null {
  const body = parked.get(workflow) ?? null;
  if (body) parked.delete(workflow);
  return body;
}
