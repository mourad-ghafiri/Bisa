/**
 * The footer's node read-out: the dot's tone and sentence per connection
 * state, and the overlay's sections. Run with `node --test desktop/src/shell/nodeStatModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { footnote, nodeShare, overlaySections, sinceOf, sinceWords, statWords } from "./nodeStatModel.mjs";

const NOW = 1_000_000;
const info = (over = {}) => ({
  version: "0.1.0",
  pid: 4242,
  started_at: NOW - (2 * 3600 + 13 * 60),
  socket: "/Users/a/.bisa/run/node.sock",
  listen: "http://127.0.0.1:4477",
  data_dir: "/Users/a/.bisa",
  logs_dir: "/Users/a/.bisa/logs",
  paused: false,
  live_sessions: 3,
  ...over,
});
const status = (over = {}) => ({ running: true, pid: 4242, port: 4477, external: false, restarts: 0, healthy_secs: 7800, ...over });
const workspace = (over = {}) => ({
  pubkey: "ab",
  npub: "npub1x",
  data_dir: "/Users/a/.bisa",
  logs_dir: "/Users/a/.bisa/logs",
  members: [],
  ...over,
});
const relay = (url, connected) => ({ url, status: connected ? "connected" : "disconnected", connected, attempts: 1, success: connected ? 1 : 0, success_rate: connected ? 1 : 0, bytes_sent: 0, bytes_received: 0 });
const sync = (over = {}) => ({
  running: true,
  enabled: true,
  relays: [relay("wss://a", true), relay("wss://b", true), relay("wss://c", false)],
  connected_relays: 2,
  published: 41,
  ingested: 17,
  last_catchup: NOW - 90,
  people: 2,
  hosts: 1,
  iroh_node_id: "n0abc",
  iroh_peers_connected: 1,
  ...over,
});
const share = { pid: 4242, name: "bisa", root: { kind: "node" }, cpu_percent: 3.4, mem_bytes: 120 * 1024 * 1024, read_bytes: 0, written_bytes: 0, run_secs: 7800 };
const facts = (over = {}) => ({ info: info(), status: status(), workspace: workspace(), sync: sync(), checks: null, paused: false, share, appVersion: "0.1.0", apiBase: "http://127.0.0.1:4477", now: NOW, ...over });
const rows = (section) => Object.fromEntries(section.rows.map((r) => [r.label, r.value]));

test("the dot and the sentence follow the connection: green connected, amber connecting, red unreachable", () => {
  assert.deepEqual(statWords("open"), { tone: "ok", word: "connected", title: "The node is connected." });
  assert.deepEqual(statWords("connecting"), { tone: "warn", word: "connecting", title: "Connecting to the node…" });
  assert.deepEqual(statWords("closed"), { tone: "danger", word: "unreachable", title: "The node is unreachable." });
});

test("a span reads in the unit that fits and never goes negative", () => {
  assert.equal(sinceWords(42), "42 s");
  assert.equal(sinceWords(59), "59 s");
  assert.equal(sinceWords(60), "1 min");
  assert.equal(sinceWords(3599), "59 min");
  assert.equal(sinceWords(3600), "1 h");
  assert.equal(sinceWords(2 * 3600 + 13 * 60), "2 h 13 min");
  assert.equal(sinceWords(86400 * 3 + 7200), "3 d 2 h");
  assert.equal(sinceWords(86400), "1 d");
  assert.equal(sinceWords(-5), "0 s");
  assert.equal(sinceOf(100, 112), 12);
  assert.equal(sinceOf(200, 112), 0, "a clock ahead of the node is not a negative span");
});

test("the sections come in order with the node's facts, the shell's, the sync's and the load's", () => {
  const s = overlaySections("open", facts());
  assert.deepEqual(s.map((x) => x.key), ["connection", "node", "sync", "load"]);
  assert.equal(s[0].label, "connected");
  assert.equal(s[0].tone, "ok");
  assert.equal(s[0].sentence, null);
  assert.deepEqual(rows(s[0]), { Address: "http://127.0.0.1:4477", "Run by": "this app · pid 4242", Up: "2 h 13 min" }, "no restarts row at zero");
  assert.equal(s[1].label, "running");
  assert.equal(s[1].tone, "ok");
  assert.equal(s[1].sentence, null, "the versions agree: no caution");
  assert.deepEqual(rows(s[1]), {
    Version: "0.1.0 · desktop 0.1.0",
    Sessions: "3 live",
    Socket: "/Users/a/.bisa/run/node.sock",
    "Listens on": "http://127.0.0.1:4477",
    Data: "/Users/a/.bisa",
    Logs: "/Users/a/.bisa/logs",
  });
  assert.equal(s[2].label, "2 of 3 connected");
  // One row per relay with its state, then the people hosted here, the hosts joined, the ledger, the direct sessions and the last catch-up.
  assert.deepEqual(rows(s[2]), { "wss://a": "connected", "wss://b": "connected", "wss://c": "disconnected", People: "2 hosted here", Hosts: "1 joined", Published: "41", Ingested: "17", "Direct sessions": "1", "Last catch-up": "1 min ago" });
  assert.deepEqual(rows(s[3]), { CPU: "3%", Memory: "120 MB", Running: "2 h 10 min" });
  assert.equal(s[3].sentence, null);
});

test("who runs the node: this app, a person outside it, or nobody the window can ask", () => {
  assert.equal(rows(overlaySections("open", facts({ status: status({ external: true, pid: null }) }))[0])["Run by"], "you — a node started outside the app");
  assert.equal(rows(overlaySections("open", facts({ status: null }))[0])["Run by"], "—", "off the shell");
  assert.equal(rows(overlaySections("open", facts({ status: status({ pid: null }) }))[0])["Run by"], "this app");
  const restarted = overlaySections("open", facts({ status: status({ restarts: 2 }) }));
  assert.equal(rows(restarted[0]).Restarts, "2");
  // The shell's healthy span stands in for the node's own start when the node was not read.
  assert.equal(rows(overlaySections("open", facts({ info: null, status: status({ healthy_secs: 125 }) }))[0]).Up, "2 min");
  assert.equal(rows(overlaySections("open", facts({ info: null, status: null }))[0]).Up, undefined);
});

test("the node's chip is the engine's flag, its sentence the version caution, and a socket-only node says so", () => {
  const paused = overlaySections("open", facts({ paused: true }));
  assert.equal(paused[1].label, "paused");
  assert.equal(paused[1].tone, "warn");
  const unread = overlaySections("open", facts({ paused: null }));
  assert.equal(unread[1].label, "not read");
  const differ = overlaySections("open", facts({ appVersion: "0.2.0" }));
  assert.match(differ[1].sentence, /The node is 0\.1\.0 and this desktop 0\.2\.0/);
  assert.equal(rows(differ[1]).Version, "0.1.0 · desktop 0.2.0");
  assert.equal(rows(overlaySections("open", facts({ info: info({ listen: null }) }))[1])["Listens on"], "the socket only");
  assert.equal(rows(overlaySections("open", facts({ info: info({ listen: undefined }) }))[1])["Listens on"], "the socket only");
});

test("the relays section lists every relay with its state, says off under the switch, and reads the last checks", () => {
  const on = overlaySections("open", facts());
  assert.equal(on[2].title, "Relays");
  assert.deepEqual(on[2].rows.slice(0, 3), [
    { label: "wss://a", value: "connected" },
    { label: "wss://b", value: "connected" },
    { label: "wss://c", value: "disconnected" },
  ]);
  assert.equal(rows(on[2]).People, "2 hosted here");
  const off = overlaySections("open", facts({ sync: sync({ enabled: false, relays: [relay("wss://a", false)].map((r) => ({ ...r, status: "off" })), connected_relays: 0 }) }));
  assert.equal(off[2].label, "off");
  assert.equal(off[2].sentence, "Relays are off — turn them on in Settings › Workspace › Relays & sync.");
  assert.deepEqual(off[2].rows, [{ label: "wss://a", value: "off" }], "the relays stay listed while off");
  const noPump = overlaySections("open", facts({ sync: sync({ running: false }) }));
  assert.equal(noPump[2].label, "no pump");
  assert.equal(noPump[2].sentence, "No collaboration pump runs on this node.");
  const unread = overlaySections("open", facts({ sync: null }));
  assert.equal(unread[2].label, "not read");
  const alone = overlaySections("open", facts({ sync: sync({ relays: [], connected_relays: 0, published: 0, ingested: 0, people: 0, hosts: 0, iroh_node_id: null, iroh_peers_connected: 0, last_catchup: null }) }));
  assert.equal(alone[2].label, "none configured");
  assert.equal(alone[2].tone, "quiet");
  const deaf = overlaySections("open", facts({ sync: sync({ relays: [relay("wss://a", false)], connected_relays: 0 }) }));
  assert.equal(deaf[2].label, "no relay answers");
  assert.equal(deaf[2].tone, "danger");
  const checked = overlaySections("open", facts({ checks: [{ url: "wss://a", ok: true, latency_ms: 12 }, { url: "wss://c", ok: false, error: "refused" }] }));
  assert.deepEqual(checked[2].rows.slice(0, 3), [
    { label: "wss://a", value: "reachable · 12 ms" },
    { label: "wss://b", value: "connected" },
    { label: "wss://c", value: "not reachable — refused" },
  ]);
  assert.deepEqual(overlaySections("open", facts({ workspace: null })).map((x) => x.key), ["connection", "node", "load"], "no workspace read: no relays section");
});

test("an unreachable node keeps the last read under its sentence; nothing read says so", () => {
  const down = overlaySections("closed", facts());
  assert.equal(down[0].label, "unreachable");
  assert.equal(down[0].tone, "danger");
  assert.equal(down[0].sentence, "The node is unreachable; what follows is the last read.");
  assert.equal(rows(down[1]).Version, "0.1.0 · desktop 0.1.0", "the last read stays");
  const nothing = overlaySections("connecting", facts({ info: null, status: null, workspace: null, paused: null, share: null }));
  assert.deepEqual(nothing.map((x) => x.key), ["connection", "node", "load"]);
  assert.equal(nothing[0].label, "connecting");
  assert.deepEqual(rows(nothing[0]), { Address: "http://127.0.0.1:4477", "Run by": "—" });
  assert.equal(nothing[1].label, "not read");
  assert.equal(nothing[1].sentence, "The node has not been read yet.");
  assert.deepEqual(nothing[1].rows, []);
  assert.equal(nothing[2].sentence, "Not read yet.");
});

test("the node's share is the process whose root is the node; the footnote says when the read landed", () => {
  assert.equal(nodeShare([{ ...share, root: { kind: "desktop" } }, share]), share);
  assert.equal(nodeShare([]), null);
  assert.equal(nodeShare(null), null);
  assert.equal(footnote(null), "not read yet");
  assert.equal(footnote(1000, 1012), "read 12 s ago");
});

test("the footer's words for the node are the catalog's: connected, connecting, unreachable, and the engine running or paused", async () => {
  const { readFileSync } = await import("node:fs");
  const model = readFileSync(new URL("./nodeStatModel.mjs", import.meta.url), "utf8");
  const said = model.slice(model.indexOf("export function statWords"), model.indexOf("export function", model.indexOf("export function statWords") + 10));
  for (const word of ["connected", "connecting", "unreachable"]) assert.ok(said.includes(`t("shell-node-stat-word-${word}")`) && !said.includes(`word: "${word}"`), word);
  assert.ok(model.includes('t("shell-node-stat-engine-paused")') && model.includes('t("shell-node-stat-engine-running")') && !model.includes('label: "paused"') && !model.includes('label: "running"'));
});
