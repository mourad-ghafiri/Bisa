/**
 * The footer's resources, as facts — no React. What the machine does
 * (**load**) and what each thing the platform can name takes of it (its
 * **share**): the desktop app, the node, every harness session and every
 * terminal, each with its whole process tree, attributed by the shell
 * (`src-tauri/src/attribution.rs`, the nearest root) and joined here to
 * what the rosters know — a session's goal, project, workflow, workstream and
 * harness; a tab's place and what runs in it. Disk is the data directory by
 * area, read by the same rules. The overlay paints rows; every number,
 * word, order and door is decided here.
 *
 * Words: what a harness's *account* has left is **usage** and lives on the
 * footer's left; nothing here is usage.
 */

import { formatSize } from "../ui/fileTreeModel.mjs";
import { claimedSessions } from "../views/_workbench/workstreamSessionsModel.mjs";
import { emptyPlaceIndex, placeWords } from "./footerSessionsModel.mjs";
import { harnessOf, shellWord } from "./terminalsModel.mjs";
import { t as tr } from "../i18n/l10n.mjs";
import { percent } from "../i18n/format.mjs";

/** The dimensions each overlay offers, in the control's order. */
export const DIMENSIONS = Object.freeze({
  cpu: Object.freeze(["platform", "goals", "projects", "workflows", "harnesses", "terminals"]),
  memory: Object.freeze(["platform", "goals", "projects", "workflows", "harnesses", "terminals"]),
  gpu: Object.freeze([]),
  disk: Object.freeze(["areas", "goals", "projects", "harnesses", "terminals", "activity"]),
});

/** A dimension's word on the control. */
export function dimensionLabel(dimension) {
  return { platform: tr("shell-resource-platform"), areas: tr("shell-sidebar-workspace"), goals: tr("shell-omnibox-goals"), projects: tr("shell-omnibox-projects"), workflows: tr("shell-nav-workflows"), harnesses: tr("shell-resource-harnesses"), terminals: tr("shell-resource-terminals"), activity: tr("shell-resource-activity") }[dimension] ?? dimension;
}

/** A dimension's glyph on the control, as the key of `ui/icons.ts` it is drawn from — the stack for the platform, the concept's own mark for the rest. */
export function dimensionIcon(dimension) {
  return { platform: "platform", areas: "folder", goals: "goal", projects: "project", workflows: "workflow", harnesses: "harness", terminals: "shell", activity: "pulse" }[dimension] ?? "pulse";
}

/** The dimension to show: the remembered one while the metric still offers it, else the first. */
export function chosenDimension(metric, remembered) {
  const offered = DIMENSIONS[metric] ?? [];
  if (remembered && offered.includes(remembered)) return remembered;
  return offered[0] ?? null;
}

const tail = (id) => String(id ?? "").slice(-6);

/** A process's share, as a number of the metric: CPU as a percent of the host (per-core figure over the cores), memory in bytes. */
function shareOf(share, metric, cores) {
  if (metric === "cpu") return share.cpu / Math.max(1, cores);
  return share.mem;
}

/**
 * Every process the shell attributed, joined to what the rosters know.
 *
 * A `session` root is the harness session with that pid — its goal, project,
 * workflow, workstream and harness. A `terminal` root is the tab with that
 * PTY id: when the tab claims a reported session (`claimedSessions`) the
 * process folds into that session — the roster draws the two as one row,
 * the harness's — else it is the tab's, with the harness the process table
 * showed in it. A session root the roster no longer names is *a session
 * that just ended*, kept, not dropped.
 * @param {readonly object[]} processes the shell's `ProcessShare[]`
 * @param {readonly object[]} sessions the roster (`SessionRow[]`)
 * @param {readonly object[]} terminals the terminal store's sessions
 * @param {import("./footerSessionsModel.mjs").PlaceIndex} [places]
 */
export function attributeShares(processes, sessions, terminals, places = emptyPlaceIndex()) {
  const byPid = new Map();
  const byId = new Map();
  for (const s of sessions ?? []) {
    byId.set(s.id, s);
    if (typeof s.pid === "number") byPid.set(s.pid, s);
  }
  const byTerminalId = new Map();
  for (const t of terminals ?? []) if (typeof t.terminalId === "string") byTerminalId.set(t.terminalId, t);
  const claimed = claimedSessions(sessions ?? [], terminals ?? []);

  const ofSession = (s, p) => ({
    kind: "session",
    sessionId: s.id,
    terminalKey: claimed.get(s.id) ?? null,
    harness: s.harness ?? null,
    agent: s.agent ?? null,
    goal: s.goal ?? null,
    run: s.run ?? null,
    project: s.project ?? places.workstreams.get(s.workstream ?? "")?.projectId ?? null,
    workstream: s.workstream ?? null,
    ...measures(p),
  });
  const ofTab = (t, p) => ({
    kind: "terminal",
    sessionId: null,
    terminalKey: t.key,
    terminalScope: t.scope,
    terminalId: t.id,
    harness: harnessOf(t),
    agent: null,
    goal: t.scope === "goal" ? t.id : null,
    project: t.scope === "project" ? t.id : t.scope === "workstream" ? places.workstreams.get(t.id)?.projectId ?? null : null,
    workstream: t.scope === "workstream" ? t.id : null,
    ...measures(p),
  });

  const out = [];
  for (const p of processes ?? []) {
    const root = p.root ?? {};
    if (root.kind === "desktop") out.push({ kind: "desktop", sessionId: null, terminalKey: null, harness: null, agent: null, goal: null, project: null, workstream: null, ...measures(p) });
    else if (root.kind === "node") out.push({ kind: "node", sessionId: null, terminalKey: null, harness: null, agent: null, goal: null, project: null, workstream: null, ...measures(p) });
    else if (root.kind === "session") {
      const s = byPid.get(root.pid);
      if (s) out.push(ofSession(s, p));
      else out.push({ kind: "session", sessionId: null, terminalKey: null, harness: null, agent: null, goal: null, project: null, workstream: null, ended: true, ...measures(p) });
    } else if (root.kind === "terminal") {
      const t = byTerminalId.get(root.terminal_id);
      if (!t) continue;
      const s = t.sessionId ? byId.get(t.sessionId) : null;
      if (s && claimed.has(s.id)) out.push(ofSession(s, p));
      else out.push(ofTab(t, p));
    }
  }
  return out;
}

function measures(p) {
  return { pid: p.pid, name: p.name, cpu: num(p.cpu_percent), mem: num(p.mem_bytes), read: num(p.read_bytes), written: num(p.written_bytes) };
}

function num(v) {
  return typeof v === "number" && Number.isFinite(v) ? v : 0;
}

/** Everything Bisa takes of a metric: the sum over every attributed share. */
export function oursOf(shares, metric, cores) {
  let sum = 0;
  for (const s of shares ?? []) sum += shareOf(s, metric, cores);
  return sum;
}

/**
 * The platform's rows: the machine (everything the platform is not), the
 * desktop app, the node, every harness session and every terminal — each a
 * number of the metric and its share of the host's total.
 * @param {readonly object[]} shares from {@link attributeShares}
 * @param {{cpu_percent?: number, mem_used?: number, mem_total?: number} | null | undefined} host
 * @param {number} cores logical cores
 * @param {"cpu" | "memory"} metric
 */
export function platformRows(shares, host, cores, metric) {
  const total = metric === "cpu" ? 100 : num(host?.mem_total);
  const hostUsed = metric === "cpu" ? num(host?.cpu_percent) : num(host?.mem_used);
  const sum = (kind) => (shares ?? []).filter((s) => s.kind === kind).reduce((n, s) => n + shareOf(s, metric, cores), 0);
  const count = (kind) => new Set((shares ?? []).filter((s) => s.kind === kind).map((s) => s.sessionId ?? s.terminalKey ?? s.pid)).size;
  const ours = oursOf(shares, metric, cores);
  const rows = [
    { key: "machine", label: tr("shell-resource-machine"), sub: tr("shell-resource-everything-bisa"), value: Math.max(0, hostUsed - ours), count: null, door: null },
    { key: "desktop", label: tr("shell-resource-desktop-app"), sub: tr("shell-resource-window-s-process-renderer-runs-under"), value: sum("desktop"), count: null, door: null },
    { key: "node", label: tr("shell-node-overlay-node"), sub: tr("shell-resource-engine-intake"), value: sum("node"), count: null, door: null },
    { key: "harnesses", label: tr("shell-resource-harnesses"), sub: sessionsWord(count("session")), value: sum("session"), count: count("session"), door: null },
    { key: "terminals", label: tr("shell-terminal-panel-terminals"), sub: tabsWord(count("terminal")), value: sum("terminal"), count: count("terminal"), door: null },
  ];
  return rows.map((r) => ({ ...r, share: total > 0 ? r.value / total : 0 }));
}

function sessionsWord(n) {
  return tr("shell-resource-sessions", { n });
}
function tabsWord(n) {
  return tr("shell-resource-shells", { n });
}

/** A percent for a legend or a tooltip: whole, and *<1%* for a share that rounds to nothing. */
export function percentWords(n) {
  const v = num(n);
  if (v > 0 && v < 1) return tr("shell-resource-under-one-percent");
  return pct(v);
}

const METRIC_NAME = Object.freeze({ cpu: "CPU", memory: tr("shell-resource-memory") });
/** The bar's short word for each of Bisa's parts, inside the label's parentheses. */
const PART_WORD = Object.freeze({ desktop: tr("shell-resource-part-desktop"), node: tr("shell-resource-part-node"), harnesses: tr("shell-resource-part-harnesses"), terminals: tr("shell-resource-part-terminals") });

/**
 * The header bar — read left to right as a person thinks: what Bisa takes
 * (its parts: the desktop app, the node, the harnesses, the terminals), then
 * what the rest of the machine takes, then what is free (the track). Every
 * band has a tone of its own — the machine's is never the track's — a legend
 * says the three in the metric's words and percents, and the label is the
 * bar's whole sentence for a reader. No host figure: no bar, no legend.
 * @param {ReturnType<typeof platformRows>} rows
 * @param {"cpu" | "memory"} metric
 */
export function usageBar(rows, metric) {
  const all = rows ?? [];
  const machine = all.find((r) => r.key === "machine");
  if (!machine) return { segments: [], legend: [], label: "" };
  const tones = { desktop: "accent", node: "ok", harnesses: "warn", terminals: "quiet" };
  const parts = all.filter((r) => r.key !== "machine" && r.share > 0);
  const ours = parts.reduce((n, r) => n + r.value, 0);
  const oursPercent = parts.reduce((n, r) => n + r.share * 100, 0);
  const machinePercent = machine.share * 100;
  // The sum never passes the track: a host figure that lags its parts is clamped at the machine's band.
  const over = Math.max(0, oursPercent + machinePercent - 100);
  const segments = parts.map((r) => ({ key: r.key, label: r.label, percent: Math.min(100, r.share * 100), tone: tones[r.key] ?? "quiet", value: r.value }));
  const machineShown = Math.max(0, machinePercent - over);
  if (machineShown > 0) segments.push({ key: "machine", label: tr("shell-resource-machine"), percent: machineShown, tone: "neutral", value: machine.value });
  const used = Math.min(100, oursPercent + machineShown);
  const free = Math.max(0, 100 - used);
  // The host's total, back from any row's value and share — bytes for memory, a hundred for CPU.
  const known = all.find((r) => r.share > 0);
  const total = metric === "cpu" ? 100 : known ? known.value / known.share : 0;
  const freeValue = metric === "cpu" ? free : Math.max(0, total - machine.value - ours);
  const words = (value, percent) => (metric === "cpu" ? percentWords(percent) : `${valueWords(metric, value)} · ${percentWords(percent)}`);
  const legend = [
    { key: "bisa", label: "Bisa", tone: "accent", words: words(ours, oursPercent) },
    { key: "machine", label: tr("shell-resource-machine"), tone: "neutral", words: words(machine.value, machineShown) },
    { key: "free", label: tr("shell-resource-free"), tone: "track", words: words(freeValue, free) },
  ];
  const partWords = parts.map((r) => tr("shell-resource-bar-part", { label: PART_WORD[r.key] ?? r.label.toLowerCase(), percent: percentWords(r.share * 100) })).join(", ");
  const args = { metric: METRIC_NAME[metric] ?? metric, ours: percentWords(oursPercent), parts: partWords, machine: percentWords(machineShown), free: percentWords(free) };
  const label = partWords ? tr("shell-resource-bar-label-parts", args) : tr("shell-resource-bar-label", args);
  return { segments, legend, label };
}

/**
 * The rows of one dimension: shares grouped by what they name — a goal, a
 * project (by id, named), the goal's workflow or the run of the workspace, a
 * harness kind, a terminal tab — summed and sorted, what names none under
 * *elsewhere*.
 * @param {"goals" | "projects" | "workflows" | "harnesses" | "terminals"} dimension
 * @param {readonly object[]} shares
 * @param {import("./footerSessionsModel.mjs").PlaceIndex} places
 * @param {"cpu" | "memory"} metric
 * @param {number} cores
 * @param {Record<string, string>} [harnessLabels]
 */
export function dimensionRows(dimension, shares, places, metric, cores, harnessLabels = {}) {
  const groups = new Map();
  const add = (key, label, sub, door, s) => {
    const g = groups.get(key) ?? { key, label, sub, door, value: 0, count: 0, members: new Set() };
    g.value += shareOf(s, metric, cores);
    g.members.add(s.sessionId ?? s.terminalKey ?? s.pid);
    groups.set(key, g);
  };
  for (const s of shares ?? []) {
    if (s.kind === "desktop" || s.kind === "node") continue;
    switch (dimension) {
      case "goals": {
        if (s.goal) add(`goal:${s.goal}`, placeWords("goal", s.goal, places), s.harness ? harnessLabels[s.harness] ?? s.harness : null, { kind: "goal", id: s.goal }, s);
        else add("elsewhere", tr("shell-resource-elsewhere"), tr("shell-resource-goal"), null, s);
        break;
      }
      case "projects": {
        if (s.project) add(`project:${s.project}`, placeWords("project", s.project, places), s.workstream ? placeWords("workstream", s.workstream, places) : null, { kind: "project", id: s.project }, s);
        else add("elsewhere", tr("shell-resource-elsewhere"), tr("shell-resource-project"), null, s);
        break;
      }
      case "workflows": {
        const w = s.goal ? places.goals.get(s.goal) : null;
        if (w?.workflow) add(`workflow:${w.workflow}`, w.workflowName ?? tr("shell-resource-workflow-tail", { tail: tail(w.workflow) }), placeWords("goal", s.goal, places), { kind: "workflow", id: w.workflow }, s);
        // A run of the workspace: no goal names its workflow, the run's page does.
        else if (s.run) add(`run:${s.run}`, placeWords("run", s.run, places), s.harness ? harnessLabels[s.harness] ?? s.harness : null, { kind: "run", id: s.run }, s);
        else add("elsewhere", tr("shell-resource-elsewhere"), tr("shell-resource-workflow"), null, s);
        break;
      }
      case "harnesses": {
        if (s.harness) add(`harness:${s.harness}`, harnessLabels[s.harness] ?? s.harness, null, null, s);
        else add("elsewhere", tr("shell-resource-elsewhere"), tr("shell-resource-harness"), null, s);
        break;
      }
      case "terminals": {
        if (s.kind === "terminal") {
          const where = placeWords(s.terminalScope, s.terminalId, places);
          add(`terminal:${s.terminalKey}`, s.harness ? harnessLabels[s.harness] ?? s.harness : shellWord(), where, { kind: "terminal", key: s.terminalKey, scope: s.terminalScope, id: s.terminalId }, s);
        } else if (s.kind === "session" && s.terminalKey) {
          add(`terminal:${s.terminalKey}`, s.harness ? harnessLabels[s.harness] ?? s.harness : tr("shell-resource-session"), s.workstream ? placeWords("workstream", s.workstream, places) : tr("shell-footer-sessions-checkout"), { kind: "terminal", key: s.terminalKey, scope: "workstream", id: s.workstream ?? "" }, s);
        } else add("elsewhere", tr("shell-resource-elsewhere"), tr("shell-resource-terminal"), null, s);
        break;
      }
      default:
        break;
    }
  }
  const rows = [...groups.values()].map((g) => ({ key: g.key, label: g.label, sub: g.sub, door: g.door, value: g.value, count: g.members.size }));
  return sortRows(rows);
}

/** Largest first; *elsewhere* last among equals, by label among equals. */
function sortRows(rows) {
  return rows.sort((a, b) => b.value - a.value || (a.key === "elsewhere") - (b.key === "elsewhere") || String(a.label).localeCompare(String(b.label)));
}

/**
 * The first `n` rows and the rest folded into one line.
 * @template {{value: number}} R
 * @param {readonly R[]} rows
 * @param {number} [n]
 * @returns {{rows: R[], more: {count: number, value: number} | null}}
 */
export function foldRows(rows, n = 8) {
  const all = rows ?? [];
  if (all.length <= n) return { rows: [...all], more: null };
  const rest = all.slice(n);
  return { rows: all.slice(0, n), more: { count: rest.length, value: rest.reduce((s, r) => s + r.value, 0) } };
}

/** A row's value in the metric's words: a percent, a size, or activity. */
export function valueWords(metric, value) {
  if (metric === "cpu" || metric === "gpu") return `${Math.round(num(value) * 10) / 10}%`;
  return formatSize(num(value)) || "0 B";
}

/** The fold line: *+3 more · 2.1%*. */
export function moreWords(metric, more) {
  if (!more) return "";
  return tr("shell-resource-more", { count: more.count, value: valueWords(metric, more.value) });
}

/** A row's bar, as its share of the top row (the glance; the text carries the number). */
export function rowPercent(row, rows) {
  const top = Math.max(0, ...(rows ?? []).map((r) => num(r.value)));
  return top > 0 ? Math.min(100, (num(row.value) / top) * 100) : 0;
}

// ---------------------------------------------------------------------------
// The header: the metric's value in the bar and its sentence in the overlay
// ---------------------------------------------------------------------------

const pct = (n) => percent(num(n) / 100);
const one = (n) => (Math.round(num(n) * 10) / 10).toFixed(1);

/**
 * @param {"cpu" | "gpu" | "memory" | "disk"} metric
 * @param {object | null | undefined} host the shell's `HostLoad`
 * @param {object | null | undefined} info the shell's `HostInfo`
 * @param {number} ours Bisa's share of the metric (CPU as a percent of the host, memory in bytes)
 * @param {object | null | undefined} [disk] the shell's `DiskUsage`
 * @returns {{value: string, title: string}}
 */
/**
 * The words a stat adds when its last read failed and the reading shown is
 * the one kept — nothing while the reads answer.
 * @param {string | null | undefined} stale the failed read's reason
 */
export function staleWords(stale) {
  return stale ? ` · ${tr("shell-resource-last-reading-kept", { stale })}` : "";
}

export function metricWords(metric, host, info, ours, disk) {
  switch (metric) {
    case "cpu": {
      if (!host) return { value: "—", title: tr("shell-resource-cpu-read-yet") };
      const pieces = [tr("shell-resource-cpu-at", { percent: pct(host.cpu_percent) })];
      if (info?.cores) pieces.push(tr("shell-resource-cores", { cores: info.cores }));
      if (Array.isArray(host.load)) pieces.push(tr("shell-resource-load", { load: host.load.map(one).join(" ") }));
      pieces.push(tr("shell-resource-bisa-at", { value: pct(ours) }));
      return { value: pct(host.cpu_percent), title: pieces.join(" · ") };
    }
    case "memory": {
      if (!host) return { value: "—", title: tr("shell-resource-memory-read-yet") };
      const pieces = [tr("shell-resource-memory-at", { value: memWords(host.mem_used, host.mem_total) })];
      if (num(host.swap_total) > 0) pieces.push(tr("shell-resource-swap", { size: formatSize(num(host.swap_used)) }));
      pieces.push(tr("shell-resource-bisa-at", { value: formatSize(num(ours)) || "0 B" }));
      return { value: memWords(host.mem_used, host.mem_total), title: pieces.join(" · ") };
    }
    case "gpu": {
      const g = host?.gpu;
      if (!g) return { value: "—", title: tr("shell-resource-gpu-reader-machine") };
      const pieces = [tr("shell-resource-gpu-at", { percent: pct(g.util_percent) })];
      if (info?.cpu) pieces.push(info.cpu);
      if (typeof g.mem_used === "number") pieces.push(tr("shell-resource-in-use", { size: formatSize(g.mem_used) }));
      if (typeof g.renderer_percent === "number") pieces.push(tr("shell-resource-renderer", { percent: pct(g.renderer_percent) }));
      if (typeof g.tiler_percent === "number") pieces.push(tr("shell-resource-tiler", { percent: pct(g.tiler_percent) }));
      return { value: pct(g.util_percent), title: pieces.join(" · ") };
    }
    case "disk": {
      if (!disk) return { value: "—", title: tr("shell-resource-disk-read-yet") };
      const size = formatSize(num(disk.total)) || "0 B";
      return { value: size, title: `${tr("shell-resource-disk-at", { size })}${volumeWords(disk.volume)}` };
    }
    default:
      return { value: "—", title: "" };
  }
}

/** `9.4 / 16 GB` — both on the larger unit's word. */
export function memWords(used, total) {
  if (typeof used !== "number" || typeof total !== "number" || total <= 0) return "—";
  const t = formatSize(total);
  const unit = t.split(" ")[1] ?? "";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const power = Math.max(0, units.indexOf(unit));
  const n = used / 1024 ** power;
  return `${n.toFixed(n >= 10 || power === 0 ? 0 : 1)} / ${t}`;
}

/** ` · volume 41% of 926 GB used`, or nothing without a volume. */
export function volumeWords(volume) {
  if (!volume || !(num(volume.total) > 0)) return "";
  const used = Math.round((num(volume.used) / num(volume.total)) * 100);
  return ` · ${tr("shell-resource-volume", { used, total: formatSize(num(volume.total)), mount: volume.mount })}`;
}

/** The overlay's footnote: the cadence and what the read cost, plus the metric's caveat. */
export function footnote(metric, pollMs, readMs, intervalSecs) {
  const pieces = [tr("shell-resource-read-every", { s: Math.round(num(pollMs) / 1000) })];
  if (typeof readMs === "number") pieces.push(tr("shell-resource-read-cost", { ms: Math.round(readMs) }));
  if (metric === "disk" && typeof intervalSecs === "number") pieces.push(tr("shell-resource-activity-over", { s: Math.round(intervalSecs) }));
  if (metric === "gpu") pieces.push(tr("shell-resource-per-process-readable-macos-without-elevated"));
  return pieces.join(" · ");
}

// ---------------------------------------------------------------------------
// Disk: the data directory by area, by the layout's rules
// ---------------------------------------------------------------------------

/** The top level of the data directory, named for a person. */
const AREA_WORDS = Object.freeze({
  goals: tr("shell-omnibox-goals"),
  projects: tr("shell-omnibox-projects"),
  attachments: tr("shell-resource-attachments"),
  sessions: tr("shell-resource-session-records"),
  logs: tr("shell-resource-diagnostic-log"),
  index: tr("shell-resource-index-cache"),
  ide: tr("shell-resource-ide-furniture"),
  run: tr("shell-resource-run-sockets-terminals"),
  notes: tr("shell-resource-notes"),
  conversation: tr("shell-aux-pane-conversations"),
  activity: tr("shell-resource-activity-log"),
  workflows: tr("shell-nav-workflows"),
  identity: tr("shell-profile-menu-identity"),
});

function sizeOf(disk, path) {
  return num((disk?.dirs ?? []).find((d) => d.path === path)?.bytes);
}

function under(disk, prefix) {
  return (disk?.dirs ?? []).filter((d) => d.path.startsWith(`${prefix}/`) && !d.path.slice(prefix.length + 1).includes("/"));
}

/** The workspace's areas: every first-level entry, the named ones by their words, the rest as they are. */
export function areas(disk) {
  const rows = (disk?.dirs ?? [])
    .filter((d) => !d.path.includes("/"))
    .map((d) => ({ key: `area:${d.path}`, label: AREA_WORDS[d.path] ?? d.path, sub: AREA_WORDS[d.path] ? d.path : null, door: null, value: num(d.bytes), count: null }));
  return sortRows(rows);
}

/** Each goal's disk: `goals/<id>` and its notes `notes/goals/<id>`, by label. */
export function goalsDisk(disk, places = emptyPlaceIndex()) {
  const byId = new Map();
  for (const d of under(disk, "goals")) byId.set(d.path.slice("goals/".length), num(d.bytes));
  for (const d of under(disk, "notes/goals")) {
    const id = d.path.slice("notes/goals/".length);
    byId.set(id, (byId.get(id) ?? 0) + num(d.bytes));
  }
  return sortRows([...byId].map(([id, value]) => ({ key: `goal:${id}`, label: placeWords("goal", id, places), sub: null, door: { kind: "goal", id }, value, count: null })));
}

/** Each project's disk: `projects/<slug>` (its checkouts' share as the sub-line) and its notes. */
export function projectsDisk(disk, places = emptyPlaceIndex()) {
  const bySlug = new Map();
  for (const d of under(disk, "projects")) bySlug.set(d.path.slice("projects/".length), { value: num(d.bytes), checkouts: 0 });
  for (const d of under(disk, "notes/projects")) {
    const slug = d.path.slice("notes/projects/".length);
    const e = bySlug.get(slug) ?? { value: 0, checkouts: 0 };
    e.value += num(d.bytes);
    bySlug.set(slug, e);
  }
  const slugs = places.projectSlugs ?? new Map();
  return sortRows(
    [...bySlug].map(([slug, e]) => ({
      key: `project:${slug}`,
      label: slugs.get(slug)?.name ?? slug,
      sub: null,
      door: slugs.get(slug) ? { kind: "project", id: slugs.get(slug).id } : null,
      value: e.value,
      count: null,
    })),
  );
}

/** Each harness's session records under `sessions/<adapter>`; a transcript is the harness's own file elsewhere. */
export function harnessesDisk(disk, harnessLabels = {}) {
  return sortRows(under(disk, "sessions").map((d) => {
    const adapter = d.path.slice("sessions/".length);
    return { key: `harness:${adapter}`, label: harnessLabels[adapter] ?? adapter, sub: tr("shell-resource-session-records-transcripts-harness-s-own"), door: null, value: num(d.bytes), count: null };
  }));
}

/** The terminals' scrollback under `run/terminals`. */
export function terminalsDisk(disk) {
  const bytes = sizeOf(disk, "run/terminals");
  return [{ key: "terminals", label: tr("shell-resource-terminal-scrollback"), sub: "run/terminals", door: null, value: bytes, count: null }];
}

/** Bytes read and written over the last interval, per platform row. */
export function activityRows(shares) {
  const sum = (kind, field) => (shares ?? []).filter((s) => s.kind === kind).reduce((n, s) => n + num(s[field]), 0);
  const row = (key, label, kind) => {
    const read = sum(kind, "read");
    const written = sum(kind, "written");
    return { key, label, sub: tr("shell-resource-read-written", { read: formatSize(read) || "0 B", written: formatSize(written) || "0 B" }), door: null, value: read + written, count: null };
  };
  return sortRows([row("desktop", tr("shell-resource-desktop-app"), "desktop"), row("node", tr("shell-resource-node"), "node"), row("harnesses", tr("shell-resource-harnesses"), "session"), row("terminals", tr("shell-resource-terminals"), "terminal")]);
}

/**
 * The rows of one overlay dimension, whatever the metric — the one door the
 * paint goes through.
 * @param {"cpu" | "memory" | "disk"} metric
 * @param {string} dimension
 * @param {{shares: readonly object[], host?: object | null, info?: object | null, disk?: object | null, places?: object, harnessLabels?: Record<string, string>}} ctx
 */
export function overlayRows(metric, dimension, ctx) {
  const places = ctx.places ?? emptyPlaceIndex();
  const cores = ctx.info?.cores ?? 1;
  if (metric === "disk") {
    switch (dimension) {
      case "areas":
        return areas(ctx.disk);
      case "goals":
        return goalsDisk(ctx.disk, places);
      case "projects":
        return projectsDisk(ctx.disk, places);
      case "harnesses":
        return harnessesDisk(ctx.disk, ctx.harnessLabels ?? {});
      case "terminals":
        return terminalsDisk(ctx.disk);
      case "activity":
        return activityRows(ctx.shares);
      default:
        return [];
    }
  }
  if (dimension === "platform") return platformRows(ctx.shares, ctx.host, cores, metric);
  return dimensionRows(dimension, ctx.shares, places, metric, cores, ctx.harnessLabels ?? {});
}
