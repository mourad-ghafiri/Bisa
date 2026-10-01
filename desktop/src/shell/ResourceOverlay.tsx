/**
 * The overlay under one of the footer's resource read-outs: the metric's
 * sentence, the host's use as a bar read left to right — Bisa's parts, the
 * rest of the machine, what is free — with a legend, a control of
 * the metric's dimensions — platform · goals · projects · workflows ·
 * harnesses · terminals for CPU and memory; workspace · goals · projects ·
 * harnesses · terminals · activity for disk; none for the GPU, which no
 * reader breaks down by process — each with its glyph and its whole word,
 * the panel sized for the row (`ResourceStat`) and the control never cutting
 * a word — and the rows of the chosen one, each a label, where it stands,
 * its number and a bar at its share of the top row.
 * A row that names a goal, a project, a workflow or a tab is a door. Every
 * fact is `resourceModel.mjs`'s; this file paints.
 */

import { useMemo } from "react";
import { navigate } from "../router";
import type { TerminalScope } from "../terminal/session";
import { boardLabel } from "../views/_work/types";
import { ICON, Meter, SegmentedControl, StackedBar, cn } from "../ui";
import type { Segment as ControlSegment } from "../ui";
import { placeIndex } from "./footerSessionsModel.mjs";
import type { Dimension, Door, Metric, Row } from "./resourceModel.mjs";
import { DIMENSIONS, attributeShares, chosenDimension, dimensionIcon, dimensionLabel, foldRows, footnote, metricWords, moreWords, oursOf, overlayRows, percentWords, platformRows, rowPercent, usageBar, valueWords } from "./resourceModel.mjs";
import { chooseDimension, useChosenDimension } from "./resourceDimensionStore";
import { openTerminalTab } from "./sessionDoors";
import { useSessions } from "./sessionsStore";
import { statsPoll, useDiskUsage, useHostLoad, useProcessShares } from "./statsStore";
import { useHarnessLabels } from "./useHarnesses";
import { useTerminals } from "./useTerminals";
import { useWorkspace } from "./useWorkspaceData";
import { t } from "../i18n/l10n.mjs";

function walk(door: Door, close: () => void) {
  if (!door) return;
  switch (door.kind) {
    case "goal":
      navigate({ name: "goal", id: door.id });
      break;
    case "project":
      // A project opens as its primary workstream, whose id is the project's.
      navigate({ name: "workbench", scope: "workstream", id: door.id });
      break;
    case "workflow":
      navigate({ name: "workflow", id: door.id });
      break;
    case "run":
      navigate({ name: "run", id: door.id });
      break;
    case "terminal":
      openTerminalTab({ key: door.key, scope: door.scope as TerminalScope, id: door.id });
      break;
  }
  close();
}

export function ResourceOverlay({ metric, close }: { metric: Metric; close: () => void }) {
  const { info, load } = useHostLoad();
  const { processes, intervalSecs, readMs } = useProcessShares();
  const { disk } = useDiskUsage();
  const sessions = useSessions();
  const { sessions: terminals } = useTerminals();
  const harnessLabels = useHarnessLabels();
  const ws = useWorkspace();
  const remembered = useChosenDimension(metric);
  const dimension = chosenDimension(metric, remembered);

  const places = useMemo(
    () => placeIndex({ workstreams: ws.workstreams, projects: ws.projects, goals: ws.goals.map((g) => ({ id: g.id, label: boardLabel(g), workflow: g.workflow ?? null, workflowName: g.strip?.workflow_name ?? null })) }),
    [ws.workstreams, ws.projects, ws.goals],
  );
  const shares = useMemo(() => attributeShares(processes, sessions, terminals, places), [processes, sessions, terminals, places]);
  const cores = info?.cores ?? 1;
  const ours = metric === "cpu" || metric === "memory" ? oursOf(shares, metric, cores) : 0;
  const words = metricWords(metric, load, info, ours, disk);
  const bar = metric === "cpu" || metric === "memory" ? usageBar(platformRows(shares, load, cores, metric), metric) : null;
  const rows: Row[] = dimension && metric !== "gpu" ? overlayRows(metric, dimension, { shares, host: load, info, disk, places, harnessLabels }) : [];
  const folded = foldRows(rows, 8);
  const options: ControlSegment<Dimension>[] = DIMENSIONS[metric].map((d) => ({ id: d, label: dimensionLabel(d), icon: ICON[dimensionIcon(d)] }));

  return (
    <div className="flex min-w-0 flex-col gap-2" role="group" aria-label={words.title}>
      <p className="px-1 text-xs font-medium text-text">{words.title}</p>
      {bar && (
        <StackedBar
          segments={bar.segments}
          legend={bar.legend}
          label={bar.label}
          words={(b) => (metric === "cpu" ? percentWords(b.percent) : `${valueWords(metric, b.value ?? 0)} · ${percentWords(b.percent)}`)}
        />
      )}
      {options.length > 0 && dimension && (
        <SegmentedControl<Dimension> size="sm" stretch label={t("shell-browser-overlay-words", { title: words.title })} options={options} value={dimension} onChange={(d) => chooseDimension(metric, d)} />
      )}
      {metric === "gpu" ? (
        <p className="px-1 text-2xs text-text-dim">{load?.gpu ? t("shell-resource-overlay-accelerator-s-load-whole-machine") : t("shell-resource-overlay-no-gpu-reader-machine")}</p>
      ) : rows.length === 0 ? (
        <p className="px-2 py-3 text-center text-2xs text-text-dim">{t("shell-resource-overlay-nothing-show-yet")}</p>
      ) : (
        <div className="flex max-h-80 flex-col overflow-y-auto">
          {folded.rows.map((r) => {
            const body = (
              <>
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="min-w-0 truncate text-2xs text-text">{r.label}</span>
                  {r.sub && <span className="min-w-0 truncate text-3xs text-text-dim">{r.sub}</span>}
                </span>
                <Meter percent={rowPercent(r, folded.rows)} tone="quiet" width="w-12" />
                <span className="tnum w-16 shrink-0 text-right text-2xs text-text">{valueWords(metric, r.value)}</span>
              </>
            );
            const className = "flex w-full items-center gap-2 rounded-control px-1.5 py-1 text-left";
            return r.door ? (
              <button key={r.key} type="button" onClick={() => walk(r.door, close)} className={cn("anim hover:bg-surface-2", className)}>
                {body}
              </button>
            ) : (
              <div key={r.key} className={className}>
                {body}
              </div>
            );
          })}
          {folded.more && <p className="px-1.5 pt-1 text-3xs text-text-dim">{moreWords(metric, folded.more)}</p>}
        </div>
      )}
      <p className="px-1 text-3xs text-text-dim">{footnote(metric, statsPoll(), metric === "disk" ? disk?.read_ms : readMs, intervalSecs)}</p>
    </div>
  );
}
