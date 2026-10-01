/**
 * The design a goal's Workflow tab is drawing — its undo history — kept per
 * goal rather than in the tab's own state, so a switch to another tab, a
 * `?step=` chip or an engine event that remounts the tab brings the drawing
 * back untouched.
 *
 * The **drawing as it stands** also outlives the window: its present — never
 * its undo history — is kept in the view memory's documents
 * (`shell/viewMemoryStore`, `docViews`) under the goal's place, and comes
 * back after a restart as a history of one entry. Nothing here reaches the
 * node: a draft is saved when the person presses *Save changes*, and is
 * dropped — here and in the memory — when they cancel, it is saved, or the
 * goal starts a run; a running workflow is not edited from the desktop.
 *
 * **Bounded.** The histories are held for the newest `MAX_DRAFTS` goals
 * drawn: a window that lives for weeks does not keep every design it ever
 * showed. A goal past the cap has lost its undo history and nothing else —
 * its drawing is in the memory, and is read back as after a restart.
 */

import { useCallback, useSyncExternalStore } from "react";
import { Lru } from "../../shell/lru.mjs";
import { docViews, placeOf } from "../../shell/viewMemoryStore";
import type { NewWorkflowBody } from "../../types";
import { MAX_DRAFTS, draftBody } from "./designDraftModel.mjs";
import { create, type History } from "./history.mjs";

type Draft = History<NewWorkflowBody> | null;

/** The name the drawing is kept under, beside the goal's other documents. */
const DRAWING = "workflow:draft";

/** Per goal, the newest drawn kept: the draft, or `null` once looked for and not found — so the memory is read once a goal. */
const drafts = new Lru<Draft>(MAX_DRAFTS);
const listeners = new Set<() => void>();

const placeOfGoal = (goal: string): string => placeOf({ name: "goal", id: goal });

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

function emit() {
  for (const l of listeners) l();
}

/** The design drawn for `goal`, or `null` when none is in progress. */
export function draftOf(goal: string): Draft {
  const held = drafts.get(goal);
  if (held !== undefined) return held;
  // Not looked for yet in this window: what the last one left, as a history of one entry.
  const body = draftBody(docViews.read(placeOfGoal(goal), DRAWING));
  const restored = body ? create(body) : null;
  drafts.set(goal, restored);
  return restored;
}

/** Keep `next` as the goal's draft; `null` drops it, and what the memory kept of it. */
export function setDraft(goal: string, next: Draft): void {
  if (draftOf(goal) === next) return;
  drafts.set(goal, next);
  // Quietly: the tab follows this store, never the memory.
  docViews.keepQuietly(placeOfGoal(goal), DRAWING, next ? next.present : null);
  emit();
}

/**
 * The goal's draft and its setter, the `useState` shape — a functional
 * update reads the store, so undo and redo compose the way they did.
 */
export function useDesignDraft(goal: string): [Draft, (next: Draft | ((h: Draft) => Draft)) => void] {
  const current = useSyncExternalStore(subscribe, () => draftOf(goal), () => null);
  const set = useCallback(
    (next: Draft | ((h: Draft) => Draft)) => {
      setDraft(goal, typeof next === "function" ? next(draftOf(goal)) : next);
    },
    [goal],
  );
  return [current, set];
}
