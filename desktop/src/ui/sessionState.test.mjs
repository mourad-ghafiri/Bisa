/**
 * The session vocabulary agrees with the engine and reads the same on every
 * surface. Run with `npm test` from `desktop/`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { STATES, attentionRank, counts, describe, iconOf, liveChildren, isAttention, isEnded, isLive, isNotifiable, isStoppable, label, loudest, sortRows, stateOf, toneOf, waitWords } from "./sessionState.mjs";

const here = dirname(fileURLToPath(import.meta.url));

test("the nine words are exactly the engine's SessionState variants", () => {
  const src = readFileSync(join(here, "../../../crates/bisa-engine/src/presence.rs"), "utf8");
  const body = src.slice(src.indexOf("pub enum SessionState {"), src.indexOf("pub enum WaitingOn {"));
  const variants = [...body.matchAll(/^\s{4}([A-Z][A-Za-z]+)\b/gm)].map((m) => m[1].replace(/([a-z])([A-Z])/g, "$1_$2").toLowerCase());
  assert.deepEqual([...variants].sort(), [...STATES].sort());
});

test("one order: what needs you, then what is happening — a tool before thinking — then what happened, then the rest", () => {
  assert.ok(attentionRank("waiting") < attentionRank("failed"));
  assert.ok(attentionRank("failed") < attentionRank("aborted"));
  assert.ok(attentionRank("aborted") < attentionRank("running"));
  assert.ok(attentionRank("running") < attentionRank("thinking"), "a tool has a name and arguments");
  assert.ok(attentionRank("thinking") < attentionRank("starting"));
  assert.ok(attentionRank("starting") < attentionRank("done"), "a loader beside a tick is the loader");
  assert.ok(attentionRank("done") < attentionRank("idle"));
  assert.ok(attentionRank("idle") < attentionRank("parked"));
  assert.equal(toneOf("waiting"), "accent", "the one state allowed to interrupt wears the accent");
  assert.equal(toneOf("failed"), "danger");
  assert.equal(toneOf("done"), "ok");
  assert.equal(toneOf("running"), "working");
  assert.equal(toneOf("parked"), "dim");
  assert.equal(describe({ state: "waiting", on: { on: "gate", gate: "approval", gate_id: "g" } }).rank, 1);
});

test("live, attention and ended partition the words the way the engine does", () => {
  for (const w of ["starting", "idle", "thinking", "running", "waiting"]) assert.ok(isLive(w), w);
  for (const w of ["done", "aborted", "failed", "parked"]) assert.ok(!isLive(w), w);
  for (const w of ["done", "aborted", "failed"]) assert.ok(isEnded(w), w);
  assert.ok(!isEnded("parked"), "parked is kept, not ended");
  // Stop is for a session doing something or blocked on a person — never one
  // idle between turns: alive, drawn live, and nothing running to end.
  for (const w of ["starting", "thinking", "running", "waiting"]) assert.ok(isStoppable(w), `${w} can be stopped`);
  for (const w of ["idle", "parked", "done", "aborted", "failed"]) assert.ok(!isStoppable(w), `${w} offers no Stop`);
  assert.ok(isStoppable({ state: "waiting", on: { on: "auth", provider: "x" } }), "an auth wait has no gate — stopping is the way out");
  assert.ok(isLive("idle") && !isStoppable("idle"), "live and stoppable are two rules");
  assert.ok(isAttention("waiting") && isAttention("failed") && !isAttention("aborted"));
  assert.equal(stateOf(undefined), "idle");
  assert.equal(stateOf("nonsense"), "idle");
});

test("labels say what a person asks: which tool, which question, which reason", () => {
  assert.equal(label({ state: "running", tool: "Edit", args: "" }), "running Edit");
  assert.equal(label({ state: "waiting", on: { on: "permission", tool: "Bash" } }), "waiting on you — permission: Bash");
  assert.equal(label({ state: "waiting", on: { on: "question", text: "Which tone?", gate_id: "g" } }), "waiting on you — Which tone?");
  assert.equal(label({ state: "waiting", on: { on: "auth", provider: "github" } }), "waiting on you — sign in to github");
  assert.equal(label({ state: "failed", reason: "boom" }), "failed: boom");
  assert.equal(label({ state: "idle" }), "idle");
  assert.equal(label("thinking"), "thinking");
});

test("a child speaks only to ask: the loudest state speaks for the set; sort is by the one order then recency", () => {
  const rows = [
    { state: { state: "idle" }, since: 5, children: [{ state: { state: "waiting", on: { on: "permission", tool: "Bash" } } }] },
    { state: { state: "running", tool: "Read" }, since: 9 },
    { state: { state: "done" }, since: 1 },
    { state: { state: "failed", reason: "x" }, since: 2 },
  ];
  assert.deepEqual(counts(rows), { waiting: 1, working: 1, done: 1, failed: 1, live: 2 });
  assert.equal(loudest(rows), "waiting", "a sub-agent waiting is the session waiting");
  assert.deepEqual(sortRows(rows).map((r) => r.since), [2, 9, 1, 5], "failed, running, done, then the idle parent");
  assert.equal(loudest([]), "idle");
  // A working harness whose sub-agent finished, or works, is a working harness.
  const delegating = [{ state: { state: "running", tool: "sub-agent", args: "explore" }, children: [{ state: { state: "done" } }, { state: { state: "running", tool: "Grep" } }] }];
  assert.equal(loudest(delegating), "running", "a done or working child never speaks for its parent");
  assert.deepEqual(counts(delegating), { waiting: 0, working: 1, done: 0, failed: 0, live: 1 }, "children are not counted as sessions");
  const thinker = [{ state: { state: "thinking" }, children: [{ state: { state: "done" } }] }];
  assert.equal(loudest(thinker), "thinking");
  assert.equal(loudest([{ state: { state: "thinking" }, children: [{ state: { state: "failed", reason: "r" } }] }]), "failed", "a failed sub-agent is the session's failure");
});

test("only edges into waiting, failed or done are moments worth a notification — which of them the person wants is the notifications model's", () => {
  assert.ok(isNotifiable("thinking", { state: "waiting", on: { on: "gate", gate: "approval", gate_id: "g" } }));
  assert.ok(!isNotifiable("waiting", "waiting"), "still waiting is not news twice");
  assert.ok(isNotifiable("running", { state: "failed", reason: "x" }));
  assert.ok(isNotifiable("running", "done"), "done is a moment; whether it is wanted is a switch, not this rule");
  assert.ok(!isNotifiable("running", "thinking"), "an edge into work is not news");
  assert.ok(!isNotifiable(null, "thinking"), "a session appearing is not news");
});

test("every state's glyph is a key of the one glyph map, so SessionMark draws each without a fallback", () => {
  const icons = readFileSync(join(here, "./icons.ts"), "utf8");
  const start = icons.indexOf("export const ICON = {");
  const block = icons.slice(start, icons.indexOf("\n};", start));
  for (const word of STATES) {
    const key = iconOf(word).replace(/^icon:/, "");
    assert.ok(new RegExp(`^\\s+${key}:`, "m").test(block), `${word} names ICON.${key}`);
  }
  assert.notEqual(iconOf("thinking"), iconOf("running"), "thinking and running are two marks");
  assert.notEqual(iconOf("idle"), iconOf("parked"), "idle and parked are two marks");
});

test("a sub-agent is shown while live or failed, never once finished, and never under a parent that ended", () => {
  const child = (state, id) => ({ id, state });
  const parent = (state, children) => ({ state, children });
  const live = parent({ state: "running", tool: "sub-agent", args: "a, b" }, [child({ state: "thinking" }, "a"), child({ state: "done" }, "b"), child({ state: "failed", reason: "r" }, "c"), child({ state: "aborted" }, "d")]);
  assert.deepEqual(
    liveChildren(live).map((c) => c.id),
    ["a", "c"],
    "the live one and the failed one — a done or aborted child has left",
  );
  assert.deepEqual(liveChildren(parent({ state: "done" }, [child({ state: "thinking" }, "a")])), [], "a parent that ended took its children with it");
  assert.deepEqual(liveChildren(parent({ state: "idle" }, [])), []);
  assert.deepEqual(liveChildren(null), []);
  // The parent's word follows the same rule: a failed child speaks for a live parent, not for one that ended.
  assert.equal(loudest([parent({ state: "thinking" }, [child({ state: "failed", reason: "r" }, "c")])]), "failed");
  assert.equal(loudest([parent({ state: "done" }, [child({ state: "failed", reason: "r" }, "c")])]), "done", "an ended parent's stale child says nothing");
  assert.deepEqual(counts([parent({ state: "done" }, [child({ state: "waiting", on: { on: "permission", tool: "Bash", gate_id: "g" } }, "w")])]), { waiting: 0, working: 0, done: 1, failed: 0, live: 0 });
});

test("a wait has words of its own, with no *waiting on you* before them, and the label says both", () => {
  const waits = [
    [{ on: "permission", tool: "Bash" }, "permission: Bash"],
    [{ on: "question", text: "Which database?" }, "Which database?"],
    [{ on: "gate", gate: "approval" }, "approval gate"],
    [{ on: "auth", provider: "GitHub" }, "sign in to GitHub"],
  ];
  for (const [on, words] of waits) {
    assert.equal(waitWords({ state: "waiting", on }), words);
    assert.equal(label({ state: "waiting", on }), `waiting on you — ${words}`, "the row's sentence is the same wait, said whole");
  }
  assert.equal(waitWords({ state: "waiting" }), null, "a wait that says no more than that it waits");
  assert.equal(waitWords({ state: "waiting", on: { on: "invented" } }), null);
  assert.equal(waitWords({ state: "waiting", on: { on: "question", text: "" } }), null);
  for (const other of [null, undefined, "waiting", { state: "running", tool: "Edit" }, { state: "failed", reason: "x" }]) assert.equal(waitWords(other), null);
});
