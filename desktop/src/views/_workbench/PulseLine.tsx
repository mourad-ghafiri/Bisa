/**
 * The rail's pulse line: beside a **folded project's** name, what its loudest
 * workstream's harness is doing right now — *running Edit · 12s · +2 working
 * · ↳ 3 · 1 port* — the words alone, in the tone's ink, hover for the who
 * and the detail. The facts are `workstreamPulseModel.mjs`'s. An open project
 * shows its workstream rows instead, and a workstream row never wears a line:
 * folded, its glyphs and dot say who is here; open, its session rows do.
 *
 * `PortChips` is the other thing this file draws: the listening ports a
 * workstream's shells and harnesses opened, on the workstream's own row.
 */

import { ContextMenu, ICON, LiveDuration, Tooltip, cn, copyText, useToast } from "../../ui";
import { moreLabel } from "./workstreamPulseModel.mjs";
import type { Pulse } from "./workstreamPulseModel.mjs";
import { portTitle, portUrl } from "./portsModel.mjs";
import type { WorkstreamPort } from "./portsModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The tone's ink: what the words wear. */
const INK: Record<Pulse["tone"], string> = {
  accent: "text-accent-ink",
  danger: "text-danger",
  ok: "text-ok",
  working: "text-text",
  dim: "text-text-dim",
};


/**
 * The listening ports a workstream's shells and harnesses opened, one chip
 * each: hover for the number and the process, click to open it, right-click
 * to copy the URL or stop the process. The ports are the workstream's, not a
 * session's, so the row itself wears them — folded or open — and the pulse
 * line under a folded project repeats them. A port is a fact about the
 * workstream, not a summons: the chip is neutral, the accent left to what
 * waits on you.
 */
export function PortChips({
  ports,
  harnessLabels,
  openPort,
  stopPort,
}: {
  ports: readonly WorkstreamPort[];
  harnessLabels: Record<string, string>;
  openPort: (port: number) => void;
  stopPort: (p: WorkstreamPort) => void;
}) {
  const toast = useToast();
  return (
    <>
      {ports.map((p) => (
        <ContextMenu
          key={`${p.pid}:${p.port}`}
          items={[
            { label: t("workbench-pulse-line-open-browser"), onSelect: () => openPort(p.port) },
            { label: t("workbench-pulse-line-copy-url"), onSelect: () => void copyText(portUrl(p.port)).then((ok) => (ok ? toast.ok(t("workbench-pulse-line-copied-url")) : toast.error(t("workbench-center-documents-clipboard-refused")))) },
            { label: t("workbench-pulse-line-stop-process", { port: p.port }), danger: true, separatorBefore: true, onSelect: () => stopPort(p) },
          ]}
        >
          <Tooltip label={portTitle(p, harnessLabels)}>
            <button
              type="button"
              className="anim flex shrink-0 items-center gap-0.5 rounded border border-border px-1 text-3xs leading-tight text-text-dim hover:border-text/35 hover:text-text"
              onClick={(e) => {
                e.stopPropagation();
                openPort(p.port);
              }}
            >
              <ICON.port size={10} aria-hidden />
              <span className="tnum">{p.port}</span>
            </button>
          </Tooltip>
        </ContextMenu>
      ))}
    </>
  );
}

export function PulseLine({ pulse, harnessLabels }: { pulse: Pulse; harnessLabels: Record<string, string> }) {
  const who = pulse.who.startsWith("↳ ") ? pulse.who : (harnessLabels[pulse.who] ?? pulse.who);
  return (
    <span className={cn("min-w-0 truncate text-2xs", INK[pulse.tone])} title={`${who} · ${pulse.detail}`}>
      {pulse.headline}
      {pulse.liveStart != null ? (
        <LiveDuration since={pulse.liveStart} prefix=" · " className="text-text-dim" />
      ) : (
        pulse.elapsed && <span className="tnum text-text-dim"> · {pulse.elapsed}</span>
      )}
      {pulse.more && <span className="text-text-dim"> · {moreLabel(pulse.more)}</span>}
      {pulse.subagents.total > 0 && <span className="text-text-dim"> · ↳ {pulse.subagents.total}</span>}
      {pulse.ports.length > 0 && <span className="text-text-dim"> · {t("workbench-pulse-line-ports", { n: pulse.ports.length })}</span>}
    </span>
  );
}
