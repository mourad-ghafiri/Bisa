/**
 * A shell opened in a terminal, under its workstream: its harness's glyph
 * (or the shell mark), its name, whether it is alive and for how long, a
 * restart once it has exited, and its dot.
 */

import { ContextMenu, Dot, ICON, LiveDuration, absolute, cn, durationPrecise, harnessMark } from "../../../ui";
import type { MenuItem, TreeRowState } from "../../../ui";
import type { RailRow as RailModelRow } from "../projectRailModel.mjs";
import { rowTreatment } from "../railStyleModel.mjs";
import { livenessWord, ranWords, shellWord } from "../../../shell/terminalsModel.mjs";
import { RailAction, RailActions } from "./RailActions";
import { RailRow } from "./RailRow";
import { RailGlyph, RailTwisty } from "./RailTwisty";
import { t } from "../../../i18n/l10n.mjs";

type ShellRow = Extract<RailModelRow, { kind: "terminal" }>;

export function RailShellRow({
  row,
  rs,
  menu,
  harnessLabels,
  onOpen,
  onRestart,
}: {
  row: ShellRow;
  rs: TreeRowState;
  menu: MenuItem[];
  harnessLabels: Record<string, string>;
  onOpen: () => void;
  onRestart: () => void;
}) {
  // The child row wears its harness's own glyph and label — launched, or
  // typed into the shell — and a plain shell keeps the shell mark. The fact,
  // never the label: a harness could be called anything.
  const harness = row.harness;
  const Glyph = harness ? harnessMark(harness) : ICON.shell;
  const live = row.liveness.status === "live";
  const failed = row.liveness.status === "exited" && !!row.liveness.code;
  const treatment = rowTreatment({ kind: "terminal", attention: failed ? "danger" : "none" });
  return (
    <ContextMenu items={menu}>
      <RailRow rs={rs} treatment={treatment} className="text-2xs" onClick={onOpen}>
        <RailTwisty open={null} />
        <RailGlyph>
          <Glyph size={12} aria-hidden />
        </RailGlyph>
        <span className="min-w-0 flex-1 truncate">{harness ? (harnessLabels[harness] ?? harness) : shellWord()}</span>
        {/* A shell says only whether it is alive; what runs in it is a
            roster row of its own when it reports, and nothing when it cannot. */}
        <span className="flex shrink-0 items-center gap-1.5 text-3xs">
          {/* The liveness word is the terminals model's — the strip, the footer and this row say one thing. */}
          {live && <span>{livenessWord(row.liveness)}</span>}
          {row.liveness.status === "exited" && <span className={cn("tnum", row.liveness.code && "text-danger")}>{livenessWord(row.liveness)}</span>}
          {row.liveness.status === "unverifiable" && <span title={row.liveness.reason}>{livenessWord(row.liveness)}</span>}
          {/* How long it has been open, or how long it ran. */}
          {live && row.openedAt !== null && (
            <span className="tnum" title={absolute(row.openedAt)}>
              <LiveDuration since={row.openedAt} />
            </span>
          )}
          {row.liveness.status === "exited" && row.openedAt !== null && row.exitedAt !== null && (
            <span className="tnum" title={t("workbench-rail-shell-row-ran-until", { exitedAt: absolute(row.exitedAt) })}>
              {ranWords(durationPrecise(row.exitedAt - row.openedAt))}
            </span>
          )}
        </span>
        {!live && (
          <RailActions>
            <RailAction label={t("workbench-rail-shell-row-restart-shell")} onClick={onRestart}>
              <ICON.refresh size={12} aria-hidden />
            </RailAction>
          </RailActions>
        )}
        <Dot tone={row.liveness.status === "exited" && row.liveness.code ? "danger" : "neutral"} />
      </RailRow>
    </ContextMenu>
  );
}
