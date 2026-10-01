/**
 * Class name joining, with Tailwind conflicts resolved.
 *
 * `clsx` flattens conditionals; `twMerge` then keeps the *last* utility in
 * each conflicting group. That second half is the reason this exists: every
 * component in the kit takes a `className` from its caller and appends it, and
 * with plain string concatenation `"px-3" + " px-2"` leaves both in the class
 * list and lets specificity order — not the caller — decide which wins. A
 * view that passes `px-2` to override padding would get it about half the
 * time, depending on the order Tailwind happened to emit the two rules in.
 */

import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}
