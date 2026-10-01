/**
 * The conversation on screen (ide/18 §Annotating a page): which surface's
 * composer a tray beside the screen — the Browser pane's annotations —
 * attaches its chips to. **Published, never looked up**: `Conversation`
 * says its kind and id while it is mounted and withdraws them as it goes,
 * the way the Details pane hands its slot over; a reader that guessed from
 * the route would have to know every screen's shape, and the Inbox shows
 * its rows' conversations under one address.
 */

import { useSyncExternalStore } from "react";

export interface ChatTarget {
  readonly kind: string;
  readonly id: string;
}

let current: ChatTarget | null = null;
const listeners = new Set<() => void>();

function set(next: ChatTarget | null): void {
  if (next === current || (next && current && next.kind === current.kind && next.id === current.id)) return;
  current = next;
  for (const l of listeners) l();
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

/** Say a conversation is on screen; answers the withdrawal, which forgets it only while it is still the one published. */
export function publishChatTarget(target: ChatTarget): () => void {
  set(target);
  return () => {
    if (current && current.kind === target.kind && current.id === target.id) set(null);
  };
}

export function useChatTarget(): ChatTarget | null {
  return useSyncExternalStore(subscribe, () => current, () => null);
}

/** The conversation on screen, outside React. */
export function chatTargetNow(): ChatTarget | null {
  return current;
}
