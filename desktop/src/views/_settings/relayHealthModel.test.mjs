import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { OFF_LINE, checkAbout, checkAllWords, checkWords, defaultRelays, isDefaultList, relayEntry, relayProblem, relayTone, relayWords, syncSummary, wireMoved } from "./relayHealthModel.mjs";

const relay = (over = {}) => ({
  url: "wss://relay.example",
  status: "connected",
  connected: true,
  attempts: 4,
  success: 3,
  success_rate: 0.75,
  latency_ms: 42,
  connected_at: 1000,
  bytes_sent: 2048,
  bytes_received: 1024,
  ...over,
});

test("a relay's tone follows its status, connected first", () => {
  assert.deepEqual(relayTone(relay()), { tone: "ok", word: "connected" });
  assert.deepEqual(relayTone(relay({ connected: false, status: "connecting" })), { tone: "warn", word: "connecting" });
  assert.deepEqual(relayTone(relay({ connected: false, status: "disconnected" })), { tone: "danger", word: "disconnected" });
  assert.deepEqual(relayTone(relay({ connected: false, status: "banned" })), { tone: "danger", word: "refused" });
  assert.deepEqual(relayTone(relay({ connected: false, status: "sleeping" })), { tone: "quiet", word: "idle" });
  assert.deepEqual(relayTone(relay({ connected: false, status: "off", attempts: 0 })), { tone: "quiet", word: "off" });
  assert.deepEqual(relayTone(null), { tone: "quiet", word: "unknown" });
  assert.deepEqual(relayTone(relay({ connected: false, status: "terminated" })), { tone: "danger", word: "closed" });
  // The words are the catalog's: the model spells no state of its own.
  const model = readFileSync(new URL("./relayHealthModel.mjs", import.meta.url), "utf8");
  assert.ok(!/word: "[a-z]+"/.test(model), "no English word outside the catalog");
});

test("the second line says what the relay proved", () => {
  assert.equal(relayWords(relay(), 1090), "42 ms · 75% of 4 attempts · up 1 min · 3.0 KB moved");
  assert.equal(relayWords(relay({ latency_ms: null, attempts: 0, connected: false, bytes_sent: 0, bytes_received: 0 })), "");
});

test("a relay that is not connected says why, in the node's own sentence, and a connected one says nothing", () => {
  const why = "never connected in 3 attempts — the address may be wrong, the relay down, or this network may not reach it";
  assert.equal(relayProblem(relay({ connected: false, status: "connecting", problem: `  ${why} ` })), why);
  assert.equal(relayProblem(relay({ problem: why })), "", "connected now: an old sentence is not shown");
  assert.equal(relayProblem(relay({ connected: false, status: "off" })), "", "off is not a problem");
  assert.equal(relayProblem(relay({ connected: false, problem: null })), "");
  assert.equal(relayProblem(null), "");
});

test("a check and a typed URL read as words", () => {
  assert.deepEqual(checkWords({ url: "x", ok: true, latency_ms: 9 }), { tone: "ok", text: "Reachable — answered in 9 ms." });
  assert.deepEqual(checkWords({ url: "x", ok: false, error: "refused" }), { tone: "danger", text: "Not reachable — refused" });
  assert.equal(checkWords(null), null);
  assert.deepEqual(relayEntry(" wss://r.example ", []), { ok: true, reason: "", url: "wss://r.example" });
  assert.equal(relayEntry("http://r", []).reason, "A relay is a ws:// or wss:// URL.");
  assert.equal(relayEntry("wss://r.example", ["wss://r.example"]).reason, "Already configured.");
  assert.equal(relayEntry("", []).reason, "");
});

test("the wire sums up for a tile", () => {
  assert.equal(syncSummary(null).running, false);
  assert.equal(syncSummary({ running: false, relays: [], connected_relays: 0 }).tone, "quiet");
  assert.deepEqual(syncSummary({ running: true, enabled: true, relays: [], connected_relays: 0 }), { running: true, enabled: true, line: "No relay configured — this node reaches nobody.", tone: "quiet" });
  assert.deepEqual(syncSummary({ running: true, enabled: true, relays: [relay(), relay({ connected: false })], connected_relays: 1 }), { running: true, enabled: true, line: "1 of 2 relays connected", tone: "ok" });
  assert.equal(syncSummary({ running: true, enabled: true, relays: [relay({ connected: false })], connected_relays: 0 }).tone, "danger");
  assert.deepEqual(syncSummary({ running: true, enabled: false, relays: [relay({ status: "off", connected: false })], connected_relays: 0 }), { running: true, enabled: false, line: OFF_LINE, tone: "quiet" });
});

test("a round of checks reads as one line, and the defaults come from the registry", () => {
  assert.equal(checkAllWords(null), null);
  assert.equal(checkAllWords([]), null);
  assert.deepEqual(checkAllWords([{ url: "a", ok: true }]), { tone: "ok", text: "The relay is reachable." });
  assert.deepEqual(checkAllWords([{ url: "a", ok: true }, { url: "b", ok: true }]), { tone: "ok", text: "All 2 relays are reachable." });
  assert.deepEqual(checkAllWords([{ url: "a", ok: false }, { url: "b", ok: false }]), { tone: "danger", text: "None of the 2 relays is reachable." });
  assert.deepEqual(checkAllWords([{ url: "a", ok: true }, { url: "b", ok: false }, { url: "c", ok: false }]), { tone: "warn", text: "1 of 3 relays reachable." });
  const defs = [{ key: "sync.enabled", default: false }, { key: "sync.relays", default: ["wss://relay.nostr.com", "wss://relay.nostr.net", "wss://relay.damus.io", "wss://nos.lol"] }];
  assert.deepEqual(defaultRelays(defs), ["wss://relay.nostr.com", "wss://relay.nostr.net", "wss://relay.damus.io", "wss://nos.lol"]);
  assert.deepEqual(defaultRelays(null), []);
  assert.deepEqual(defaultRelays([{ key: "sync.relays", default: "not a list" }]), []);
  assert.equal(isDefaultList(defaultRelays(defs), defaultRelays(defs)), true);
  assert.equal(isDefaultList(["wss://nos.lol"], defaultRelays(defs)), false);
  assert.equal(isDefaultList([...defaultRelays(defs)].reverse(), defaultRelays(defs)), false, "the order is part of the default");
});

test("a check's answer is drawn only while it is about the URL typed: one that lands after the field moved on says nothing", () => {
  const answered = { url: "wss://old.example", ok: true, latency_ms: 9 };
  assert.equal(checkAbout(answered, "wss://old.example"), answered);
  assert.equal(checkAbout(answered, "wss://new.example"), null, "reachable, said under an address nobody checked, would be a lie");
  assert.equal(checkAbout(answered, null), null, "the field holds no address to add");
  assert.equal(checkAbout(null, "wss://old.example"), null);
  const panel = readFileSync(new URL("./RelaysPanel.tsx", import.meta.url), "utf8");
  assert.ok(panel.includes("const checked = checkWords(checkAbout(check, addUrl));"), "the panel draws the answer through the rule");
});

test("the wire's readers read again when the relays move or a sync.* setting does — the switch among them — and all three ask the one rule", () => {
  assert.equal(wireMoved({ type: "relays_changed" }), true);
  assert.equal(wireMoved({ type: "settings_changed", scope: "machine", keys: ["sync.enabled"] }), true, "the switch turned on or off");
  assert.equal(wireMoved({ type: "settings_changed", scope: "machine", keys: ["editor.tab_size", "sync.relays"] }), true);
  assert.equal(wireMoved({ type: "settings_changed", scope: "machine", keys: ["lsp.enabled", "resync.thing"] }), false, "another setting");
  assert.equal(wireMoved({ type: "settings_changed" }), false, "a frame that names no key");
  assert.equal(wireMoved({ type: "people_changed" }), false);
  assert.equal(wireMoved(null), false);
  for (const [file, reload] of [["./RelaysPanel.tsx", "sync.reload();"], ["./PeoplePanel.tsx", "wire.reload();"], ["../../shell/NodeOverlay.tsx", "read.reload();"]]) {
    const src = readFileSync(new URL(file, import.meta.url), "utf8");
    assert.ok(src.includes("wireMoved(e.payload)") && src.includes(reload), `${file} reads the wire again by the rule`);
    assert.ok(!src.includes('startsWith("sync.")'), `${file} restates no part of it`);
  }
});
