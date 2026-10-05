/**
 * A rendering takes the keyboard when it is shown (ide/03 §Rendered
 * documents) — opened on its rendering, switched to it, its find bar closed
 * — so the find chord finds in it and Space and the arrows scroll it, as a
 * reading surface owes. Never from where the keyboard must stay: a field
 * being typed in, a shell, or the Files tree a person is arrowing through to
 * preview files. The selectors are the keymap's (`keyContextsModel.WITHIN`),
 * so what counts as a shell or a tree is spelt once.
 */

import { WITHIN } from "../../shell/keyContextsModel.mjs";
import { isTypingTarget } from "../../shell/shortcuts";

/** Give `el` the keyboard unless it is somewhere it must stay; whether it moved. */
export function takeKeyboard(el: HTMLElement | null): boolean {
  if (!el) return false;
  const held = document.activeElement;
  if (held && held !== document.body && (isTypingTarget(held) || held.closest(WITHIN.terminal) || held.closest(WITHIN.files))) return false;
  el.focus({ preventScroll: true });
  return document.activeElement === el;
}
