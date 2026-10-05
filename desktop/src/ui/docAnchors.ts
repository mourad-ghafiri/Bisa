/**
 * A heading a same-document link names (ide/03 §Rendered documents): found
 * under the rendering by its id — the anchor `headingAnchorsModel` gave it,
 * matched without regard to case — and scrolled into the rendering's own
 * scrollport. Never `location.hash`: the window is the router's, and a hash
 * that names nothing lands on home.
 */

import { yieldKeptScroll } from "./useKeptScroll";

/**
 * Scroll the rendering under `root` to the element whose id is `fragment`;
 * whether one was there. The kept scroll yields first (`yieldKeptScroll`), so
 * a restore still putting the document back does not fight the reveal.
 */
export function scrollToFragment(root: HTMLElement, fragment: string): boolean {
  const want = fragment.toLowerCase();
  for (const el of root.querySelectorAll<HTMLElement>("[id]")) {
    if (el.id.toLowerCase() !== want) continue;
    yieldKeptScroll(root);
    el.scrollIntoView({ block: "start" });
    return true;
  }
  return false;
}
