/**
 * The leave guard a floating editor's store owns (ide/03 §Tabs, for the
 * Notes and Draw panels): an editor **holds** its document — says whether it
 * is dirty, saves on demand, or lets go — and every way out of the panel
 * asks the guard first. Dirty, the departure is parked and the panel draws
 * the one question (`UnsavedDialog`): *Save* proceeds only once the hold
 * saved clean, *Don't save* lets the hold go and proceeds, *Cancel* keeps the
 * editor as it was. Taking a hold also registers it with `editorRegistry`,
 * so the quit question counts a dirty note or drawing like a document and
 * the close flow saves it — one hold, two readers. The rules are
 * `leaveGuardModel.mjs`'s.
 */

import { useEffect, useRef, useSyncExternalStore } from "react";
import { registerDirtySource, type DirtySource } from "../views/_workbench/editorRegistry";
import { commandForEvent } from "./keymapModel.mjs";
import { holdKey, leaveDecision, type HoldKind } from "./leaveGuardModel.mjs";
import { currentKeymap } from "./shortcuts";
import { isMac } from "../ui/KeyHint";

/** What an editor registers: dirty, save now, or let go of what is unsaved. */
export interface DocumentHold extends DirtySource {
  /** The title the question names. */
  title: () => string;
  /** Forget the unsaved work: nothing more is saved for this document. */
  discard: () => void;
}

/** A departure parked behind the question. */
export interface PendingLeave {
  kind: HoldKind;
  title: string;
  saving: boolean;
}

export interface LeaveGuard {
  /** The editor's hold, for as long as it is mounted; the unregister forgets it. */
  take(key: string, hold: DocumentHold): () => void;
  /** A way out: runs `then` at once, or parks it behind the question while the hold is dirty. */
  leave(then: () => void): void;
  usePending(): PendingLeave | null;
  cancel(): void;
  saveAndGo(): Promise<void>;
  discardAndGo(): void;
}

export function createLeaveGuard(kind: HoldKind): LeaveGuard {
  let hold: DocumentHold | null = null;
  let pending: { then: () => void; saving: boolean } | null = null;
  let snapshot: PendingLeave | null = null;
  const listeners = new Set<() => void>();
  const publish = () => {
    snapshot = pending && hold ? { kind, title: hold.title(), saving: pending.saving } : null;
    for (const l of listeners) l();
  };
  const subscribe = (l: () => void) => {
    listeners.add(l);
    return () => {
      listeners.delete(l);
    };
  };
  const go = () => {
    const then = pending?.then;
    pending = null;
    publish();
    then?.();
  };
  return {
    take(key, next) {
      hold = next;
      const forget = registerDirtySource(key, next);
      return () => {
        forget();
        if (hold === next) {
          hold = null;
          pending = null;
          publish();
        }
      };
    },
    leave(then) {
      if (leaveDecision(hold) === "go") {
        then();
        return;
      }
      pending = { then, saving: false };
      publish();
    },
    usePending() {
      return useSyncExternalStore(subscribe, () => snapshot, () => snapshot);
    },
    cancel() {
      pending = null;
      publish();
    },
    async saveAndGo() {
      if (!pending || !hold) return;
      pending = { ...pending, saving: true };
      publish();
      const clean = await hold.save();
      if (!pending) return;
      if (clean) go();
      else {
        // The save failed or met a conflict: the editor stays, saying why.
        pending = null;
        publish();
      }
    },
    discardAndGo() {
      hold?.discard();
      go();
    },
  };
}

/** The editor's side: hold the document for as long as this mounts. Reads through refs, so the hold is always current. */
export function useDocumentHold(guard: LeaveGuard, kind: HoldKind, id: string, hold: DocumentHold): void {
  const ref = useRef(hold);
  ref.current = hold;
  useEffect(
    () =>
      guard.take(holdKey(kind, id), {
        dirty: () => ref.current.dirty(),
        save: () => ref.current.save(),
        title: () => ref.current.title(),
        discard: () => ref.current.discard(),
      }),
    [guard, kind, id],
  );
}

/**
 * Whether a key is the keymap's `save` — ⌘S by default, a rebinding
 * followed — asked as an editor would, so a floating editor saves on the
 * same chord the IDE's documents do without being a keymap context of its own.
 */
export function isSaveChord(e: { key: string; metaKey: boolean; ctrlKey: boolean; shiftKey: boolean; altKey: boolean }): boolean {
  return commandForEvent(currentKeymap(), e, ["editor"], isMac) === "save";
}
