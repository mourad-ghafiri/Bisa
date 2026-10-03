/**
 * The designer session's driver: the clock and the wire around
 * `designerSession.mjs`, which decides everything else.
 *
 * The session says what to send (`saveRequest`) and when (`saveDue`: after
 * `workflow.autosave.delay_ms` of quiet, or the max wait since the first
 * unsaved edit, or the backoff after a failure); this hook keeps the timer
 * and performs the request — an `update` at the base revision; a workflow
 * is created before its designer opens, so nothing here creates. The
 * outcome goes back through the session's one door, `settle`: stored,
 * failed, rejected (a body the node could not read, never resent), or a
 * conflict when the node answered 409 (the stored copy is fetched so the
 * person can see theirs).
 *
 * **One save at a time.** The in-flight save is a promise this hook holds,
 * so a timer firing and a *Save now* in the same tick send once, and a
 * flush waits for it rather than racing it.
 *
 * **Nothing is lost on the way out.** Leaving the screen waits for the save
 * in flight, then sends the head with `keepalive` if it moved meanwhile. A
 * retired workflow (`abandon`) is not saved again, not even then.
 */

import { useCallback, useEffect, useMemo, useRef, type Dispatch, type SetStateAction } from "react";
import { ApiError } from "../../api";
import type { NewWorkflowBody, Problem, Workflow } from "../../types";
import { errorFields, log } from "../../log";
import {
  asksStored,
  refusalOutcome,
  saveDue,
  saveRequest,
  saveStarted,
  settle,
  type SaveOutcome,
  type SaveRequest,
  type Session,
} from "./designerSession.mjs";
import { failureText } from "../../ui";

interface AutosaveDriver {
  update: (
    id: string,
    body: NewWorkflowBody,
    revision: number,
    options?: { keepalive?: boolean },
  ) => Promise<{ workflow: Workflow; problems: Problem[] }>;
  /** The stored copy, when a 409 says somebody saved first. */
  fetchStored: (id: string) => Promise<Workflow>;
  delay: number;
}

export interface Autosave {
  /** Save now, ignoring the delay. Resolves `true` when the save was stored. */
  flush: () => Promise<boolean>;
  /**
   * The workflow was retired: nothing here is saved again, not even on the
   * way out — a flush on leaving would put a deleted workflow back or
   * un-freeze an archived one.
   */
  abandon: () => void;
}

/** What the node said of a save it did not store, as the session reads it (`refusalOutcome`). */
function refusalOf(e: unknown): { status: number | null; message: string; body?: unknown } {
  const message = failureText("workflow", "use-autosave-failed", e);
  return e instanceof ApiError ? { status: e.status, message, body: e.body } : { status: null, message };
}

export function useAutosave(
  session: Session | null,
  setSession: Dispatch<SetStateAction<Session | null>>,
  driver: AutosaveDriver,
): Autosave {
  const latest = useRef(session);
  latest.current = session;
  const driverRef = useRef(driver);
  driverRef.current = driver;
  /** The save in flight, so a second caller joins it rather than sending again. */
  const inflight = useRef<Promise<SaveOutcome | null> | null>(null);
  /** The last save's outcome — what the flush on leaving settles a stale session with. */
  const lastOutcome = useRef<SaveOutcome | null>(null);
  /** The workflow was retired: nothing is saved again, not even on the way out. */
  const abandoned = useRef(false);

  const perform = useCallback(
    async (req: SaveRequest): Promise<SaveOutcome> => {
      const d = driverRef.current;
      try {
        const { workflow, problems } = await d.update(req.id, req.body, req.revision);
        return { kind: "stored", workflow, problems };
      } catch (e) {
        const refusal = refusalOf(e);
        let theirs: Workflow | null = null;
        if (asksStored(refusal.status)) {
          try {
            theirs = await d.fetchStored(req.id);
          } catch (unread) {
            // The save stays a failure and is tried again, reading the stored copy again then.
            log.warn("designer", "the stored workflow could not be read after a save was refused", { ...errorFields(unread), workflow: req.id });
          }
        }
        return refusalOutcome(refusal, req.revision, theirs);
      }
    },
    [],
  );

  const run = useCallback((): Promise<SaveOutcome | null> => {
    if (inflight.current) return inflight.current;
    const s = latest.current;
    if (!s) return Promise.resolve(null);
    const req = saveRequest(s);
    if (!req) return Promise.resolve(null);
    setSession((cur) => (cur ? saveStarted(cur, req.body) : cur));
    const p = perform(req).then((outcome) => {
      inflight.current = null;
      lastOutcome.current = outcome;
      setSession((cur) => (cur ? settle(cur, outcome) : cur));
      return outcome;
    });
    inflight.current = p;
    return p;
  }, [perform, setSession]);

  // The save timer: what the session says is due, re-armed on every change
  // of the session, so a keystroke pushes the quiet out and the max wait
  // still lands.
  useEffect(() => {
    if (!session) return;
    const wait = saveDue(session, Date.now(), driver.delay);
    if (wait === null) return;
    const t = setTimeout(() => void run(), wait);
    return () => clearTimeout(t);
  }, [session, driver.delay, run]);

  // Leaving — the screen or the page — flushes an unsaved edit of a stored
  // workflow, after the save in flight has settled. The session in `latest`
  // may still say `saving` when the screen is already gone (no render
  // followed the outcome), so it is settled here with the outcome first.
  useEffect(() => {
    const flushOnLeave = () => {
      const pending = inflight.current ?? Promise.resolve(null);
      void pending.then(() => {
        if (abandoned.current) return;
        let s = latest.current;
        if (!s) return;
        if (s.status === "saving" && lastOutcome.current) s = settle(s, lastOutcome.current);
        const req = saveRequest(s);
        if (!req) return;
        void driverRef.current.update(req.id, req.body, req.revision, { keepalive: true }).catch((e: unknown) => {
          // The screen is gone, so the log is the one place left to say the
          // last keystrokes were refused.
          log.warn("designer", "the save on leaving was refused", { ...errorFields(e), workflow: req.id });
        });
      });
    };
    const beforeUnload = () => flushOnLeave();
    window.addEventListener("beforeunload", beforeUnload);
    return () => {
      window.removeEventListener("beforeunload", beforeUnload);
      flushOnLeave();
    };
  }, []);

  return useMemo(
    () => ({
      flush: () => run().then((outcome) => outcome?.kind === "stored"),
      abandon: () => {
        abandoned.current = true;
      },
    }),
    [run],
  );
}
