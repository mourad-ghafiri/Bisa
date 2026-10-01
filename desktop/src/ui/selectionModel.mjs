/**
 * When a `click` is really the end of a text selection.
 *
 * A rendering turns every path and URL into a link (ide/17), and a browser
 * fires `click` at the end of a drag whose two ends share an ancestor — and
 * on the second press of a double-click. A handler that opens the link on
 * any click inside an anchor therefore opens it when a person was only
 * selecting the words: the card takes the focus and the selection is gone.
 * So a click that leaves text selected inside the container it was heard in
 * is the selection's, not the link's. A plain click leaves the selection
 * collapsed, and opens the link as ever.
 *
 * No DOM types here: the two arguments are read through the little of
 * `Selection` and `Node` that matters, so `node --test` reads this.
 */

/**
 * @param {{isCollapsed: boolean, rangeCount: number, getRangeAt(i: number): {commonAncestorContainer: unknown}} | null | undefined} selection the window's
 * @param {{contains(node: unknown): boolean} | null | undefined} container the element the click was heard on
 * @returns {boolean} whether the click ended a selection of text inside the container
 */
export function endsSelection(selection, container) {
  if (!selection || !container || selection.isCollapsed || selection.rangeCount === 0) return false;
  for (let i = 0; i < selection.rangeCount; i++) {
    const at = selection.getRangeAt(i).commonAncestorContainer;
    // The range sits inside the container, or spans it from outside: text of it is selected either way.
    if (container.contains(at) || (at && typeof at.contains === "function" && at.contains(container))) return true;
  }
  return false;
}
