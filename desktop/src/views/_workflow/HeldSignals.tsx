/**
 * What a host's events hold for a person to read (guide/events §Durable,
 * deduplicated, and never in a loop): beside the On/Off switch of a library
 * workflow, and beside a goal's *Listening* line, a chip — *2 held for you*
 * — while a payload from outside waits on the content screen's refusal. It
 * opens the list: where each came from, which start heard it, when, the
 * node's own reason, and *Let it through*. Leaving one is doing nothing.
 *
 * Nothing is drawn while nothing is held. The list is read while the host
 * listens and again when one of its events is written down, begins a run or
 * is refused (`movesSignalsOf`). One verb at a time on a signal: a second
 * press while the first is on its way sends nothing. The rules and the
 * words are `heldSignalsModel.mjs`'s; this draws.
 */

import { useRef, useState } from "react";
import { api } from "../../api";
import { useEngineEvents } from "../../bus";
import { Button, Chip, ICON, Popover, RelativeTime, useToast } from "../../ui";
import { attempt, useAsync } from "../_work/useAsync";
import { heldOf, heldWords, hostOf, movesSignalsOf, signalsQuery, type ListeningHost } from "./heldSignalsModel.mjs";
import { inFlight, settled, taken } from "./workflowRunsModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export function HeldSignals({ host: of, listening, onChanged }: { host: ListeningHost; /** Whether the host listens: nothing is asked of the node while it does not. */ listening: boolean; onChanged?: () => void }) {
  const toast = useToast();
  const host = hostOf(of);
  const asked = listening ? host : null;
  const { data, reload } = useAsync((s) => (asked === null ? Promise.resolve([]) : api.signals(signalsQuery(asked), s)), [asked]);
  useEngineEvents((e) => {
    if (movesSignalsOf(e, asked)) reload();
  });
  // What was held while the node was away reaches the chip by no frame: the
  // read is made again when the bus comes back (`useAsync`).
  // The ref is what a press reads — two presses in one tick see each other; the state is what the buttons draw.
  const flying = useRef<ReadonlySet<string>>(new Set());
  const [drawn, setDrawn] = useState<ReadonlySet<string>>(flying.current);

  const held = heldOf(data, asked);
  const words = heldWords(held.length);
  if (words === null) return null;

  const letThrough = async (id: string) => {
    const next = taken(flying.current, id);
    if (!next) return;
    flying.current = next;
    setDrawn(next);
    await attempt(() => api.releaseSignal(id), toast.error, () => {
      toast.ok(t("workflow-held-signals-let-through-done"));
      reload();
      onChanged?.();
    });
    flying.current = settled(flying.current, id);
    setDrawn(flying.current);
  };

  return (
    <Popover
      label={t("workflow-held-signals-list")}
      align="end"
      className="w-96"
      trigger={
        <Chip tone="warn" icon={ICON.guard}>
          {words}
        </Chip>
      }
    >
      <div className="flex flex-col gap-2 p-3 text-2xs" data-held-signals={held.length}>
        <p className="leading-relaxed text-text-dim">{t("workflow-held-signals-note")}</p>
        {/* Rows divided by a hairline inside the popover, not boxes inside its box. */}
        <ul className="flex max-h-72 flex-col divide-y divide-hairline overflow-y-auto">
          {held.map((h) => (
            <li key={h.id} className="flex flex-col gap-1 py-2">
              <span className="flex flex-wrap items-center gap-x-2">
                <span className="font-medium">{t("workflow-held-signals-row", { source: h.source, step: h.step })}</span>
                <span className="text-text-dim">
                  <RelativeTime at={h.at} />
                </span>
              </span>
              <span className="text-text-dim">{h.why}</span>
              <span>
                <Button size="sm" variant="default" disabled={inFlight(drawn, h.id)} onClick={() => void letThrough(h.id)}>
                  {t("workflow-held-signals-let-through")}
                </Button>
              </span>
            </li>
          ))}
        </ul>
      </div>
    </Popover>
  );
}
