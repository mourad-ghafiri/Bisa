/**
 * A run's two verbs — *Stop* and *Restart* — as the Runs pane and the run's
 * page press them (03-workflows §Runs of the workspace). One verb at a time
 * on a run: a press is taken only while the run has none on its way
 * (`workflowRunsModel.taken`), so a restart pressed twice starts one run,
 * and the run's buttons wait until the node answered. A restart lands on
 * the new run's page. The rule is the model's; this performs it.
 */

import { useCallback, useRef, useState } from "react";
import { api } from "../../api";
import { navigate } from "../../router";
import { useToast } from "../../ui";
import { attempt } from "../_work/useAsync";
import { inFlight, settled, taken } from "./workflowRunsModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export interface RunVerbs {
  /** Whether a verb of this run is on its way: its buttons wait. */
  busy: (run: string) => boolean;
  stop: (run: string) => void;
  restart: (run: string) => void;
}

/** @param onMoved what re-reads the rows once a verb landed */
export function useRunVerbs(onMoved: () => void): RunVerbs {
  const toast = useToast();
  // The ref is what a press reads — two presses in one tick see each other; the state is what the buttons draw.
  const flying = useRef<ReadonlySet<string>>(new Set());
  const [drawn, setDrawn] = useState<ReadonlySet<string>>(flying.current);
  const moved = useRef(onMoved);
  moved.current = onMoved;

  const act = useCallback(
    async <T,>(run: string, call: () => Promise<T>, done: (value: T) => void) => {
      const next = taken(flying.current, run);
      if (!next) return;
      flying.current = next;
      setDrawn(next);
      await attempt(call, toast.error, done);
      flying.current = settled(flying.current, run);
      setDrawn(flying.current);
    },
    [toast],
  );

  const stop = useCallback(
    (run: string) =>
      void act(
        run,
        () => api.stopRun(run),
        () => {
          toast.ok(t("workflow-runs-pane-stopped"));
          moved.current();
        },
      ),
    [act, toast],
  );
  const restart = useCallback(
    (run: string) =>
      void act(
        run,
        () => api.restartRun(run),
        (made) => {
          toast.ok(t("workflow-runs-pane-restarted"));
          moved.current();
          navigate({ name: "run", id: made.run.id });
        },
      ),
    [act, toast],
  );
  return { busy: (run) => inFlight(drawn, run), stop, restart };
}
