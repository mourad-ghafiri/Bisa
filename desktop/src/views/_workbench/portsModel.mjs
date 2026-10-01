/**
 * Listening ports, tied to the workstream that opened them.
 *
 * The Tauri scanner answers a flat list of ports, each traced to a *root* —
 * a terminal id or a harness pid. This turns roots into workstreams: a
 * terminal root through the terminal session's `(scope, id)`, a pid root
 * through the session whose `pid` matches. A port whose root names nothing
 * the desktop still holds — a shell that closed, a session that ended between
 * the scan and now — is dropped, the same way an orphan session row is.
 *
 * Facts only, no React: which workstream a port belongs to, how it got there
 * (which shell or which session), how it reads in a tooltip, and the URL it
 * opens. The rail draws the chips; this decides what they say.
 */

import { harnessOf } from "../../shell/terminalsModel.mjs";
import { t as tr } from "../../i18n/l10n.mjs";

/** `http://localhost:<port>` — loopback by name, which every dev server answers. */
export function portUrl(port) {
  return `http://localhost:${port}`;
}

/**
 * Attribute each scanned port to a workstream, or drop it.
 * @param {readonly {port: number, pid: number, process: string, root: {kind: "terminal", terminal_id: string} | {kind: "pid", pid: number}}[]} ports
 * @param {readonly {key: string, scope: string, id: string, harness: string | null, terminalId?: string | null}[]} terminals
 * @param {readonly {id: string, pid?: number | null, harness?: string, workstream?: string | null}[]} sessions
 * @returns {WorkstreamPort[]}
 */
export function attributePorts(ports, terminals, sessions) {
  const byTerminalId = new Map();
  for (const t of terminals ?? []) {
    if (t.scope === "workstream" && typeof t.terminalId === "string") byTerminalId.set(t.terminalId, t);
  }
  const byPid = new Map();
  for (const s of sessions ?? []) {
    if (typeof s.pid === "number" && s.workstream) byPid.set(s.pid, s);
  }
  const out = [];
  for (const p of ports ?? []) {
    if (p.root?.kind === "terminal") {
      const t = byTerminalId.get(p.root.terminal_id);
      if (!t) continue;
      out.push({ port: p.port, pid: p.pid, process: p.process, workstream: t.id, via: { kind: "shell", key: t.key, harness: harnessOf(t) } });
    } else if (p.root?.kind === "pid") {
      const s = byPid.get(p.root.pid);
      if (!s) continue;
      out.push({ port: p.port, pid: p.pid, process: p.process, workstream: s.workstream, via: { kind: "harness", session: s.id, harness: s.harness ?? null } });
    }
  }
  return out;
}

/** One workstream's ports, lowest port first; a port shown twice (two roots) is shown once. */
export function portsOf(attributed, workstream) {
  const seen = new Set();
  return (attributed ?? [])
    .filter((p) => p.workstream === workstream)
    .filter((p) => {
      const k = `${p.pid}:${p.port}`;
      if (seen.has(k)) return false;
      seen.add(k);
      return true;
    })
    .sort((a, b) => a.port - b.port);
}

/**
 * A port's tooltip: the port, the process and its pid, and who opened it —
 * the harness label when a harness did, "shell" when a plain shell did.
 * @param {WorkstreamPort} p
 * @param {Record<string, string>} [harnessLabels]
 */
export function portTitle(p, harnessLabels = {}) {
  const who = p.via.harness ? harnessLabels[p.via.harness] ?? p.via.harness : tr("workbench-ports-shell");
  return tr("workbench-ports-port-title", { port: p.port, process: p.process, pid: p.pid, who });
}

/** The line a confirm dialog shows before stopping one. */
export function stopPrompt(p) {
  const started = p.via.kind === "shell" ? tr("workbench-ports-shell-started-stays-open") : tr("workbench-ports-harness-started-keeps-running");
  return tr("workbench-ports-stop-pid-listening", { process: p.process, pid: p.pid, port: p.port, started });
}

/**
 * @typedef {object} WorkstreamPort
 * @property {number} port
 * @property {number} pid
 * @property {string} process
 * @property {string} workstream
 * @property {{kind: "shell", key: string, harness: string | null} | {kind: "harness", session: string, harness: string | null}} via
 */

/**
 * Every open port, tied to where it lives — for the footer's global view,
 * not just the rail's per-workstream one. A terminal-rooted port
 * carries its terminal's `(scope, id)` (any scope, not only workstream); a
 * pid-rooted port carries the whole place off its `SessionRow` — the goal,
 * project, workstream and work item it belongs to — so a person can open or
 * stop a port from wherever it was started.
 * @param {readonly {port: number, pid: number, process: string, root: {kind: "terminal", terminal_id: string} | {kind: "pid", pid: number}}[]} ports
 * @param {readonly {key: string, scope: string, id: string, harness: string | null, terminalId?: string | null}[]} terminals
 * @param {readonly {id: string, pid?: number | null, harness?: string, goal?: string | null, project?: string | null, workstream?: string | null, work_item?: string | null}[]} sessions
 * @returns {PlacedPort[]}
 */
export function globalPorts(ports, terminals, sessions) {
  const byTerminalId = new Map();
  for (const t of terminals ?? []) {
    if (typeof t.terminalId === "string") byTerminalId.set(t.terminalId, t);
  }
  const byPid = new Map();
  for (const s of sessions ?? []) {
    if (typeof s.pid === "number") byPid.set(s.pid, s);
  }
  const seen = new Set();
  const out = [];
  for (const p of ports ?? []) {
    let owner = null;
    if (p.root?.kind === "terminal") {
      const t = byTerminalId.get(p.root.terminal_id);
      if (t) owner = { kind: t.scope === "workstream" ? "workstream" : t.scope, id: t.id, harness: t.harness ?? null, via: "shell" };
    } else if (p.root?.kind === "pid") {
      const s = byPid.get(p.root.pid);
      if (s) {
        // The most specific place the session names — a workstream sits in a
        // project sits under a goal — with the rest carried for the label.
        const kind = s.workstream ? "workstream" : s.work_item ? "work_item" : s.project ? "project" : s.goal ? "goal" : "harness";
        const id = s.workstream ?? s.work_item ?? s.project ?? s.goal ?? s.id;
        owner = { kind, id, harness: s.harness ?? null, via: "harness", goal: s.goal ?? null, project: s.project ?? null, workstream: s.workstream ?? null };
      }
    }
    if (!owner) continue;
    const k = `${p.pid}:${p.port}`;
    if (seen.has(k)) continue;
    seen.add(k);
    out.push({ port: p.port, pid: p.pid, process: p.process, owner });
  }
  return out.sort((a, b) => a.port - b.port);
}

/** The kinds a footer groups ports under, in the order it shows them. */
const OWNER_KIND_ORDER = ["goal", "project", "workstream", "work_item", "shell", "harness"];

/** Placed ports bucketed by their owner (`kind:id`), each group in port order — for the footer's list. */
export function groupPorts(placed) {
  const groups = new Map();
  for (const p of placed ?? []) {
    const key = `${p.owner.kind}:${p.owner.id}`;
    const g = groups.get(key) ?? { kind: p.owner.kind, id: p.owner.id, owner: p.owner, ports: [] };
    g.ports.push(p);
    groups.set(key, g);
  }
  return [...groups.values()].sort(
    (a, b) => OWNER_KIND_ORDER.indexOf(a.kind) - OWNER_KIND_ORDER.indexOf(b.kind) || String(a.id).localeCompare(String(b.id)),
  );
}

/**
 * The line above the footer's port groups: how many ports, and how many
 * were started in a terminal and in a harness — *3 ports · 2 in terminals ·
 * 1 in a harness*; nothing when there are none.
 * @param {readonly PlacedPort[]} placed
 */
/**
 * What the ports popover says while the scans fail: the ports shown are the
 * last ones read, and may no longer be true. Nothing while the scans answer.
 * @param {string | null | undefined} stale the failed scan's reason
 */
export function portsKeptWords(stale) {
  const why = typeof stale === "string" ? stale.trim() : "";
  return why ? tr("workbench-ports-ports-could-not-scanned-last-answer", { why }) : "";
}

export function portsSummary(placed) {
  const all = placed ?? [];
  if (all.length === 0) return "";
  const shells = all.filter((p) => p.owner.via === "shell").length;
  const harnesses = all.length - shells;
  const parts = [`${all.length} ${all.length === 1 ? "port" : "ports"}`];
  if (shells > 0) parts.push(tr("workbench-ports-terminal-terminals", { shells }));
  if (harnesses > 0) parts.push(tr("workbench-ports-harness-harnesses", { harnesses }));
  return parts.join(" · ");
}

/** The confirm line before stopping a placed port. */
export function stopPlacedPrompt(p) {
  const started = p.owner.via === "shell" ? tr("workbench-ports-shell-started-stays-open") : tr("workbench-ports-harness-started-keeps-running");
  return tr("workbench-ports-stop-pid-listening", { process: p.process, pid: p.pid, port: p.port, started });
}

/**
 * @typedef {object} PortOwner
 * @property {"goal"|"project"|"workstream"|"work_item"|"shell"|"harness"} kind
 * @property {string} id
 * @property {string | null} harness
 * @property {"shell"|"harness"} via
 * @property {string | null} [goal]
 * @property {string | null} [project]
 * @property {string | null} [workstream]
 */

/**
 * @typedef {object} PlacedPort
 * @property {number} port
 * @property {number} pid
 * @property {string} process
 * @property {PortOwner} owner
 */
