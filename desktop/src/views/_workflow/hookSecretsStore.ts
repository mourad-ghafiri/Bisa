/**
 * The public hook secrets waiting to be shown — once.
 *
 * A secret is minted when a listener is armed and answered by the act that
 * armed it. A dialog that armed it shows it itself (`TurnOnDialog`,
 * `StartRunDialog`); an ask card cannot — an adoption decided from the Inbox
 * or a goal's *Your move* band is gone the moment it is decided, and the
 * card with it — and neither can the capture dialog, which closes on the
 * goal it made. So each hands what its act minted here, and one dialog at
 * the app's root (`HookSecretsDialog`) shows it until the person has taken
 * it.
 *
 * Memory only, for the life of the window: a secret is never written to
 * storage, and dismissing forgets it.
 */

import { useSyncExternalStore } from "react";
import type { HookSecret } from "../../types";
import { shownWith } from "./hookSecretsModel.mjs";

const NONE: readonly HookSecret[] = Object.freeze([]);
let shown: readonly HookSecret[] = NONE;
const listeners = new Set<() => void>();

function set(next: readonly HookSecret[]): void {
  shown = next;
  for (const l of listeners) l();
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

/** The secrets waiting to be shown, oldest first. */
export function useShownHookSecrets(): readonly HookSecret[] {
  return useSyncExternalStore(subscribe, () => shown, () => shown);
}

/** Show these secrets beside any still on screen (`shownWith`). Nothing for none. */
export function showHookSecrets(minted: readonly HookSecret[]): void {
  const next = shownWith(shown, minted);
  if (next !== shown) set(next);
}

/** The person has them: forgotten. */
export function dismissHookSecrets(): void {
  if (shown.length > 0) set(NONE);
}
