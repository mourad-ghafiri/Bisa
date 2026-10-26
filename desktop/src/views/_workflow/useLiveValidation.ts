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
import { useEngineEvents } from "../../bus";
import type { NewWorkflowBody, Problem } from "../../types";
import { failureText } from "../../ui";
import { useReloadOnReconnect } from "../../ui/useReloadOnReconnect";
import { listReloads } from "../_settings/connectorHealthModel.mjs";
import { VALIDATE_DELAY_MS } from "./designerSettingsModel.mjs";

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
  // The validator's word on a connector step — an unknown connector, a lost
  // account, an input of another connector's — moves when the connectors
  // do, with no edit: the body is judged again on the frame.
  const [connectorsMoved, setConnectorsMoved] = useState(0);
  useEngineEvents((e) => {
    if (listReloads(e.payload)) setConnectorsMoved((n) => n + 1);
  });
  // The node was away: what it did meanwhile — a connector gone, an account
  // forgotten from the command line — reached no frame, so the body is
  // judged again when the bus comes back.
  useReloadOnReconnect(() => setConnectorsMoved((n) => n + 1));

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
  }, [body, delay, connectorsMoved]);

  return { problems, validating, error };
}
