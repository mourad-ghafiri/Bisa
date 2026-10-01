/**
 * The one place the desktop asks whether the person prefers reduced motion
 * (`prefers-reduced-motion: reduce`): the pet's sprite stands still, a
 * streamed reply is shown as it lands rather than paced. False where the
 * question cannot be asked.
 */
export function prefersReducedMotion(): boolean {
  try {
    return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  } catch {
    return false;
  }
}
