/**
 * What the platform needs before it can work, read and kept fresh (16 — The
 * setup gate): the five checks of `GET /readiness`. Read on mount; again on
 * a fact that moves them (`setupModel.movesReadiness`), after a fix, on
 * *Check again*, when the bus comes back after the node was away — what a
 * restarted node finds on the machine reaches the page by no frame — and on
 * the model's cadence while something is missing or the read failed
 * (`recheckMs`). One read in flight at a time: a newer one ends the older.
 *
 * The rules — what a read leaves the gate knowing, when it reads again —
 * are `setupModel.mjs`'s; this keeps the clock and the wire.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../api";
import { useEngineEvents } from "../bus";
import { errorFields, log } from "../log";
import { useReloadOnReconnect } from "../ui/useReloadOnReconnect";
import { UNREAD, afterRead, movesReadiness, recheckMs, type GateState } from "./setupModel.mjs";
import { t } from "../i18n/l10n.mjs";

export interface ReadinessRead extends GateState {
  /** A read is out. */
  checking: boolean;
  /** Read the checks again, now. */
  check: () => void;
}

export function useReadiness(): ReadinessRead {
  const [state, setState] = useState<GateState>(UNREAD);
  const [checking, setChecking] = useState(false);
  const live = useRef<AbortController | null>(null);
  const check = useCallback(() => {
    live.current?.abort();
    const ctrl = new AbortController();
    live.current = ctrl;
    setChecking(true);
    void api
      .readiness(ctrl.signal)
      .then((readiness) => {
        if (ctrl.signal.aborted) return;
        setState((was) => afterRead(was, { ok: true, readiness }));
      })
      .catch((e: unknown) => {
        if (ctrl.signal.aborted) return;
        // The gate never blocks on what it does not know; the log says why it does not, and the read is tried again.
        log.warn("setup", "the checks could not be read", errorFields(e));
        setState((was) => afterRead(was, { ok: false, error: e instanceof Error ? e.message : t("shell-setup-gate-could-read-what-platform-needs") }));
      })
      .finally(() => {
        if (!ctrl.signal.aborted) setChecking(false);
      });
  }, []);
  useEffect(() => {
    check();
    return () => live.current?.abort();
  }, [check]);
  useEngineEvents((e) => {
    if (movesReadiness(e.payload)) check();
  });
  useReloadOnReconnect(check);
  const again = recheckMs(state);
  useEffect(() => {
    if (again === null) return;
    const timer = window.setInterval(check, again);
    return () => window.clearInterval(timer);
  }, [again, check]);
  return { ...state, checking, check };
}
