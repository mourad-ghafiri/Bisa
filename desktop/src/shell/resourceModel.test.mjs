/**
 * The footer's resources are facts before they are pixels. Run with
 * `node --test desktop/src/shell/resourceModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { placeIndex } from "./footerSessionsModel.mjs";
import {
  DIMENSIONS,
  activityRows,
  areas,
  attributeShares,
  chosenDimension,
  dimensionIcon,
  dimensionLabel,
  dimensionRows,
  foldRows,
  footnote,
  goalsDisk,
  harnessesDisk,
  memWords,
  metricWords,
  moreWords,
  oursOf,
  overlayRows,
  platformRows,
  projectsDisk,
  rowPercent,
  percentWords,
  usageBar,
  terminalsDisk,
  valueWords,
  volumeWords,
} from "./resourceModel.mjs";

const MB = 1024 * 1024;
const GB = 1024 * MB;

const live = { status: "live" };

/** The workspace: one project with a workstream, two goals on one workflow. */
const PLACES = placeIndex({
  projects: [{ project: { id: "p1", slug: "shop", name: "Shop" } }],
  workstreams: [{ workstream: { id: "w1", project: "p1", kind: "primary", name: "main" }, project_name: "Shop" }],
  goals: [
    { id: "g1", label: "Ship the cart", workflow: "wf1", workflowName: "Ship it" },
    { id: "g2", label: "Fix the total", workflow: "wf1", workflowName: "Ship it" },
  ],
});

/** The roster: a worker on g1 in w1 (pid 300), a worker on g2 nowhere (pid 400), an interactive session a tab claims (pid 500). */
const SESSIONS = [
  { id: "s1", kind: "worker", state: { state: "running" }, harness: "claude-code", agent: "builder", goal: "g1", project: "p1", workstream: "w1", pid: 300 },
  { id: "s2", kind: "worker", state: { state: "running" }, harness: "codex", agent: null, goal: "g2", project: null, workstream: null, pid: 400 },
  { id: "s3", kind: "terminal", state: { state: "idle" }, harness: "claude-code", agent: null, goal: null, project: null, workstream: "w1", pid: 500 },
];

/** The tabs: a plain shell in w1 (PTY t1), the tab that claims s3 (PTY t2), a shell on the machine running a harness nobody reported (PTY t3). */
const TERMINALS = [
  { key: "k1", scope: "workstream", id: "w1", harness: null, running: null, sessionId: null, terminalId: "t1", liveness: live },
  { key: "k2", scope: "workstream", id: "w1", harness: "claude-code", running: "claude-code", sessionId: "s3", terminalId: "t2", liveness: live },
  { key: "k3", scope: "machine", id: "machine", harness: null, running: "codex", sessionId: null, terminalId: "t3", liveness: live },
];

const proc = (pid, root, cpu, mem, extra = {}) => ({ pid, name: `p${pid}`, root, cpu_percent: cpu, mem_bytes: mem, read_bytes: 0, written_bytes: 0, run_secs: 10, ...extra });

const PROCESSES = [
  proc(10, { kind: "desktop" }, 20, 400 * MB, { read_bytes: 1 * MB, written_bytes: 2 * MB }),
  proc(20, { kind: "node" }, 10, 200 * MB),
  proc(300, { kind: "session", pid: 300 }, 100, 800 * MB, { written_bytes: 5 * MB }),
  proc(301, { kind: "session", pid: 300 }, 50, 100 * MB),
  proc(400, { kind: "session", pid: 400 }, 30, 300 * MB),
  proc(600, { kind: "session", pid: 600 }, 5, 50 * MB),
  proc(1000, { kind: "terminal", terminal_id: "t1" }, 2, 30 * MB, { read_bytes: 3 * MB }),
  proc(2000, { kind: "terminal", terminal_id: "t2" }, 40, 600 * MB),
  proc(3000, { kind: "terminal", terminal_id: "t3" }, 8, 90 * MB),
  proc(4000, { kind: "terminal", terminal_id: "gone" }, 8, 90 * MB),
];

const HOST = { cpu_percent: 40, load: [2.1, 1.8, 1.5], mem_used: 9.4 * GB, mem_total: 16 * GB, swap_used: 1.2 * GB, swap_total: 4 * GB, uptime_secs: 100, gpu: null };
const INFO = { cores: 10, cpu: "Apple M2 Max", hostname: "mac", os: "macOS 15" };
const LABELS = { "claude-code": "Claude Code", codex: "Codex" };

const shares = () => attributeShares(PROCESSES, SESSIONS, TERMINALS, PLACES);

test("a session root joins its roster row: goal, project, workstream, harness, and every process of its tree", () => {
  const rows = shares().filter((s) => s.sessionId === "s1");
  assert.equal(rows.length, 2, "the harness and its child");
  assert.deepEqual(rows.map((r) => [r.goal, r.project, r.workstream, r.harness, r.agent]), [["g1", "p1", "w1", "claude-code", "builder"], ["g1", "p1", "w1", "claude-code", "builder"]]);
});

test("a session with no project takes its project from its workstream, and one with neither names none", () => {
  const s2 = shares().find((s) => s.sessionId === "s2");
  assert.equal(s2.project, null);
  assert.equal(s2.goal, "g2");
});

test("a tab that claims a reported session folds into that session; an unclaimed tab keeps its place and the harness it runs", () => {
  const all = shares();
  const claimed = all.find((s) => s.pid === 2000);
  assert.equal(claimed.kind, "session");
  assert.equal(claimed.sessionId, "s3");
  assert.equal(claimed.terminalKey, "k2");
  assert.equal(claimed.workstream, "w1");
  const plain = all.find((s) => s.pid === 1000);
  assert.equal(plain.kind, "terminal");
  assert.equal(plain.terminalKey, "k1");
  assert.equal(plain.project, "p1", "a workstream tab's project comes from the index");
  assert.equal(plain.harness, null);
  const machine = all.find((s) => s.pid === 3000);
  assert.equal(machine.harness, "codex", "what the process table showed in the shell");
  assert.equal(machine.project, null);
});

test("a session root the roster no longer names is kept as a session that just ended; a tab nobody knows is dropped", () => {
  const all = shares();
  const ended = all.find((s) => s.pid === 600);
  assert.equal(ended.kind, "session");
  assert.equal(ended.ended, true);
  assert.equal(ended.sessionId, null);
  assert.equal(all.find((s) => s.pid === 4000), undefined);
});

test("the platform rows sum each part over the cores and the machine is what is left of the host", () => {
  const rows = platformRows(shares(), HOST, 10, "cpu");
  const by = Object.fromEntries(rows.map((r) => [r.key, r]));
  assert.equal(by.desktop.value, 2);
  assert.equal(by.node.value, 1);
  // sessions: 300+301 (150) + 400 (30) + 600 (5) + the claimed tab 2000 (40) = 225 per core → 22.5
  assert.equal(by.harnesses.value, 22.5);
  assert.equal(by.harnesses.count, 4, "s1, s2, s3 and the ended one");
  assert.equal(by.terminals.value, 1);
  assert.equal(by.terminals.count, 2);
  assert.equal(by.machine.value, 40 - (2 + 1 + 22.5 + 1));
  assert.equal(by.desktop.share, 0.02);
  assert.match(by.desktop.sub, /WebKit/);
  const mem = platformRows(shares(), HOST, 10, "memory");
  assert.equal(mem.find((r) => r.key === "desktop").value, 400 * MB);
  assert.equal(mem.find((r) => r.key === "desktop").share, (400 * MB) / (16 * GB));
});

test("the machine row never goes below zero when the shares outrun the host's figure", () => {
  const rows = platformRows(shares(), { ...HOST, cpu_percent: 1 }, 10, "cpu");
  assert.equal(rows.find((r) => r.key === "machine").value, 0);
});

test("the usage bar puts Bisa's parts first, the machine after them and the free rest as the track, every band toned apart from the track", () => {
  const bar = usageBar(platformRows(shares(), HOST, 10, "cpu"), "cpu");
  assert.deepEqual(bar.segments.map((s) => s.key), ["desktop", "node", "harnesses", "terminals", "machine"]);
  assert.equal(bar.segments.find((s) => s.key === "harnesses").percent, 22.5);
  assert.equal(bar.segments.find((s) => s.key === "machine").tone, "neutral", "the machine has a tone of its own — never the track's");
  assert.ok(bar.segments.every((s) => s.tone !== "dim"));
  const sum = bar.segments.reduce((n, s) => n + s.percent, 0);
  assert.ok(sum <= 100.0001, `${sum}`);
  assert.equal(Math.round(sum), 40, "the parts and the machine add up to the host's figure");
  const nobody = usageBar(platformRows([], HOST, 10, "cpu"), "cpu");
  assert.deepEqual(nobody.segments.map((s) => s.key), ["machine"], "nothing of Bisa: the machine alone");
  assert.equal(usageBar([], "cpu").segments.length, 0, "no host figure: no bar");
  assert.deepEqual(usageBar([], "cpu").legend, []);
});

test("the legend says Bisa, the machine and the free rest in the metric's words with rounded percents, and the label is the whole sentence", () => {
  const cpu = usageBar(platformRows(shares(), HOST, 10, "cpu"), "cpu");
  assert.deepEqual(cpu.legend.map((l) => [l.key, l.tone]), [["bisa", "accent"], ["machine", "neutral"], ["free", "track"]]);
  assert.equal(cpu.legend[2].words, "60%", "CPU reads as a percent alone");
  assert.match(cpu.label, /^CPU: Bisa \d+% \(desktop \d+%, node \d+%, harnesses \d+%, terminals \d+%\), machine \d+%, free 60%$/);
  const mem = usageBar(platformRows(shares(), HOST, 10, "memory"), "memory");
  assert.match(mem.legend[0].words, /^[\d.]+ (MB|GB) · \d+%$/, mem.legend[0].words);
  assert.match(mem.legend[1].words, /GB · \d+%$/);
  assert.match(mem.legend[2].words, /GB · 41%$/, "16 GB with 9.4 in use leaves 41% free");
  assert.ok(mem.label.startsWith("Memory: Bisa "));
});

test("a part under one percent still has a band and reads as under one percent; the machine's band gives way when the host's figure lags its parts", () => {
  assert.equal(percentWords(0.4), "<1%");
  assert.equal(percentWords(0), "0%");
  assert.equal(percentWords(41.6), "42%");
  const tiny = usageBar([
    { key: "machine", label: "Machine", sub: "", value: 50, count: null, door: null, share: 0.5 },
    { key: "desktop", label: "Desktop app", sub: "", value: 0.3, count: null, door: null, share: 0.003 },
  ], "cpu");
  assert.equal(tiny.segments[0].key, "desktop");
  assert.ok(tiny.segments[0].percent > 0 && tiny.segments[0].percent < 1);
  assert.match(tiny.label, /desktop <1%/);
  const lagging = usageBar([
    { key: "machine", label: "Machine", sub: "", value: 80, count: null, door: null, share: 0.8 },
    { key: "node", label: "Node", sub: "", value: 40, count: null, door: null, share: 0.4 },
  ], "cpu");
  const total = lagging.segments.reduce((n, s) => n + s.percent, 0);
  assert.equal(total, 100, "clamped at the track");
  assert.equal(lagging.segments.find((s) => s.key === "machine").percent, 60);
  assert.equal(lagging.legend[2].words, "0%");
});

test("goals: every share on a goal summed under its label, the rest elsewhere, largest first, doors to the goal", () => {
  const rows = dimensionRows("goals", shares(), PLACES, "cpu", 10, LABELS);
  assert.deepEqual(rows.map((r) => [r.key, r.label, r.value]), [
    ["goal:g1", "Ship the cart", 15],
    ["elsewhere", "Elsewhere", 5.5],
    ["goal:g2", "Fix the total", 3],
  ]);
  assert.deepEqual(rows[0].door, { kind: "goal", id: "g1" });
  assert.equal(rows[0].sub, "Claude Code");
  assert.equal(rows[1].door, null);
});

test("projects aggregate by id and read by name; a workstream's tab counts for its project", () => {
  const rows = dimensionRows("projects", shares(), PLACES, "memory", 10, LABELS);
  const shop = rows.find((r) => r.key === "project:p1");
  assert.equal(shop.label, "Shop");
  assert.equal(shop.value, (800 + 100 + 30 + 600) * MB, "s1's tree, the plain tab, the claimed tab");
  assert.deepEqual(shop.door, { kind: "project", id: "p1" });
  assert.equal(rows.find((r) => r.key === "elsewhere").value, (300 + 50 + 90) * MB);
});

test("workflows are the goal's, by the run's name, with a door to the workflow", () => {
  const rows = dimensionRows("workflows", shares(), PLACES, "cpu", 10, LABELS);
  assert.equal(rows[0].key, "workflow:wf1");
  assert.equal(rows[0].label, "Ship it");
  assert.equal(rows[0].value, 18, "both goals");
  assert.equal(rows[0].count, 2);
  assert.deepEqual(rows[0].door, { kind: "workflow", id: "wf1" });
  assert.equal(rows[1].key, "elsewhere");
});

test("a run of the workspace's session files under its run, with a door to the run's page, and under no goal", () => {
  const worker = { id: "s9", kind: "worker", state: { state: "running" }, harness: "codex", agent: null, goal: null, run: "01JRUN0000UVWXYZ", project: null, workstream: null, pid: 700 };
  const runShares = attributeShares([proc(700, { kind: "session", pid: 700 }, 20, 100 * MB)], [worker], [], PLACES);
  assert.equal(runShares[0].run, "01JRUN0000UVWXYZ");
  const rows = dimensionRows("workflows", runShares, PLACES, "cpu", 10, LABELS);
  assert.deepEqual(rows.map((r) => [r.key, r.label, r.sub, r.value]), [["run:01JRUN0000UVWXYZ", "run ·UVWXYZ", "Codex", 2]]);
  assert.deepEqual(rows[0].door, { kind: "run", id: "01JRUN0000UVWXYZ" });
  assert.deepEqual(dimensionRows("goals", runShares, PLACES, "cpu", 10, LABELS).map((r) => r.key), ["elsewhere"]);
});

test("harnesses group by kind under the catalog's label, a tab's harness included", () => {
  const rows = dimensionRows("harnesses", shares(), PLACES, "cpu", 10, LABELS);
  assert.deepEqual(rows.map((r) => [r.label, r.value]), [
    ["Claude Code", 19],
    ["Codex", 3.8],
    ["Elsewhere", 0.7],
  ]);
});

test("terminals are one row per tab, a claimed harness under its tab, with a door to the tab", () => {
  const rows = dimensionRows("terminals", shares(), PLACES, "cpu", 10, LABELS);
  const k2 = rows.find((r) => r.key === "terminal:k2");
  assert.equal(k2.label, "Claude Code");
  assert.equal(k2.sub, "Shop › main");
  assert.equal(k2.value, 4);
  assert.deepEqual(k2.door, { kind: "terminal", key: "k2", scope: "workstream", id: "w1" });
  const k1 = rows.find((r) => r.key === "terminal:k1");
  assert.equal(k1.label, "shell");
  assert.deepEqual(k1.door, { kind: "terminal", key: "k1", scope: "workstream", id: "w1" });
  const k3 = rows.find((r) => r.key === "terminal:k3");
  assert.equal(k3.label, "Codex");
  assert.equal(k3.sub, "this machine");
  assert.equal(rows.find((r) => r.key === "elsewhere").value, 18.5, "sessions in no tab");
});

test("the fold keeps the first n rows and sums the rest into one line", () => {
  const rows = Array.from({ length: 11 }, (_, i) => ({ key: `r${i}`, value: 11 - i }));
  const f = foldRows(rows, 8);
  assert.equal(f.rows.length, 8);
  assert.deepEqual(f.more, { count: 3, value: 3 + 2 + 1 });
  assert.equal(moreWords("cpu", f.more), "+3 more · 6%");
  assert.equal(foldRows(rows.slice(0, 3), 8).more, null);
  assert.equal(moreWords("memory", null), "");
});

test("values read in the metric's words and a row's bar is its share of the top row", () => {
  assert.equal(valueWords("cpu", 22.46), "22.5%");
  assert.equal(valueWords("memory", 640 * MB), "640 MB");
  assert.equal(valueWords("disk", 0), "0 B");
  const rows = [{ value: 40 }, { value: 10 }];
  assert.equal(rowPercent(rows[1], rows), 25);
  assert.equal(rowPercent({ value: 5 }, []), 0);
});

test("the metric words: the bar's value and the overlay's sentence, with and without a GPU", () => {
  const ours = oursOf(shares(), "cpu", 10);
  assert.equal(ours, 26.5);
  assert.deepEqual(metricWords("cpu", HOST, INFO, ours), { value: "40%", title: "CPU 40% · 10 cores · load 2.1 1.8 1.5 · Bisa 27%" });
  assert.deepEqual(metricWords("memory", HOST, INFO, 2.6 * GB), { value: "9.4 / 16 GB", title: "Memory 9.4 / 16 GB · swap 1.2 GB · Bisa 2.6 GB" });
  assert.deepEqual(metricWords("gpu", HOST, INFO, 0), { value: "—", title: "GPU — no reader on this machine" });
  const gpu = { ...HOST, gpu: { util_percent: 30, renderer_percent: 15, tiler_percent: 20, mem_used: 556 * MB } };
  assert.deepEqual(metricWords("gpu", gpu, INFO, 0), { value: "30%", title: "GPU 30% · Apple M2 Max · 556 MB in use · renderer 15% · tiler 20%" });
  assert.deepEqual(metricWords("cpu", null, INFO, 0), { value: "—", title: "CPU — not read yet" });
  assert.equal(memWords(2 * GB, 16 * GB), "2.0 / 16 GB");
  assert.equal(memWords(null, 16 * GB), "—");
});

test("the disk words carry the volume when there is one", () => {
  const disk = { total: 1.2 * GB, dirs: [], volume: { mount: "/", used: 380 * GB, total: 926 * GB }, read_ms: 120 };
  assert.deepEqual(metricWords("disk", HOST, INFO, 0, disk), { value: "1.2 GB", title: "Disk 1.2 GB · volume 41% of 926 GB used (/)" });
  assert.equal(volumeWords(null), "");
  assert.equal(volumeWords({ mount: "/", used: 1, total: 0 }), "");
  assert.deepEqual(metricWords("disk", HOST, INFO, 0, null), { value: "—", title: "Disk — not read yet" });
});

test("the footnote says the cadence, the cost, the activity's span and the GPU's caveat", () => {
  assert.equal(footnote("cpu", 5000, 38, 5.01), "read every 5 s · 38 ms");
  assert.equal(footnote("disk", 60000, 120, 5.01), "read every 60 s · 120 ms · activity over the last 5 s");
  assert.equal(footnote("gpu", 5000, null, null), "read every 5 s · per process: not readable on macOS without elevated access");
});

test("the chosen dimension is the remembered one while the metric offers it, else the first, and the GPU offers none", () => {
  assert.equal(chosenDimension("cpu", "goals"), "goals");
  assert.equal(chosenDimension("cpu", "areas"), "platform");
  assert.equal(chosenDimension("disk", null), "areas");
  assert.equal(chosenDimension("gpu", "platform"), null);
  assert.deepEqual(DIMENSIONS.disk, ["areas", "goals", "projects", "harnesses", "terminals", "activity"]);
});

test("every dimension of every metric has a word and a glyph, and each glyph is an entry of the icon registry", () => {
  const icons = readFileSync(new URL("../ui/icons.ts", import.meta.url), "utf8");
  const seen = new Set();
  for (const dims of Object.values(DIMENSIONS)) {
    for (const d of dims) {
      assert.notEqual(dimensionLabel(d), d, `${d} has a word`);
      const key = dimensionIcon(d);
      assert.match(icons, new RegExp(`^  ${key}: `, "m"), `${d} draws from ICON.${key}`);
      seen.add(key);
    }
  }
  assert.equal(seen.size, 8, "eight dimensions, eight glyphs");
  assert.equal(dimensionIcon("platform"), "platform");
});

test("every dimension's word is one short word, the row the overlay is sized for", () => {
  // The overlay's popover is forty type-rems wide for six of these with their
  // glyphs, none cut. A longer word — *Workstreams* — fails here before it
  // fails in the panel.
  const longest = new Set();
  for (const dims of Object.values(DIMENSIONS)) {
    for (const d of dims) {
      const word = dimensionLabel(d);
      assert.doesNotMatch(word, /\s/, `${d} is one word`);
      assert.ok(word.length <= 9, `${d}'s word "${word}" fits a segment`);
      if (word.length === 9) longest.add(word);
    }
  }
  assert.deepEqual([...longest].sort(), ["Harnesses", "Terminals", "Workflows", "Workspace"]);
});

const DISK = {
  total: 0,
  dirs: [
    { path: "goals", bytes: 300 },
    { path: "goals/g1", bytes: 200 },
    { path: "goals/g9", bytes: 100 },
    { path: "notes", bytes: 60 },
    { path: "notes/goals", bytes: 10 },
    { path: "notes/goals/g1", bytes: 10 },
    { path: "notes/projects", bytes: 50 },
    { path: "notes/projects/shop", bytes: 50 },
    { path: "projects", bytes: 5000 },
    { path: "projects/shop", bytes: 5000 },
    { path: "sessions", bytes: 30 },
    { path: "sessions/claude-code", bytes: 20 },
    { path: "sessions/codex", bytes: 10 },
    { path: "run", bytes: 400 },
    { path: "run/terminals", bytes: 400 },
    { path: "index", bytes: 1330 },
    { path: "logs", bytes: 20 },
    { path: "members.json", bytes: 3 },
  ],
  volume: null,
  read_ms: 1,
};

test("the workspace's areas are the top level, the known ones by their words, largest first", () => {
  const rows = areas(DISK);
  assert.deepEqual(rows.slice(0, 3).map((r) => [r.label, r.sub, r.value]), [
    ["Projects", "projects", 5000],
    ["Index (a cache)", "index", 1330],
    ["Run (sockets, terminals)", "run", 400],
  ]);
  assert.deepEqual(rows.find((r) => r.key === "area:members.json"), { key: "area:members.json", label: "members.json", sub: null, door: null, value: 3, count: null });
  assert.equal(rows.some((r) => r.key === "area:goals/g1"), false, "nothing below the top level");
});

test("a goal's disk is its folder and its notes, by label, a goal the index forgot by its tail", () => {
  const rows = goalsDisk(DISK, PLACES);
  assert.deepEqual(rows.map((r) => [r.label, r.value]), [
    ["Ship the cart", 210],
    ["goal ·g9", 100],
  ]);
  assert.deepEqual(rows[0].door, { kind: "goal", id: "g1" });
});

test("a project's disk is filed by slug, named by the index, with a door to the project", () => {
  const rows = projectsDisk(DISK, PLACES);
  assert.deepEqual(rows.map((r) => [r.label, r.value, r.door]), [["Shop", 5050, { kind: "project", id: "p1" }]]);
  assert.deepEqual(projectsDisk(DISK).map((r) => [r.label, r.door]), [["shop", null]], "no index: the slug, no door");
});

test("harnesses on disk are their session records by adapter; terminals are their scrollback", () => {
  assert.deepEqual(harnessesDisk(DISK, LABELS).map((r) => [r.label, r.value]), [
    ["Claude Code", 20],
    ["Codex", 10],
  ]);
  assert.match(harnessesDisk(DISK, LABELS)[0].sub, /transcripts are the harness's own/);
  assert.deepEqual(terminalsDisk(DISK).map((r) => [r.label, r.value]), [["Terminal scrollback", 400]]);
  assert.equal(terminalsDisk({ total: 0, dirs: [] })[0].value, 0);
});

test("activity is bytes read and written over the interval per platform row, largest first", () => {
  const rows = activityRows(shares());
  assert.equal(rows[0].key, "harnesses");
  assert.equal(rows[0].value, 5 * MB);
  assert.equal(rows[0].sub, "0 B read · 5.0 MB written");
  const desktop = rows.find((r) => r.key === "desktop");
  assert.equal(desktop.sub, "1.0 MB read · 2.0 MB written");
  assert.equal(rows.find((r) => r.key === "terminals").sub, "3.0 MB read · 0 B written");
});

test("overlayRows is the one door: every metric and dimension answers rows", () => {
  const ctx = { shares: shares(), host: HOST, info: INFO, disk: DISK, places: PLACES, harnessLabels: LABELS };
  assert.equal(overlayRows("cpu", "platform", ctx)[0].key, "machine");
  assert.equal(overlayRows("memory", "goals", ctx)[0].key, "goal:g1");
  assert.equal(overlayRows("disk", "areas", ctx)[0].label, "Projects");
  assert.equal(overlayRows("disk", "activity", ctx)[0].key, "harnesses");
  assert.deepEqual(overlayRows("disk", "platform", ctx), []);
});

test("a stat whose last read failed says the reading shown is the one kept, and why; nothing while the reads answer", async () => {
  const { staleWords } = await import("./resourceModel.mjs");
  assert.equal(staleWords(null), "");
  assert.equal(staleWords(undefined), "");
  assert.equal(staleWords(""), "");
  assert.equal(staleWords("the shell did not answer"), " · the last reading is kept — the shell did not answer");
});
