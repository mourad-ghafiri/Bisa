/**
 * A field that commits on blur — a JSON body, a number, a template, an id —
 * holds a draft the parent has not seen. When the field unmounts before any
 * blur — the person picked another step on the canvas, switched the pane,
 * the host froze the form — the draft went with it, and the last thing typed
 * was lost without a word. This hook commits what is pending on the way out.
 *
 * `pending` is the value still to commit, or `null` once there is nothing
 * (the draft equals what the parent holds, or it was just sent); `commit`
 * is the field's own commit. Both are read through refs at unmount, so the
 * latest draft is what lands, and a draft already committed — `pending`
 * back to `null` — is never sent twice.
 */

import { useEffect, useRef } from "react";

export function useCommitOnUnmount<T>(pending: T | null, commit: (value: T) => void): void {
  const pendingRef = useRef(pending);
  pendingRef.current = pending;
  const commitRef = useRef(commit);
  commitRef.current = commit;
  useEffect(
    () => () => {
      const value = pendingRef.current;
      if (value !== null) commitRef.current(value);
    },
    [],
  );
}
