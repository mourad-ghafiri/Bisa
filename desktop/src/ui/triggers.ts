/**
 * What a menu or a popover was handed as its trigger.
 *
 * Radix renders the trigger as a `<button>` of its own, so a caller that
 * hands it a styled `<span>` gets a keyboard-reachable control for free.
 * A caller that hands it a button — a bare `<button>`, or the kit's
 * {@link Button} — would get a button inside a button: invalid HTML, which
 * React reports on every render, and two controls where a screen reader
 * expects one. `Menu` and `Popover` read the trigger here and, when it is
 * already a button, make *it* the trigger (`asChild`) instead of nesting it.
 */

import { isValidElement, type ReactElement, type ReactNode } from "react";
import { Button } from "./Button";

/** Whether `node` is a button element the trigger can become rather than contain. */
export function isButtonElement(node: ReactNode): node is ReactElement {
  return isValidElement(node) && (node.type === "button" || node.type === Button);
}
