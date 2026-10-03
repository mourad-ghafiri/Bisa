/**
 * The window's footer: a small always-present bar. On the left, **what a
 * harness's account has left** — one harness's usage windows, when the
 * tightest resets, a refresh, and on a click every installed harness with
 * the choice of which one stays (`UsageStat`); nothing when none is
 * installed. On the right, read-outs a person wants at a glance — the caret,
 * the live counts, the browser's tabs (`BrowserStat`: one glyph and one number,
 * its overlay in the resources' shape), then the node, and the machine's
 * CPU, GPU, memory and the platform's disk footprint as glyphs with values,
 * each opening its breakdown on a click (`ResourceStat`) — and one-tap
 * actions: every open port (open it, stop it, from wherever it was
 * started), and the pet and notes overlays on or off.
 *
 * It owns no state of its own. It reads the stores the rest of the shell reads
 * (`useHostLoad`, `useDiskUsage`, `usePorts`, `useSessions`, `useTerminals`,
 * `usePet`, `useNotesOverlay`, `useWorkspace`, the usage store through
 * `UsageStat`) and calls their existing setters — the same stop/open verbs
 * the rail uses, the same toggles the settings panels use. The facts — the
 * words, the counts, where a port lives — are `statusBarModel.mjs`'s,
 * `portsModel.mjs`'s, `footerUsageModel.mjs`'s, `resourceModel.mjs`'s and
 * `browserStatModel.mjs`'s.
 */

import { useMemo, useState, type ReactNode } from "react";
import { openExternal } from "../api";
import { errorFields, log } from "../log";
import type { ConnState } from "../bus";
import { navigate } from "../router";
import { PetStat } from "./PetStat";
import { AddonsStat } from "./AddonsStat";
import { setDockVisible, useNotesOverlay } from "../notes/notesStore";
import { DrawStat } from "./DrawStat";
import { FooterToggle } from "./FooterToggle";
import { stopPort } from "../terminal/session";
import { boardLabel } from "../views/_work/types";
import { globalPorts, groupPorts, portUrl, portsKeptWords, portsSummary, stopPlacedPrompt } from "../views/_workbench/portsModel.mjs";
import type { PlacedPort, PortGroup, PortOwner } from "../views/_workbench/portsModel.mjs";
import { ConfirmDialog, ICON, Popover, SessionMark, Tooltip, cn, harnessMark, useToast } from "../ui";
import { rescanPorts, usePorts, usePortsStale } from "./portsStore";
import { useHarnessLabels } from "./useHarnesses";
import { useSessions } from "./sessionsStore";
import { useDiskUsage, useHostLoad } from "./statsStore";
import { useTerminals } from "./useTerminals";
import { openUrlInBrowser } from "./browserDoors";
import { BrowserStat } from "./BrowserStat";
import { canOpenBrowser, openBrowserIn } from "./useBrowsers";
import { PERSON } from "./browsersModel.mjs";
import { useWorkspace } from "./useWorkspaceData";
import { caretLabel, countWords, cpuLabel, diskLabel, gpuLabel, languageLabel, memLabel, portOwnerWord } from "./statusBarModel.mjs";
import { NodeStat } from "./NodeStat";
import { metricWords, staleWords } from "./resourceModel.mjs";
import { NetworkStat } from "./NetworkStat";
import { ResourceStat } from "./ResourceStat";
import { footerSessions, placeIndex, placeWords } from "./footerSessionsModel.mjs";
import { openHarnessSession, openTerminalTab } from "./sessionDoors";
import { useEditorStatus } from "../views/_workbench/editorStatusStore";
import { UsageStat } from "./UsageStat";
import { t as tr } from "../i18n/l10n.mjs";

/** One read-out with a word: the caret's position and language, whose words are the value's meaning. */
function Stat({ label, value, tip }: { label: string; value: string; tip: string }) {
  return (
    <Tooltip label={tip}>
      <span className="flex h-6 shrink-0 items-center gap-1 px-1.5">
        <span className="text-3xs text-text-dim">{label}</span>
        <span className="tnum text-2xs text-text">{value}</span>
      </span>
    </Tooltip>
  );
}

/**
 * A count that opens a popover — terminals, harnesses, ports. Controlled,
 * so a row that opened something can close the list behind it: what moved
 * is then in front of the person, not behind the panel. It draws its glyph
 * and its number and no word — the footer's read-outs are glyphs with
 * values — and its name says both (`countWords`), as the tooltip and as
 * the popover's name, which stands in for what the trigger draws.
 */
function CountButton({ icon: Icon, count, label, children }: { icon: (typeof ICON)[keyof typeof ICON]; count: number; label: string; children: (close: () => void) => ReactNode }) {
  const [open, setOpen] = useState(false);
  return (
    <Popover
      label={label}
      side="top"
      align="end"
      open={open}
      onOpenChange={setOpen}
      className="w-72 max-w-[80vw]"
      trigger={
        <Tooltip label={label}>
          <span className="anim flex h-6 shrink-0 cursor-pointer items-center gap-1 rounded-control px-1.5 text-2xs text-text-dim hover:bg-surface-2 hover:text-text">
            <Icon size={13} aria-hidden />
            {/* A zero stays dim, so the counts that are not zero are the ones that read. */}
            <span className={cn("tnum", count > 0 ? "text-text" : "text-text-dim")}>{count}</span>
          </span>
        </Tooltip>
      }
    >
      {children(() => setOpen(false))}
    </Popover>
  );
}

export function StatusBar({ conn }: { conn: ConnState }) {
  const { info, load, stale } = useHostLoad();
  const kept = staleWords(stale);
  const { disk, dataDir } = useDiskUsage();
  const sessions = useSessions();
  const { sessions: terminals } = useTerminals();
  const scanned = usePorts();
  const portsKept = portsKeptWords(usePortsStale());
  const harnessLabels = useHarnessLabels();
  const ws = useWorkspace();
  const toast = useToast();
  // The editor you are in, the notes button's state, and the port a person is about to stop.
  const caret = useEditorStatus();
  const notes = useNotesOverlay();
  const [stopping, setStopping] = useState<PlacedPort | null>(null);

  const placed = useMemo(() => globalPorts(scanned, terminals, sessions), [scanned, terminals, sessions]);
  const portGroups = useMemo(() => groupPorts(placed), [placed]);
  // Where a session stands — its project and workstream — from the workspace index, once.
  const places = useMemo(() => placeIndex({ workstreams: ws.workstreams, projects: ws.projects, goals: ws.goals.map((g) => ({ id: g.id, label: boardLabel(g) })) }), [ws.workstreams, ws.projects, ws.goals]);
  // The rail's rows: the two rosters reconciled, a reported harness once, each with its place.
  const live = useMemo(() => footerSessions(terminals, sessions, places), [terminals, sessions, places]);

  // A port's owner → a human label, from the same index.
  const ownerLabel = (o: PortOwner): string => {
    if (o.kind === "harness") return o.harness ? harnessLabels[o.harness] ?? o.harness : tr("shell-status-bar-a-session");
    return placeWords(o.kind, o.id, places);
  };

  // A port opens in the embedded browser (ide/18): at home in the checkout
  // its owner works in, the IDE going there; one a shell of the machine
  // owns is the workspace's tab, shown in the Browser pane. Outside the
  // shell, or with the browser off, the machine's browser.
  const open = (port: number, o: PortOwner) => {
    const wid = o.kind === "workstream" ? o.id : o.workstream;
    const url = portUrl(port);
    if (!canOpenBrowser()) {
      void openExternal(url).catch((e: unknown) => log.warn("shell", "the machine's browser could not be opened", { url, ...errorFields(e) }));
      return;
    }
    if (!wid) {
      openUrlInBrowser(url);
      return;
    }
    openBrowserIn({ home: { scope: "workstream", id: wid }, url, by: PERSON });
    navigate({ name: "workbench", scope: "workstream", id: wid });
  };
  const openOutside = (port: number) =>
    void openExternal(portUrl(port)).catch((e: unknown) => log.warn("shell", "the machine's browser could not be opened", { port, ...errorFields(e) }));
  const goToOwner = (o: PortOwner) => {
    const wid = o.kind === "workstream" ? o.id : o.workstream;
    if (wid) navigate({ name: "workbench", scope: "workstream", id: wid });
  };

  return (
    <footer data-pane className="@container flex h-chrome shrink-0 items-center gap-0.5 border-t border-border bg-surface px-2 text-2xs">
      {/* What a harness's account has left — one harness, the person's pick. */}
      <UsageStat />

      <span className="flex-1" />

      {/* The editor you are in: where the caret is, what the document is */}
      {caret && (
        <>
          <Stat label={tr("shell-status-bar-pos")} value={caretLabel(caret)} tip={caret.path} />
          <Stat label={tr("shell-status-bar-lang")} value={languageLabel(caret)} tip={tr("shell-status-bar-document-s-language-editor-sees")} />
          <span className="mx-1.5 h-4 w-px shrink-0 bg-hairline" />
        </>
      )}

      {/* Live work */}
      <CountButton icon={ICON.shell} count={live.openTerminals} label={countWords("terminals", live.openTerminals)}>
        {(close) => (
          <PopList
            empty={tr("shell-status-bar-terminal-open")}
            rows={live.terminals.map((t) => {
              const Mark = t.harness ? harnessMark(t.harness) : ICON.shell;
              return {
                key: t.key,
                dot: <Mark size={12} aria-hidden className="shrink-0 text-text-dim" />,
                label: t.harness ? (harnessLabels[t.harness] ?? t.harness) : tr("shell-status-bar-shell"),
                sub: t.place,
                trailing: <span className={cn("tnum", t.liveness.status === "exited" && t.liveness.code ? "text-danger" : "text-text-dim")}>{t.word}</span>,
                dim: !t.open,
                onClick: () => {
                  openTerminalTab(t);
                  close();
                },
              };
            })}
          />
        )}
      </CountButton>
      <CountButton icon={ICON.harness} count={live.harnesses.length} label={countWords("harnesses", live.harnesses.length)}>
        {(close) => (
          <PopList
            empty={tr("shell-status-bar-harness-running")}
            rows={live.harnesses.map((s) => ({
              key: s.id,
              dot: <SessionMark state={s.state} />,
              label: s.agent ?? (s.harness ? (harnessLabels[s.harness] ?? s.harness) : tr("shell-status-bar-session")),
              sub: s.place,
              disabled: !s.workstream,
              onClick: () => {
                if (openHarnessSession(s)) close();
              },
            }))}
          />
        )}
      </CountButton>
      <CountButton icon={ICON.port} count={placed.length} label={countWords("ports", placed.length)}>
        {() => placed.length === 0 ? (
          <p className="px-2 py-3 text-center text-2xs text-text-dim">{portsKept || tr("shell-status-bar-no-open-ports")}</p>
        ) : (
          <div className="flex max-h-80 flex-col gap-1 overflow-y-auto p-1">
            <p className="px-1.5 pt-0.5 text-2xs font-semibold text-text-dim">{portsSummary(placed)}</p>
            {portsKept && <p role="status" className="px-1.5 text-2xs text-warn">{portsKept}</p>}
            {portGroups.map((g: PortGroup) => (
              <div key={`${g.kind}:${g.id}`} className="flex flex-col">
                <button type="button" onClick={() => goToOwner(g.owner)} className="anim flex items-center gap-1 rounded-control px-1.5 py-1 text-left text-2xs text-text-dim hover:text-text">
                  <span>{portOwnerWord(g.kind)}</span>
                  <span className="min-w-0 truncate">· {ownerLabel(g.owner)}</span>
                </button>
                {g.ports.map((p) => (
                  <div key={`${p.pid}:${p.port}`} className="group flex h-row-sm items-center gap-1.5 rounded-control px-1.5 hover:bg-surface-2">
                    <button type="button" onClick={() => open(p.port, p.owner)} className="anim flex min-w-0 flex-1 items-center gap-1.5 text-left">
                      <ICON.port size={11} aria-hidden className="shrink-0 text-text-dim" />
                      <span className="tnum text-2xs font-medium text-text">:{p.port}</span>
                      <span className="min-w-0 truncate font-mono text-2xs text-text-dim">{p.process}</span>
                      <span className="tnum shrink-0 text-2xs text-text-dim">{tr("shell-status-bar-pid", { pid: p.pid })}</span>
                    </button>
                    {/* A row's own verbs, beside its button and never inside it: on hover, and on focus — reaching one with the keyboard is never reaching for something invisible. */}
                    <span className="row-actions anim flex shrink-0 items-center gap-0.5">
                      <Tooltip label={tr("shell-status-bar-open-machine-s-browser")}>
                        <button type="button" aria-label={tr("shell-status-bar-open-browser", { port: p.port })} onClick={() => openOutside(p.port)} className="anim inline-flex shrink-0 rounded p-0.5 text-text-dim hover:text-text">
                          <ICON.open size={11} aria-hidden />
                        </button>
                      </Tooltip>
                      <Tooltip label={tr("shell-status-bar-stop-process")}>
                        <button type="button" aria-label={tr("shell-status-bar-stop-port", { port: p.port })} onClick={() => setStopping(p)} className="anim inline-flex shrink-0 rounded p-0.5 text-text-dim hover:text-danger">
                          <ICON.close size={11} aria-hidden />
                        </button>
                      </Tooltip>
                    </span>
                  </div>
                ))}
              </div>
            ))}
          </div>
        )}
      </CountButton>

      {/* The browser's tabs (ide/18): one glyph, one number, the overlay on a click. */}
      <BrowserStat />

      <span className="mx-1.5 h-4 w-px shrink-0 bg-hairline" />

      {/* The node and the machine: the node a glyph and a dot that open what the node is; the machine's five a glyph and a value each that open their breakdown — the network the last, how the machine reaches out. */}
      <NodeStat conn={conn} />
      <ResourceStat metric="cpu" value={cpuLabel(load?.cpu_percent)} title={tr("shell-status-bar-words", { title: metricWords("cpu", load, info, 0).title, kept })} />
      {load?.gpu && <ResourceStat metric="gpu" value={gpuLabel(load.gpu)} title={tr("shell-status-bar-words", { title: metricWords("gpu", load, info, 0).title, kept })} />}
      <ResourceStat metric="memory" value={memLabel(load?.mem_used, load?.mem_total)} title={tr("shell-status-bar-words", { title: metricWords("memory", load, info, 0).title, kept })} />
      <ResourceStat metric="disk" value={diskLabel(disk?.total)} title={dataDir ? `${metricWords("disk", load, info, 0, disk).title} · ${dataDir}` : metricWords("disk", load, info, 0, disk).title} />
      <NetworkStat />

      <span className="mx-1.5 h-4 w-px shrink-0 bg-hairline" />

      {/* Overlays: the footer turns the feature on and off — it shows or hides
          the floating note button; opening the notes panel is that button's job
          (or Alt+N), never the footer's. */}
      <FooterToggle
        icon={ICON.note}
        on={notes.dockVisible}
        label={notes.dockVisible ? tr("shell-status-bar-hide-notes-button") : tr("shell-status-bar-show-notes-button")}
        onClick={() => setDockVisible(!notes.dockVisible)}
      />
      {/* Draw (19 — Drawings): the same switch for the drawings dock, with a dot while an agent draws. */}
      <DrawStat />
      <PetStat />
      <AddonsStat />

      <ConfirmDialog
        open={stopping !== null}
        onClose={() => setStopping(null)}
        title={stopping ? tr("shell-status-bar-stop-process-2", { port: stopping.port }) : ""}
        body={stopping ? stopPlacedPrompt(stopping) : ""}
        confirmLabel={tr("shell-status-bar-stop")}
        danger
        onConfirm={() => {
          if (!stopping) return;
          const { pid, port } = stopping;
          setStopping(null);
          void stopPort(pid, port)
            .then(() => rescanPorts())
            .catch((e: unknown) => {
              log.warn("ports", "the process on the port did not stop", { port, pid, ...errorFields(e) });
              toast.error(tr("shell-status-bar-could-not-stop", { port }));
            });
        }}
      />
    </footer>
  );
}

/** A row list inside a count popover: each row a door, its place under the name, a word at the edge. */
/** A row of a footer list, or a heading over a group of them. */
type PopRow = { key: string; heading: string } | { key: string; heading?: undefined; label: string; sub?: string; dot?: ReactNode; trailing?: ReactNode; dim?: boolean; /** The thing on screen right now. */ current?: boolean; disabled?: boolean; onClick: () => void };

/**
 * A footer list: each row a button — a heading over a group, the row's own
 * control (a ✕) beside the button and never inside it, the current row
 * marked.
 */
function PopList({ rows, empty }: { rows: PopRow[]; empty: string }) {
  if (rows.length === 0) return <p className="px-2 py-3 text-center text-2xs text-text-dim">{empty}</p>;
  return (
    <div className="flex max-h-80 flex-col overflow-y-auto p-1">
      {rows.map((r) =>
        r.heading !== undefined ? (
          <div key={r.key} role="presentation" className="px-1.5 pb-0.5 pt-2 text-2xs font-semibold text-text-dim first:pt-0.5">
            {r.heading}
          </div>
        ) : (
          <div key={r.key} className={cn("flex items-center gap-1", r.current && "rounded-control bg-selected")}>
            {/* A row that cannot open says why the kit's way (`Button`'s `disabledReason`): a Tooltip on a focusable wrapper, and the words for a screen reader beside the button. */}
            <Tooltip label={tr("shell-status-bar-nowhere-open-session-runs-checkout")} active={Boolean(r.disabled)}>
              <span tabIndex={r.disabled ? 0 : -1} className="flex min-w-0 flex-1 rounded-control">
                <button
                  type="button"
                  onClick={r.onClick}
                  disabled={r.disabled}
                  aria-current={r.current ? "true" : undefined}
                  className={cn("anim flex min-w-0 flex-1 items-center gap-1.5 rounded-control px-1.5 py-1 text-left hover:bg-surface-2 disabled:cursor-default disabled:hover:bg-transparent", r.dim && "opacity-60")}
                >
                  {r.dot}
                  <span className="flex min-w-0 flex-1 flex-col">
                    <span className={cn("min-w-0 truncate text-2xs text-text", r.current && "font-medium")}>{r.label}</span>
                    {r.sub && <span className="min-w-0 truncate text-2xs text-text-dim">{r.sub}</span>}
                  </span>
                </button>
                {r.disabled && <span className="sr-only">{tr("shell-status-bar-nowhere-open-session-runs-checkout")}</span>}
              </span>
            </Tooltip>
            {r.trailing && <span className="shrink-0 pr-1.5 text-2xs">{r.trailing}</span>}
          </div>
        ),
      )}
    </div>
  );
}

