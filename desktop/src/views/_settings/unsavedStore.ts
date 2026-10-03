/**
 * Which Settings forms hold unsaved edits, for the rail to ask before it
 * moves (`unsavedModel.mjs`). A form says so with `useUnsavedForm(id, dirty)`
 * while it is mounted; unmounting clears it, so a panel that is gone never
 * holds the rail back.
 */

import { useEffect, useSyncExternalStore } from "react";
import { markForm } from "./unsavedModel.mjs";

let forms: ReadonlySet<string> = new Set();
const listeners = new Set<() => void>();

function set(next: ReadonlySet<string>) {
  if (next === forms) return;
  forms = next;
  for (const l of listeners) l();
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** The forms holding unsaved edits now. */
function unsavedForms(): ReadonlySet<string> {
  return forms;
}

/** Forget every form's edits — the person chose to leave them. */
export function forgetUnsaved() {
  set(new Set());
}

/** The forms holding unsaved edits, as React state. */
export function useUnsavedForms(): ReadonlySet<string> {
  return useSyncExternalStore(subscribe, unsavedForms);
}

/** Mark the form `id` as holding unsaved edits while `dirty`, and clean once it unmounts. */
export function useUnsavedForm(id: string, dirty: boolean) {
  useEffect(() => {
    set(markForm(forms, id, dirty));
  }, [id, dirty]);
  useEffect(() => () => set(markForm(forms, id, false)), [id]);
}
