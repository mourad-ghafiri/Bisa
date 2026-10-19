import type { ReactNode } from "react";

/**
 * A label for a control that is not one control.
 *
 * {@link Field} renders a real `<label>`, which is exactly right for an input
 * and wrong for a composite: a picker or an ordered list is a dozen buttons
 * and several inputs, and wrapping all of them in one `<label>` points every
 * stray click at whichever labelable descendant happens to come first. This
 * is the same visual shape — dim caption above, hint below — built from a
 * `<div>`, so a group of controls reads as a field without pretending to be
 * one.
 *
 * `error` is a reason the group refused what was done to it — a branch name
 * taken, a path that is no path — in the danger ink under the group, as
 * `Field`'s is under its control, and announced.
 */
export function Labelled({
  label,
  hint,
  error,
  children,
}: {
  label: string;
  hint?: ReactNode;
  /** Why the group refused or put back a value — shown in danger ink and announced. */
  error?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="block">
      <span className="mb-1 block text-2xs font-medium text-text-dim">{label}</span>
      {children}
      {error && (
        <span role="alert" className="mt-1 block text-2xs text-danger">
          {error}
        </span>
      )}
      {hint && <span className="mt-1 block text-2xs text-text-dim">{hint}</span>}
    </div>
  );
}
