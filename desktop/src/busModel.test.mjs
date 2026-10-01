/**
 * The bus's facts: the filter, the frame, the wait, the warning. Run with
 * `node --test desktop/src/busModel.test.mjs`.
 */
import { readFileSync } from "node:fs";
import { test } from "node:test";
import assert from "node:assert/strict";
import { BASE_RETRY_MS, FIRST_CONN, MALFORMED_WARN_WINDOW_MS, MAX_RETRY_MS, connAfter, connected, endLine, matches, parseFrame, retryDelay, warnsMalformed } from "./busModel.mjs";

const engine = (goal) => ({ stream: "engine", payload: { type: "run_changed", goal } });
const conversation = (scope) => ({ stream: "conversation", payload: { scope } });
const inbox = (key) => ({ stream: "inbox", payload: { key } });

test("a subscriber hears the frames it asked for: by stream, by goal, by scope, by inbox key — and the key never aliases the scope", () => {
  assert.ok(matches({}, engine("g1")) && matches({}, inbox("k1")), "no filter: every frame");
  assert.ok(matches({ stream: "engine" }, engine("g1")) && !matches({ stream: "engine" }, inbox("g1")));
  assert.ok(matches({ goal: "g1" }, engine("g1")) && !matches({ goal: "g1" }, engine("g2")));
  assert.ok(!matches({ goal: "g1" }, conversation("g1")), "a goal filter hears engine frames alone");
  assert.ok(matches({ scope: "c1" }, conversation("c1")) && !matches({ scope: "c1" }, inbox("c1")), "a conversation's scope is not an inbox row's key");
  assert.ok(matches({ key: "c1" }, inbox("c1")) && !matches({ key: "c1" }, conversation("c1")), "and the other way round");
  assert.ok(!matches({ stream: "inbox", key: "c1" }, inbox("c2")));
  assert.ok(!matches({ goal: "g1" }, { stream: "engine" }), "a frame without the field is not a match");
});

test("a frame is a JSON object with a stream word; anything else is not one, and never throws", () => {
  assert.deepEqual(parseFrame('{"stream":"engine","payload":{"type":"x"}}'), { stream: "engine", payload: { type: "x" } });
  for (const bad of ["", "not json", "[1,2]", "null", "42", '"words"', '{"payload":{}}', '{"stream":""}', '{"stream":7}', undefined, null, 5, {}]) {
    assert.equal(parseFrame(bad), null, `${JSON.stringify(bad)} is not a frame`);
  }
});

test("the wait doubles from the base to the cap, spread by a quarter either way, and a huge attempt count does not overflow", () => {
  assert.equal(retryDelay(0), BASE_RETRY_MS);
  assert.equal(retryDelay(1), 1000);
  assert.equal(retryDelay(2), 2000);
  assert.equal(retryDelay(5), MAX_RETRY_MS, "capped");
  assert.equal(retryDelay(1000), MAX_RETRY_MS, "and no further");
  assert.equal(retryDelay(0, 0), 375, "the low draw");
  assert.equal(retryDelay(0, 0.999), 625, "the high draw, inside the spread");
  assert.equal(retryDelay(3, 1.5), 4000 * 1.25, "a draw out of range is clamped");
  assert.equal(retryDelay(-1), BASE_RETRY_MS, "a negative attempt is the first");
  assert.equal(retryDelay(Number.NaN, Number.NaN), BASE_RETRY_MS, "nothing throws");
  for (let n = 0; n < 8; n++) assert.ok(retryDelay(n, 0) <= retryDelay(n, 1) && retryDelay(n, 1) <= MAX_RETRY_MS * 1.25);
});

test("a frame that could not be read is one line the first time and one per window after", () => {
  assert.equal(warnsMalformed(null, 1000), true);
  assert.equal(warnsMalformed(1000, 1000 + MALFORMED_WARN_WINDOW_MS - 1), false);
  assert.equal(warnsMalformed(1000, 1000 + MALFORMED_WARN_WINDOW_MS), true);
});

test("the connection's word: connecting before anything answered, open, and closed for as long as the node is away", () => {
  assert.equal(FIRST_CONN, "connecting", "nothing was tried: the node is not known to be away");
  assert.equal(connAfter(FIRST_CONN, "attempt"), "connecting", "the first attempt is the one that connects");
  assert.equal(connAfter("connecting", "opened"), "open");
  assert.equal(connAfter("open", "ended"), "closed", "the stream ended: the node went away");
  assert.equal(connAfter("closed", "attempt"), "closed", "asking again is not news: the node is away until the stream is open");
  assert.equal(connAfter("closed", "ended"), "closed", "an attempt that found nobody");
  assert.equal(connAfter("closed", "opened"), "open", "and back");
  assert.equal(connAfter("open", "idle"), "closed", "nobody listens: whoever listens next missed what was said");
  assert.equal(connAfter("open", "attempt"), "open");
  for (const conn of ["connecting", "open", "closed", "lagged"]) assert.equal(connAfter(conn, "invented"), conn, "an event nobody knows changes nothing");
});

test("a node that stays away reads closed at every moment of every retry — never connecting again", () => {
  let conn = connAfter(connAfter(FIRST_CONN, "attempt"), "opened");
  conn = connAfter(conn, "ended");
  const seen = [];
  for (let attempt = 0; attempt < 6; attempt++) {
    conn = connAfter(conn, "attempt");
    seen.push(conn);
    conn = connAfter(conn, "ended");
    seen.push(conn);
  }
  assert.deepEqual([...new Set(seen)], ["closed"]);
  assert.equal(connAfter(connAfter(conn, "attempt"), "opened"), "open");
});

test("a stream the node ended is the node going away: one line at info, a first miss a warning, the rest debug — never an error", () => {
  assert.deepEqual(endLine(true, 0), { level: "info", message: "the event stream ended: the node went away" });
  assert.equal(endLine(true, 7).level, "info", "an open stream that ends is said whatever came before");
  assert.deepEqual(endLine(false, 0), { level: "warn", message: "the event stream could not be opened: the node is away" });
  for (const attempt of [1, 2, 40]) assert.equal(endLine(false, attempt).level, "debug", `attempt ${attempt}: a node that stays down does not fill the log`);
  assert.equal(endLine(false, Number.NaN).level, "warn", "a count nobody can read is the first");
  for (const line of [endLine(true, 0), endLine(false, 0), endLine(false, 3)]) assert.notEqual(line.level, "error");
});

test("the bus takes its words from the model: every end closes, waits and asks again, and an attempt that throws is an end too", () => {
  const bus = readFileSync(new URL("./bus.ts", import.meta.url), "utf8");
  assert.ok(bus.includes("let state: ConnState = FIRST_CONN"), "the first word is the model's");
  assert.ok(!/setState\("(connecting|closed|open)"\)/.test(bus.replace(/setState\("open"\);\n    \}\n    return;/, "")), "no word is spelt by the bus but the lagged pulse's two");
  const onerror = bus.slice(bus.indexOf("es.onerror = () => {"), bus.indexOf("})()"));
  assert.ok(onerror.includes("drop(es)") && onerror.includes("ended(wasOpen)"), `the stream's end: ${onerror}`);
  const ended = bus.slice(bus.indexOf("function ended("), bus.indexOf("function dispatch("));
  assert.ok(ended.includes("endLine(wasOpen, attempt)") && ended.includes('happened("ended")') && ended.includes("scheduleReconnect()"), "one line, the word, the next attempt");
  assert.ok(!/log\.error/.test(ended), "never an error");
  assert.ok(/\.catch\(\(e: unknown\) => \{[\s\S]*?ended\(false, e\)/.test(bus), "an attempt that threw before it had a stream asks again");
});

test("the node is there while the stream is open — a lagged pulse is an open stream, never a node that went away", () => {
  assert.ok(connected("open") && connected("lagged"));
  for (const conn of ["connecting", "closed", "", undefined, null]) assert.equal(connected(conn), false, `${JSON.stringify(conn)}`);
});
