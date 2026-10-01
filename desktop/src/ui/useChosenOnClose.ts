/**
 * A menu's chosen action, run once the menu has left — shared by the
 * dropdown ({@link Menu}) and the right-click menu ({@link ContextMenu}),
 * which offer the same items and must run them the same way.
 *
 * Radix hands focus back to the trigger as the menu leaves, and its focus
 * trap holds until then; an action that mounts a field (a rename, a new
 * file's name) would have it blurred away before a letter is typed. So the
 * action waits for the menu's last word, `onCloseAutoFocus`, and takes the
 * focus the trigger would have taken. Escape and a click outside choose
 * nothing, and the trigger gets focus back as before. An `immediate` item is
 * the one exception: it runs in the gesture, because a file dialog opens in
 * no other moment.
 */

import { useRef } from "react";
import type { MenuItem } from "./Menu";

export interface ChosenOnClose {
  /** An item was picked: run it now when it says so, else keep it for the close. */
  readonly select: (item: MenuItem) => void;
  /** The menu's last word: run what was kept, taking the focus the trigger would have. */
  readonly onCloseAutoFocus: (e: Event) => void;
}

export function useChosenOnClose(): ChosenOnClose {
  const chosen = useRef<(() => void) | null>(null);
  return {
    select: (item) => {
      if (item.immediate) item.onSelect();
      else chosen.current = item.onSelect;
    },
    onCloseAutoFocus: (e) => {
      const run = chosen.current;
      if (!run) return;
      chosen.current = null;
      e.preventDefault();
      run();
    },
  };
}
