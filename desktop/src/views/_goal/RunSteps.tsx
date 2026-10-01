/**
 * A run read downward: every step of its frozen workflow as a row — or,
 * before it started, the chosen definition's steps ghosted as pending — the
 * live ones open, with what each produced and the verbs it admits. Every
 * verb addresses the run by its id (`/runs/{rid}/steps/{step}/…`), so the
 * same rows serve the goal page's Progress tab and a run of the workspace's
 * own page.
 *
 * The rows a person opened or closed by hand are the screen's memory
 * (`shell/viewMemoryStore`), kept under the place the caller names — the
 * run's — so they stand through a tab switch, a way out and back, and a
 * restart. How a row stands is `progressModel.rowOpen`'s to say.
 */
import { useState } from "react";
import { useViewState } from "../../shell/viewMemoryStore";
import { wordsOf } from "../../shell/viewValuesModel.mjs";
import type { RunStrip, WorkflowRun } from "../../types";
import { parseOpened } from "../_work/openedModel.mjs";
import { progressRows, rowOpen, rowPressed } from "./progressModel.mjs";
import { StepRow } from "./StepRow";

/** Nothing said by hand: what a run's rows begin with. */
const NOTHING_SAID: ReadonlySet<string> = new Set();

export function RunSteps({
  run,
  strip,
  place,
  onChanged,
  onOpenItem,
  onDecide,
}: {
  run: WorkflowRun | null;
  /** The live steps (`current`) and, with no run, the ghosted definition (`steps`). */
  strip: Pick<RunStrip, "current" | "steps">;
  /** The place the rows opened by hand are kept under: the run's, or before a run the goal's. */
  place: string;
  onChanged: () => void;
  onOpenItem: (item: string) => void;
  /** Bring the band that signs a decision into view. */
  onDecide: () => void;
}) {
  const rows = progressRows(run, strip);
  const [said, setSaid] = useViewState<ReadonlySet<string>>(place, "steps:opened", NOTHING_SAID, parseOpened, wordsOf);
  // The steps live when the page opened stay open as they go quiet: a row
  // that closed under the reader the moment its step finished would take
  // what it had just produced out of sight.
  const [wasLive] = useState<ReadonlySet<string>>(() => new Set(strip.current));
  return (
    <ul className="flex flex-col gap-0.5">
      {rows.map((r) => {
        const live = r.current || wasLive.has(r.id);
        return (
          <StepRow
            key={r.id}
            run={run?.id ?? null}
            row={r}
            step={run?.workflow.steps.find((s) => s.id === r.id) ?? null}
            record={run?.steps[r.id] ?? null}
            expanded={rowOpen(said, r.id, live)}
            onToggle={() => setSaid((prev) => rowPressed(prev, r.id, live))}
            onChanged={onChanged}
            onOpenItem={onOpenItem}
            onDecide={onDecide}
          />
        );
      })}
    </ul>
  );
}
