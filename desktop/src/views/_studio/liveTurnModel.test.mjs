/**
 * The turns in flight: frames add up to a turn, a landed reply clears it,
 * a late reader is primed whole, a landed turn waits for its message; the
 * thinking block's words and the control's three modes. Run with `node --test desktop/src/views/_studio/liveTurnModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { NO_TURNS, THINKING_MODES, agentsOf, applyStreamed, clearScope, clearTurn, dropSettled, glimpse, landedOf, lengthWords, primeTurns, retireLanded, sameAgents, settleTurn, thinkingChoices, thinkingMode, thinkingOpen, thinkingTriggerWords, thinkingWords, turnsOf } from "./liveTurnModel.mjs";

test("frames add up to the agent's turn in the scope, the first one opening it; an empty frame changes nothing", () => {
  assert.equal(applyStreamed(NO_TURNS, { scope: "c1", agent: "general-agent", text: "", thinking: "" }), NO_TURNS, "nothing in it: the same map");
  assert.equal(applyStreamed(NO_TURNS, { scope: "", agent: "general-agent", text: "x" }), NO_TURNS, "no scope, no turn");
  let turns = applyStreamed(NO_TURNS, { scope: "c1", agent: "general-agent", text: "", thinking: "the palette " });
  turns = applyStreamed(turns, { scope: "c1", agent: "general-agent", text: "We ship ", thinking: "was chosen" });
  turns = applyStreamed(turns, { scope: "c1", agent: "general-agent", text: "the muted palette." });
  const [t] = turnsOf(turns, "c1");
  assert.equal(t.agent, "general-agent");
  assert.equal(t.text, "We ship the muted palette.");
  assert.equal(t.thinking, "the palette was chosen");
  assert.ok(t.since > 0, "a turn the bus opened is dated when its first frame came");
  assert.deepEqual(turnsOf(turns, "c2"), [], "another scope has none");
  assert.notEqual(turns, NO_TURNS, "a change is a new map");
});

test("two agents in one scope are two turns, oldest first; clearing one leaves the other, clearing the last removes the scope", () => {
  let turns = primeTurns(NO_TURNS, "c1", [
    { agent: "reviewer", text: "later", thinking: "", since: 20 },
    { agent: "general-agent", text: "first", thinking: "hm", since: 10 },
  ]);
  assert.deepEqual(
    turnsOf(turns, "c1").map((t) => t.agent),
    ["general-agent", "reviewer"],
  );
  const same = clearTurn(turns, "c1", "nobody");
  assert.equal(same, turns, "nothing to clear: the same map");
  turns = clearTurn(turns, "c1", "general-agent");
  assert.deepEqual(
    turnsOf(turns, "c1").map((t) => t.agent),
    ["reviewer"],
  );
  turns = clearTurn(turns, "c1", "reviewer");
  assert.equal(turns.has("c1"), false, "the last turn gone, the scope is gone");
  assert.equal(clearScope(turns, "c1"), turns, "clearing an absent scope is the same map");
});

test("priming stands in for the bus: the node's whole turns replace what the bus said, and an empty answer clears the scope", () => {
  let turns = applyStreamed(NO_TURNS, { scope: "c1", agent: "general-agent", text: "tail only" });
  turns = primeTurns(turns, "c1", [{ agent: "general-agent", text: "the whole thing, tail only", thinking: "all of it", since: 5 }]);
  assert.deepEqual(turnsOf(turns, "c1"), [{ agent: "general-agent", text: "the whole thing, tail only", thinking: "all of it", since: 5, working: null, landed: null }]);
  turns = applyStreamed(turns, { scope: "c1", agent: "general-agent", text: " and more" });
  assert.equal(turnsOf(turns, "c1")[0].text, "the whole thing, tail only and more", "frames after the prime append to it");
  assert.equal(primeTurns(turns, "c1", []).has("c1"), false, "nothing runs: the scope is cleared");
  assert.equal(clearScope(turns, "c1").has("c1"), false);
});

test("the thinking mode is auto, shown or hidden — auto before a choice and for a word off the list", () => {
  assert.deepEqual([...THINKING_MODES], ["auto", "shown", "hidden"]);
  assert.equal(thinkingMode("shown"), "shown");
  assert.equal(thinkingMode("hidden"), "hidden");
  assert.equal(thinkingMode("auto"), "auto");
  assert.equal(thinkingMode(null), "auto");
  assert.equal(thinkingMode("loud"), "auto");
});

test("a block is open by the mode's rule — auto only while live and before the words — and its own press wins until the mode changes", () => {
  assert.equal(thinkingOpen({ mode: "shown", live: false, writing: true }), true);
  assert.equal(thinkingOpen({ mode: "hidden", live: true, writing: false }), false);
  assert.equal(thinkingOpen({ mode: "auto", live: true, writing: false }), true, "only thinking so far: open");
  assert.equal(thinkingOpen({ mode: "auto", live: true, writing: true }), false, "the words began: folded");
  assert.equal(thinkingOpen({ mode: "auto", live: false, writing: true }), false, "a landed reply: folded");
  assert.equal(thinkingOpen({ mode: "hidden", own: true, live: false, writing: true }), true, "pressed open");
  assert.equal(thinkingOpen({ mode: "shown", own: false, live: true, writing: false }), false, "pressed closed");
});

test("the control offers the three modes in order with a meaning each, and the trigger names the one chosen", () => {
  const choices = thinkingChoices();
  assert.deepEqual(choices.map((c) => c.id), ["auto", "shown", "hidden"]);
  assert.ok(choices.every((c) => c.label && c.description.endsWith(".") && c.icon));
  assert.equal(thinkingTriggerWords("hidden").label, "Thinking: Hidden");
  assert.equal(thinkingTriggerWords("sideways").label, "Thinking: Auto", "a word off the list reads as auto");
  assert.equal(thinkingTriggerWords("shown").title, choices[1].description);
});

test("a landed turn is kept, frozen, until its message is in the page; no message clears it at once", () => {
  let turns = applyStreamed(NO_TURNS, { scope: "c1", agent: "general-agent", text: "Muted.", working: "Read src/app.ts" });
  turns = settleTurn(turns, "c1", "general-agent", "m-42");
  const [t] = turnsOf(turns, "c1");
  assert.equal(t.landed, "m-42");
  assert.equal(t.working, null, "nothing runs once the reply landed");
  assert.equal(t.text, "Muted.", "the words stay on screen");
  assert.equal(retireLanded(turns, new Set(["m-1"])), turns, "another message: the same map");
  assert.equal(retireLanded(turns, new Set(["m-42"])).has("c1"), false, "its message drawn: the row goes");
  assert.equal(settleTurn(turns, "c1", "nobody", "m-9"), turns, "no such turn: the same map");
  assert.equal(settleTurn(turns, "c1", "general-agent", null).has("c1"), false, "no message to wait for: cleared");
});

test("the tool line is set by the frame that says so, replaced not appended, and cleared by the one that says null", () => {
  let turns = applyStreamed(NO_TURNS, { scope: "c1", agent: "a", text: "", thinking: "", working: "Read src/app.ts" });
  assert.equal(turnsOf(turns, "c1")[0].working, "Read src/app.ts", "a tool alone opens the turn");
  const same = applyStreamed(turns, { scope: "c1", agent: "a", text: "", thinking: "", working: "Read src/app.ts" });
  assert.equal(same, turns, "the same line again is nothing new");
  turns = applyStreamed(turns, { scope: "c1", agent: "a", text: "so ", thinking: "" });
  assert.equal(turnsOf(turns, "c1")[0].working, "Read src/app.ts", "a frame that says nothing of tools keeps the line");
  turns = applyStreamed(turns, { scope: "c1", agent: "a", text: "", thinking: "", working: "Grep main" });
  assert.equal(turnsOf(turns, "c1")[0].working, "Grep main");
  turns = applyStreamed(turns, { scope: "c1", agent: "a", text: "far", thinking: "", working: null });
  assert.equal(turnsOf(turns, "c1")[0].working, null, "null: the tool ended");
  assert.equal(turnsOf(turns, "c1")[0].text, "so far");
  assert.deepEqual(agentsOf(turns, "c1"), ["a"]);
  assert.deepEqual(agentsOf(applyStreamed(NO_TURNS, { scope: "c2", agent: "b", working: null }), "c2"), [], "a turn opened by a null line has nothing to draw");
  assert.ok(sameAgents(["a", "b"], ["a", "b"]) && !sameAgents(["a"], ["b"]) && !sameAgents(["a"], ["a", "b"]));
});

test("a glimpse is the tail of the thinking on one line, never splitting a surrogate pair", () => {
  assert.equal(glimpse("short"), "short");
  assert.equal(glimpse("a\nb   c"), "a b c");
  const long = `${"x".repeat(100)}😀${"y".repeat(10)}`;
  const g = glimpse(long, 12);
  assert.ok(g.startsWith("…"), "cut: an ellipsis first");
  assert.ok(!/[\ud800-\udfff]/.test(g[1]) || g.codePointAt(1) > 0xffff, "the emoji is whole or gone, never half");
  assert.equal(glimpse("", 10), "");
});

test("a thinking's length reads in chars under a thousand and in k above", () => {
  assert.equal(lengthWords(0), "0 chars");
  assert.equal(lengthWords(1), "1 char");
  assert.equal(lengthWords(999), "999 chars");
  assert.equal(lengthWords(1234), "1.2k chars");
  assert.equal(lengthWords(12_345), "12k chars");
});

test("the block's header names the thinking, its length, and what a press does — with an ellipsis while it is still being written", () => {
  assert.deepEqual(thinkingWords(1234, false), { label: "Thinking", length: "1.2k chars", hint: "Show this thinking — 1.2k chars" });
  assert.equal(thinkingWords(40, true).hint, "Hide this thinking");
  assert.equal(thinkingWords(40, true, true).label, "Thinking…");
  assert.equal(thinkingWords(40, true, true, 4.7).label, "Thinking… 4s", "live, the header counts the seconds");
  assert.equal(thinkingWords(40, false, false, 9).label, "Thinking", "landed, no clock");
});

test("agent_replied names the last message of the reply, and posted: false means nothing landed — no id is waited for", () => {
  assert.equal(landedOf({ posted: true, message: "m-3" }), "m-3", "a reply in three messages names the last: the row waits for the whole reply");
  assert.equal(landedOf({ posted: false, message: null }), null);
  assert.equal(landedOf({ posted: false, message: "m-stale" }), null, "the node's word that nothing was written wins over an id beside it: a row waiting for it would wait for ever");
  assert.equal(landedOf({ posted: true, message: null }), null);
  assert.equal(landedOf({ posted: true, message: "" }), null);
  assert.equal(landedOf({ message: "m-1" }), "m-1", "a frame that does not say is taken at its message");
  assert.equal(landedOf(null), null);
});

test("a turn that settles where no timeline reads is cleared, and a scope's frozen rows go when its last reader leaves — the store is bounded by what is in flight and what is on screen", () => {
  let turns = applyStreamed(NO_TURNS, { scope: "c1", agent: "general-agent", text: "Done." });
  turns = applyStreamed(turns, { scope: "c2", agent: "reviewer", text: "Reading…" });
  // Nobody reads c1: its reply is in the page for whoever opens it; the row is not kept.
  const unwatched = settleTurn(turns, "c1", "general-agent", "m-1", false);
  assert.equal(unwatched.has("c1"), false);
  assert.equal(unwatched.get("c2")?.has("reviewer"), true, "another scope's turn is untouched");
  // Somebody reads c1: frozen until its message is drawn.
  const watched = settleTurn(turns, "c1", "general-agent", "m-1", true);
  assert.equal(watched.get("c1")?.get("general-agent")?.landed, "m-1");
  assert.equal(settleTurn(turns, "c1", "general-agent", "m-1").get("c1")?.get("general-agent")?.landed, "m-1", "watched unless said otherwise");
  // The reader leaves before the message is drawn: the frozen row goes, a turn still in flight stays.
  let both = applyStreamed(watched, { scope: "c1", agent: "reviewer", text: "Still going" });
  both = dropSettled(both, "c1");
  assert.deepEqual([...(both.get("c1")?.keys() ?? [])], ["reviewer"]);
  assert.equal(dropSettled(both, "c1"), both, "nothing settled: the same map");
  assert.equal(dropSettled(both, "nowhere"), both);
  assert.equal(dropSettled(watched, "c1").has("c1"), false, "the scope's only turn was settled: the scope goes with it");
});

