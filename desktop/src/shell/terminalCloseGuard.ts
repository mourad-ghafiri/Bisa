/**
 * One place decides whether closing needs a confirmation, and one pending
 * question serves every confirmation the app asks before something ends.
 *
 * A terminal/harness is closed from four surfaces — the rail menu, the split
 * strip and its X, the centre strip and its menu, and the keyboard verb. A
 * per-site dialog would be that prompt written four times, so instead every
 * site calls `requestClose*` here. Whether a close asks is the model's fact
 * (`closeGuardModel.confirmsClose`: a live tab whose kind's switch is on —
 * `terminal.confirm_close` for a shell, `terminal.confirm_terminate` for a
 * harness); the words are the model's too. An exited tab, or a switched-off
 * kind, closes at once. The quit flow (`useCloseGuard`) asks through the
 * same `askClose`, so the app has one confirmation dialog
 * (`CloseConfirmDialog`), mounted once.
 */

import { useSyncExternalStore } from "react";
import { closeQuestion, confirmsClose, othersQuestion, type CloseQuestion } from "./closeGuardModel.mjs";
import { confirmPrefs } from "./closeGuardSettings";
import { closeOtherTerminalTabs, closeTerminalTab, terminalSessions } from "./useTerminals";

/** A question waiting on the person's answer. */
export interface PendingClose extends CloseQuestion {
  /** The answer: confirmed, or dismissed. */
  resolve: (ok: boolean) => void;
}

let pending: PendingClose | null = null;
const listeners = new Set<() => void>();

function emit(): void {
  for (const l of listeners) l();
}

function setPending(next: PendingClose | null): void {
  pending = next;
  emit();
}

/**
 * Ask, and wait for the answer. A question asked while one is already up
 * dismisses the earlier one — two confirmations at once would be two
 * dialogs over each other, and the later act is the one meant.
 */
export function askClose(question: CloseQuestion): Promise<boolean> {
  pending?.resolve(false);
  return new Promise((resolve) => setPending({ ...question, resolve }));
}

/** Close one terminal tab — asking first when its kind asks and it is live. */
export function requestCloseTerminal(key: string): void {
  const session = terminalSessions().find((s) => s.key === key);
  if (!session || !confirmsClose(session, confirmPrefs())) {
    closeTerminalTab(key);
    return;
  }
  void askClose(closeQuestion(session)).then((ok) => {
    if (ok) closeTerminalTab(key);
  });
}

/** Close the other tabs rooted where this one is — asking first when any of them would. */
export function requestCloseOthers(key: string): void {
  const me = terminalSessions().find((s) => s.key === key);
  if (!me) return;
  const prefs = confirmPrefs();
  const asking = terminalSessions().filter((s) => s.key !== key && s.scope === me.scope && s.id === me.id && confirmsClose(s, prefs)).length;
  if (asking === 0) {
    closeOtherTerminalTabs(key);
    return;
  }
  void askClose(othersQuestion(asking)).then((ok) => {
    if (ok) closeOtherTerminalTabs(key);
  });
}

/** Dismiss without closing. */
export function cancelClose(): void {
  const p = pending;
  setPending(null);
  p?.resolve(false);
}

/** Answer yes and clear the question. */
export function confirmClose(): void {
  const p = pending;
  setPending(null);
  p?.resolve(true);
}

/** The pending question, for the one dialog that renders it. */
export function usePendingClose(): PendingClose | null {
  return useSyncExternalStore(
    (cb) => {
      listeners.add(cb);
      return () => {
        listeners.delete(cb);
      };
    },
    () => pending,
    () => null,
  );
}
