/**
 * A start step as the designer edits it, and the workflow's ways in: a
 * fresh event writes only the fields the core's table lists, a start by hand
 * maps and guards nothing, the mapping and the guard write what the core
 * writes, the phrases, a hook's call, a test run's sample, and the entry
 * rules — starts, the manual entry, what turning On asks. Run
 * with `node --test --import ./src/i18n/preload.mjs desktop/src/views/_workflow/forms/startForm.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import {
  DEFAULT_CRON,
  DEFAULT_EVERY_SECS,
  DEFAULT_POLL_SECS,
  MANUAL,
  PAYLOAD_FIELDS,
  blankStartOn,
  cadenceOf,
  eventOf,
  eventPhrase,
  eventStarts,
  guardOf,
  guardWords,
  inputPlaceholders,
  listeningInputs,
  listeningNeeds,
  startInputs,
  listens,
  localHookPath,
  manualEntry,
  mappingRows,
  mappingSuggestions,
  payloadTemplate,
  samplePayload,
  setCadence,
  setGuard,
  setMapping,
  setStartEvent,
  startInputRefs,
  startSteps,
  startTemplates,
  strayMappings,
  validSignalName,
} from "./startForm.mjs";
import { START_EVENTS } from "../stepKinds.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const CORE = join(HERE, "../../../../../crates/bisa-core/src");
const source = (file) => readFileSync(join(CORE, file), "utf8");

/** A `FIELDS` constant of a filter or a schedule, read from the core. */
function constFields(name) {
  const src = source("start.rs") + source("listen.rs");
  const at = src.indexOf(`impl ${name} {`);
  assert.ok(at >= 0, `${name} has its impl`);
  const m = src.slice(at).match(/pub const FIELDS: &'static \[&'static str\] = &\[([^\]]*)\]/);
  assert.ok(m, `${name} names its fields`);
  return [...m[1].matchAll(/"([a-z_]+)"/g)].map((x) => x[1]);
}

/** `StartOn::fields_of`, tag by tag. */
function startFields() {
  const src = source("start.rs");
  const at = src.indexOf("pub fn fields_of(tag: &str)");
  assert.ok(at >= 0, "the core lists each event's fields");
  const body = src.slice(at, src.indexOf("_ => return None", at));
  const out = {};
  for (const m of body.matchAll(/"([a-z_]+)" => (&\[[^\]]*\]|[A-Z][A-Za-z]*::FIELDS)/g)) {
    out[m[1]] = m[2].startsWith("&") ? [...m[2].matchAll(/"([a-z_]+)"/g)].map((x) => x[1]) : constFields(m[2].split("::")[0]);
  }
  return out;
}

const start = (id, on, extra = {}) => ({ id, name: id, kind: "start", on, then: [], ...extra });
const agent = (id, then = []) => ({ id, name: id, kind: "agent", instructions: id, then });
const input = (name, over = {}) => ({ name, label: name.toUpperCase(), kind: "text", required: true, ...over });

test("a fresh event of every kind writes only the fields the core's table lists, and chooses nothing for the person", () => {
  const fields = startFields();
  assert.deepEqual(Object.keys(fields).sort(), START_EVENTS.map((e) => e.event).sort(), "the table and the designer name the same events");
  for (const { event } of START_EVENTS) {
    const on = blankStartOn(event);
    assert.equal(on.event, event);
    for (const key of Object.keys(on)) assert.ok(key === "event" || fields[event].includes(key), `${event} writes \`${key}\`, which the core does not list`);
    // A reference not chosen is null — `""` is an id the node cannot parse;
    // a name or a command not written yet is `""`, the validator's to name.
    for (const key of ["project", "connector", "operation", "account", "workflow"]) {
      if (key in on) assert.notEqual(on[key], "", `${event}.${key} is not a blank reference`);
    }
  }
  assert.deepEqual(blankStartOn("manual"), { event: "manual" });
  assert.deepEqual(blankStartOn("schedule"), { event: "schedule", every: DEFAULT_EVERY_SECS });
  assert.deepEqual(blankStartOn("project"), { event: "project", project: null, change: "commit" });
  assert.equal(blankStartOn("connector").connector, null);
  assert.equal(blankStartOn("connector").operation, null);
  assert.equal(blankStartOn("connector").every, DEFAULT_POLL_SECS, "a poll keeps its own cadence");
  assert.equal(blankStartOn("check").every, DEFAULT_POLL_SECS);
  assert.equal(blankStartOn("check").fire_on, "starts_failing", "the alert, once per outage");
  assert.throws(() => blankStartOn("webhook"), /not a start event/);
});

test("a start's event changes whole; a start by hand keeps no mapping and no guard", () => {
  const hook = start("ticket", { event: "hook" }, { inputs: { ticket: "{event.payload.body}" }, guard: { overlap: "skip" } });
  assert.equal(eventOf(hook), "hook");
  const scheduled = setStartEvent(hook, "schedule");
  assert.deepEqual(scheduled.on, { event: "schedule", every: DEFAULT_EVERY_SECS });
  assert.deepEqual(scheduled.inputs, hook.inputs, "an event start keeps its mapping");
  const byHand = setStartEvent(hook, MANUAL);
  assert.deepEqual(byHand.on, { event: "manual" });
  assert.equal("inputs" in byHand, false, "a start by hand maps nothing");
  assert.equal("guard" in byHand, false, "and guards nothing");
  assert.equal(setStartEvent(hook, "hook"), hook, "the same event is no edit");
  assert.equal(eventOf({ kind: "start" }), MANUAL);
});

test("a cadence is seconds or a cron, never both", () => {
  const every = { event: "schedule", every: 60 };
  assert.equal(cadenceOf(every), "every");
  const cron = setCadence(every, "cron");
  assert.deepEqual(cron, { event: "schedule", cron: DEFAULT_CRON, tz: null });
  assert.equal(cadenceOf(cron), "cron");
  assert.deepEqual(setCadence(cron, "every"), { event: "schedule", every: DEFAULT_EVERY_SECS });
  assert.deepEqual(setCadence({ event: "check", command: "true", fire_on: "failing", every: 5 }, "cron"), { event: "check", command: "true", fire_on: "failing", cron: DEFAULT_CRON, tz: null }, "a check keeps its own fields");
});

test("the mapping: one row per input, a blank template unmaps, an empty mapping is none, and a stray one is shown", () => {
  const inputs = [input("ticket"), input("customer", { required: false }), input("tone", { default: "warm" })];
  let s = start("ticket", { event: "hook" });
  assert.deepEqual(mappingRows(s, inputs).map((r) => [r.input, r.required, r.template]), [
    ["ticket", true, null],
    ["customer", false, null],
    ["tone", false, null],
  ]);
  s = setMapping(s, "ticket", "{event.payload.body}");
  s = setMapping(s, "customer", "{event.payload.customer}");
  assert.deepEqual(s.inputs, { ticket: "{event.payload.body}", customer: "{event.payload.customer}" });
  assert.equal(mappingRows(s, inputs)[0].template, "{event.payload.body}");
  s = setMapping(s, "customer", "  ");
  assert.deepEqual(s.inputs, { ticket: "{event.payload.body}" });
  s = setMapping(s, "ticket", null);
  assert.equal("inputs" in s, false, "an empty mapping is no mapping");
  assert.deepEqual(strayMappings(start("x", { event: "hook" }, { inputs: { gone: "{event.at}", ticket: "{event.at}" } }), inputs), ["gone"]);
  assert.deepEqual(mappingSuggestions("message").slice(0, 2), ["{event.payload.message}", "{event.payload.scope}"], "in the order the guide's table lists them");
  assert.ok(mappingSuggestions("message").includes("{event.payload.text}"));
  assert.equal(payloadTemplate("body.customer"), "{event.payload.body.customer}", "the event is read under its own root, and nowhere but here");
  assert.deepEqual(mappingSuggestions("hook"), [], "a hook's body is the caller's shape");
  for (const { event } of START_EVENTS) assert.ok(Array.isArray(PAYLOAD_FIELDS[event]), `${event} says what it carries`);
});

test("the guard writes what the core writes: the defaults unwritten, several at once as { parallel: n }", () => {
  const s = start("ticket", { event: "hook" });
  assert.deepEqual(guardOf(s), { overlap: "queue", max: 2, debounce: 0 });
  assert.equal("guard" in setGuard(s, { overlap: "queue", max: 2, debounce: 0 }), false, "a default guard is no guard");
  assert.deepEqual(setGuard(s, { overlap: "skip", max: 2, debounce: 0 }).guard, { overlap: "skip" });
  assert.deepEqual(setGuard(s, { overlap: "parallel", max: 4, debounce: 0 }).guard, { overlap: { parallel: 4 } });
  assert.deepEqual(setGuard(s, { overlap: "parallel", max: 0, debounce: 0 }).guard, { overlap: { parallel: 1 } }, "at least one at a time");
  assert.deepEqual(setGuard(s, { overlap: "queue", max: 2, debounce: 60 }).guard, { debounce_secs: 60 });
  assert.deepEqual(guardOf(start("t", { event: "hook" }, { guard: { overlap: { parallel: 3 }, debounce_secs: 30 } })), { overlap: "parallel", max: 3, debounce: 30 });
  assert.equal(guardWords(s), "One at a time: a later occurrence waits for the run before it.");
  assert.equal(guardWords(start("t", { event: "hook" }, { guard: { overlap: "skip" } })), "Skipped while a run it started is still going — and it says so.");
  assert.equal(guardWords(start("t", { event: "hook" }, { guard: { overlap: { parallel: 3 } } })), "Up to 3 runs at once; the rest wait.");
  assert.equal(guardWords(start("t", { event: "hook" }, { guard: { debounce_secs: 120 } })), "One at a time: a later occurrence waits for the run before it. One within 2m of the last run it started is dropped.");
});

test("an event in a few words, for a card and a switch", () => {
  assert.equal(eventPhrase({ event: "manual" }), "by hand");
  assert.equal(eventPhrase({ event: "schedule", every: 3600 }), "every 1h");
  assert.equal(eventPhrase({ event: "schedule", every: { input: "interval" } }), "every {inputs.interval}");
  assert.equal(eventPhrase({ event: "schedule", cron: "0 9 * * 1", tz: "Europe/Paris" }), "on the schedule 0 9 * * 1");
  assert.equal(eventPhrase({ event: "schedule" }), "on a schedule (not set yet)");
  assert.equal(eventPhrase({ event: "hook" }), "when called");
  assert.equal(eventPhrase({ event: "hook", public: true }), "when called from outside");
  assert.equal(eventPhrase({ event: "message", in: "support" }), "a message in support");
  assert.equal(eventPhrase({ event: "signal", name: "report.ready" }), "the signal report.ready");
  assert.equal(eventPhrase({ event: "project", project: null, change: "pull_request" }), "a pull request's change");
  assert.equal(eventPhrase({ event: "run", outcome: "failed" }), "a run that fails");
  assert.equal(eventPhrase({ event: "run" }), "a run that ends");
  assert.equal(eventPhrase({ event: "platform", topic: "goal.closed" }), "the platform event goal.closed");
  assert.equal(eventPhrase({ event: "connector", connector: "github", operation: "list-issues" }), "each new item from github.list-issues");
  assert.equal(eventPhrase({ event: "check", command: "true" }), "a check that starts failing");
  assert.equal(eventPhrase(null), "by hand");
});

test("a hook's local call is the host's own route", () => {
  assert.equal(localHookPath({ workflow: "01WF" }, "ticket"), "/workflows/01WF/hooks/ticket");
  assert.equal(localHookPath({ goal: "01G" }, "ticket"), "/goals/01G/hooks/ticket");
});

test("a test run's sample reads as an occurrence its start would hear", () => {
  assert.equal(samplePayload({ event: "manual" }), null, "a start by hand is never a test run's");
  assert.deepEqual(samplePayload({ event: "schedule", every: 60 }, 1000), { at: 1000 });
  assert.deepEqual(samplePayload({ event: "signal", name: "deploy.finished", fields: { env: "prod" } }), { env: "prod" }, "its exact fields, already there");
  assert.equal(samplePayload({ event: "platform", topic: "goal.closed", fields: { reason: "done" } }).event, "goal.closed");
  assert.equal(samplePayload({ event: "run", outcome: "failed" }).outcome, "failed");
  assert.equal(samplePayload({ event: "run" }).outcome, "failed", "a run that ends, tried as one that failed");
  assert.equal(samplePayload({ event: "check", command: "curl -f x" }).exit_code, 1, "a check that fails");
  const heard = samplePayload({ event: "message", in: "support", contains: "refund" });
  assert.deepEqual([heard.text, heard.author_kind, heard.scope], ["refund", "you", "support"]);
});

test("what an event reads of the inputs it listens with: a reference, or a placeholder — a doubled brace is text", () => {
  assert.deepEqual(inputPlaceholders("{inputs.a} {{inputs.b}} {inputs.c.d} {inputs.a}"), ["a", "c"]);
  assert.deepEqual(inputPlaceholders(null), []);
  assert.deepEqual(startInputRefs({ event: "check", command: "sh -c {inputs.command}", every: { input: "interval" }, project: { input: "repo" } }).sort(), ["interval", "repo"]);
  assert.deepEqual(startInputRefs({ event: "message", from: { input: "who" }, mentions: { agent: "sre" } }), ["who"]);
  assert.deepEqual(startTemplates({ event: "check", command: "sh -c {inputs.command}" }), ["sh -c {inputs.command}"]);
  assert.deepEqual(startTemplates({ event: "signal", name: "{inputs.topic}", fields: { env: "prod" } }), ["{inputs.topic}", "prod"]);
  assert.deepEqual(startTemplates({ event: "hook" }), []);
});

test("the ways in: the starts, the one by hand — none when only events begin it — and what turning On asks: the core's rules", () => {
  const inputs = [input("ticket"), input("customer", { default: "anon" }), input("command"), input("interval", { kind: "number" })];
  const both = {
    inputs,
    steps: [
      start("by-hand", { event: "manual" }, { then: [{ to: "work" }] }),
      start("ticket", { event: "hook" }, { inputs: { ticket: "{event.payload.body}" }, then: [{ to: "work" }] }),
      start("probe", { event: "check", command: "sh -c {inputs.command}", every: { input: "interval" } }, { then: [{ to: "work" }] }),
      agent("work"),
    ],
  };
  assert.deepEqual(startSteps(both).map((s) => s.id), ["by-hand", "ticket", "probe"]);
  assert.equal(manualEntry(both).id, "by-hand");
  assert.deepEqual(eventStarts(both).map((s) => s.id), ["ticket", "probe"]);
  assert.equal(listens(both), true);
  assert.deepEqual(listeningNeeds(both), ["ticket", "command", "interval"], "the union: the check start maps no ticket, and reads the command and the interval");

  const hookOnly = { inputs: [], steps: [start("ticket", { event: "hook" }, { then: [{ to: "work" }] }), agent("work")] };
  assert.equal(manualEntry(hookOnly), null, "only events begin it: no run by hand but a test");
  assert.deepEqual(listeningNeeds(hookOnly), []);

  const bare = { steps: [agent("a", [{ to: "b" }]), agent("b")] };
  assert.deepEqual(startSteps(bare).map((s) => s.id), ["a"], "a workflow with no start begins at its root");
  assert.equal(manualEntry(bare).id, "a");
  assert.equal(listens(bare), false);
  assert.equal(manualEntry({ steps: [agent("a"), agent("b")] }), null, "two roots: no one way in by hand");
  assert.deepEqual(startSteps({ steps: [agent("a", [{ to: "b" }]), agent("b", [{ to: "a" }])] }).map((s) => s.id), ["a"], "a ring begins at its first step");
  assert.deepEqual(startSteps(null), []);
});

test("a signal's name is dotted lowercase words, as the core holds it", () => {
  for (const good of ["report.ready", "deploy-finished", "a2a.task", "x"]) assert.ok(validSignalName(good), good);
  for (const bad of ["", "Report", "a..b", ".a", "a b", "a/b", "x".repeat(129), null]) assert.ok(!validSignalName(bad), String(bad));
});

/** What each event's occurrence carries, as the guide's table promises it (`docs/guide/events.md` §Start events). */
function promisedPayloads() {
  const guide = readFileSync(join(HERE, "../../../../../docs/guide/events.md"), "utf8");
  const head = guide.indexOf("| `event` | Begins a run when | Fields | The event's payload |");
  assert.ok(head >= 0, "the guide's table of start events");
  const out = {};
  for (const line of guide.slice(head).split("\n").slice(2)) {
    if (!line.startsWith("|")) break;
    const cells = line.split("|").map((c) => c.trim());
    const event = cells[1].replace(/`/g, "");
    const shape = cells[4].match(/^`\{([^}]*)\}`$/);
    // A hook's body and a signal's payload are the caller's own shape; a start by hand carries nothing.
    out[event] = shape ? shape[1].split(",").map((f) => f.trim()).filter(Boolean) : [];
  }
  return out;
}

/** The keys of the payload the engine builds, read from its source: the `json!({ … })` block that holds `marker`. */
function enginePayload(file, marker) {
  const src = readFileSync(join(HERE, "../../../../../crates/bisa-engine/src/listen", file), "utf8");
  const at = src.indexOf(marker);
  assert.ok(at >= 0, `${file} builds a payload with ${marker}`);
  const open = src.lastIndexOf("json!({", at);
  const block = src.slice(open, src.indexOf("})", at));
  return [...block.matchAll(/"([a-z_]+)":/g)].map((m) => m[1]);
}

test("what a mapping may read of each event is what its occurrence carries — the guide's table, and the engine's own payloads", () => {
  const promised = promisedPayloads();
  assert.deepEqual(Object.keys(promised).sort(), START_EVENTS.map((e) => e.event).sort(), "the guide names every start event");
  for (const { event } of START_EVENTS) {
    assert.deepEqual([...PAYLOAD_FIELDS[event]].sort(), [...promised[event]].sort(), `${event}: the fields the form offers are the ones the payload has`);
    assert.deepEqual([...PAYLOAD_FIELDS[event]], promised[event], `${event}: in the table's order`);
  }
  // The engine's own words, where a payload is built in one place.
  assert.deepEqual(enginePayload("ear.rs", '"author_kind": author_kind'), PAYLOAD_FIELDS.message);
  assert.deepEqual(enginePayload("ear.rs", '"fields": event.payload.fields()'), PAYLOAD_FIELDS.platform);
  assert.deepEqual(enginePayload("ear.rs", '"outcome": outcome'), PAYLOAD_FIELDS.run);
  assert.deepEqual(enginePayload("sources.rs", '"exit_code": code'), PAYLOAD_FIELDS.check);
  assert.ok(!PAYLOAD_FIELDS.message.includes("at"), "a message's moment is the event's own — {event.at} — never a field of its payload");
  assert.deepEqual(mappingSuggestions("check"), ["{event.payload.exit_code}", "{event.payload.passed}", "{event.payload.output}", "{event.payload.command}"]);
});

test("a test run's sample carries every field a mapping may read, so a mapping picked from the list finds its value", () => {
  for (const { event } of START_EVENTS) {
    if (event === "manual") continue;
    const sample = samplePayload(blankStartOn(event), 1000);
    for (const field of PAYLOAD_FIELDS[event]) assert.ok(field in sample, `${event}: the sample carries \`${field}\``);
  }
  assert.deepEqual(samplePayload({ event: "check", command: "curl -f x" }), { exit_code: 1, passed: false, output: "", command: "curl -f x" }, "a check that fails");
  assert.deepEqual(samplePayload({ event: "message", in: "support", contains: "refund", from: "agents" }), { message: "", scope: "support", author: "", author_kind: "agent", teams: [], mentions: [], text: "refund" });
  assert.deepEqual(samplePayload({ event: "run", workflow: "01WF", outcome: "done" }), { run: "", workflow: "01WF", outcome: "done", goal: null });
  assert.deepEqual(samplePayload({ event: "platform", topic: "goal.closed", fields: { reason: "done" } }), { event: "goal.closed", fields: { reason: "done" }, goal: null, workflow: null, run: null });
  assert.deepEqual(samplePayload({ event: "project", project: "01P", change: "push", branch: "main" }), { project: "01P", slug: "", change: "push", branch: "main", before: "", after: "", forced: false, commits: [], paths: [], pull_request: null });
});

test("what a host is asked when it begins listening: what its events do not supply, each required, and never an input a default fills", () => {
  const inputs = [
    { name: "audience", label: "Audience", kind: "text", required: true },
    { name: "when", label: "When", kind: "text", required: false },
    { name: "channel", label: "Channel", kind: "text", required: true, default: "general" },
    { name: "tone", label: "Tone", kind: "text", required: false },
  ];
  const wf = { inputs, steps: [start("by-hand", { event: "manual" }, { then: [{ to: "work" }] }), start("weekly", { event: "schedule", cron: { input: "when" }, tz: "UTC" }, { then: [{ to: "work" }] }), agent("work")] };
  assert.deepEqual(listeningNeeds(wf), ["audience", "when"]);
  // `when` is optional for a run by hand, and the schedule cannot be armed without it: listening asks it as required.
  assert.deepEqual(listeningInputs(wf).map((d) => [d.name, d.required]), [["audience", true], ["when", true]]);
  assert.equal(inputs[1].required, false, "the definition is left as it was");
  // The node's word, when a row carries it — and an input a default fills is not asked even were it named.
  assert.deepEqual(listeningInputs(wf, ["when", "channel", "gone"]).map((d) => d.name), ["when"]);
  assert.deepEqual(listeningInputs(wf, []), []);
  assert.deepEqual(listeningInputs(null), []);
  // A start by hand asks every input, as the definition holds them; a start by listening, what listening needs.
  assert.equal(startInputs(wf, false), inputs, "the definition's own list: no copy a memo would take for a change");
  assert.deepEqual(startInputs(wf, true).map((d) => d.name), ["audience", "when"]);
  assert.equal(startInputs(null, false), startInputs(undefined, false), "and nothing is one nothing");
  for (const screen of ["../../_goal/RunVerbDialogs.tsx", "../GoalWorkflowTab.tsx"]) {
    const text = readFileSync(new URL(screen, import.meta.url), "utf8");
    assert.ok(/(startInputs|askedAtStart)\(/.test(text) && !text.includes("needs.has("), `${screen} asks by the model's rule`);
  }
});

// added by the coverage pass: startForm.test.mjs
test("an unknown event reads as its own word, samples nothing, and each event's templates are its own fields", () => {
  assert.equal(eventPhrase({ event: "weird" }), "weird");
  assert.deepEqual(samplePayload({ event: "weird" }), {});
  assert.deepEqual(startTemplates({ event: "message", in: "support", contains: "{inputs.word}" }), ["support", "{inputs.word}"]);
  assert.deepEqual(startTemplates({ event: "message" }), []);
  assert.deepEqual(startTemplates({ event: "project", branch: "main", glob: "src/**" }), ["main", "src/**"]);
  assert.deepEqual(startTemplates({ event: "platform", topic: "goal.closed", fields: { goal: "{inputs.goal}", n: 3 } }), ["{inputs.goal}"]);
  assert.deepEqual(startTemplates({ event: "connector", params: { channel: "{inputs.channel}" } }), ["{inputs.channel}"]);
  assert.deepEqual(startTemplates({ event: "connector" }), []);
});
