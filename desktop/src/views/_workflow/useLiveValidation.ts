/**
 * Validate as you type.
 *
 * Validation is cheap and pure (`POST /workflows/validate` writes nothing),
 * so it runs a moment after the last edit and the problems list follows the
 * cursor. Shared by the library's designer and the goal tab's amendment, so
 * an amendment is judged the way a design is — before the save, not by it.
 *
 * `problems` is `null` until the first answer for the current body arrives,
 * so a caller can show the node's last word rather than an empty list. A
 * validation that could not run — the node unreachable, or a body it could
 * not read — is `error`, beside the last problems, so a list is never
 * silently stale: the caller says the node has not judged this body.
 */

import { useEffect, useRef, useState } from "react";
import type { NewWorkflowBody, Problem } from "../../types";
import { failureText } from "../../ui";

const VALIDATE_DELAY_MS = 200;

interface LiveValidation {
  problems: Problem[] | null;
  validating: boolean;
  /** Why the current body could not be validated; `null` when it was, or is being. */
  error: string | null;
}

export function useLiveValidation(
  body: NewWorkflowBody | null,
  validate: (body: NewWorkflowBody, signal: AbortSignal) => Promise<Problem[]>,
  delay = VALIDATE_DELAY_MS,
): LiveValidation {
  const [problems, setProblems] = useState<Problem[] | null>(null);
  const [validating, setValidating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const validateRef = useRef(validate);
  validateRef.current = validate;

  useEffect(() => {
    if (body === null) {
      setProblems(null);
      setValidating(false);
      setError(null);
      return;
    }
    const ac = new AbortController();
    setValidating(true);
    const t = setTimeout(() => {
      void validateRef
        .current(body, ac.signal)
        .then((p) => {
          if (ac.signal.aborted) return;
          setProblems(p);
          setError(null);
        })
        .catch((e: unknown) => {
          // A validation that could not run is not a problem list: the last
          // one stays, and the reason stands beside it.
          if (!ac.signal.aborted) setError(failureText("workflow", "use-live-validation-failed", e));
        })
        .finally(() => {
          if (!ac.signal.aborted) setValidating(false);
        });
    }, delay);
    return () => {
      clearTimeout(t);
      ac.abort();
    };
  }, [body, delay]);

  return { problems, validating, error };
}
