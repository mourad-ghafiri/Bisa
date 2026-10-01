/**
 * The setup gate's rules. Run with `node --test desktop/src/shell/setupModel.test.mjs`.
 */
import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync } from "node:fs";
import { CHECK_IDS, RECHECK_MS, UNREAD, afterRead, bannerWords, checkWords, commandsFor, doorLabel, doorTarget, fixBody, gateMode, movesReadiness, offeredFixes, platformOf, progressWords, recheckMs } from "./setupModel.mjs";

const rust = (path) => readFileSync(new URL(`../../../crates/${path}`, import.meta.url), "utf8");
const PLAN = { strategy: "fallback", models: [{ model: "claude-opus-5-5[1m]", weight: 1, enabled: true }, { model: "claude-sonnet-5-5[1m]", weight: 1, enabled: true }] };
const five = (states) => ({ ready: states.every((s) => s === "ready"), checks: states.map((state, n) => ({ id: CHECK_IDS[n], state, title: CHECK_IDS[n], detail: "", door: { door: "none" }, fixes: [] })), checked_at: 1 });

const hint = {
  url: "https://git-scm.com/install",
  commands: [
    { platform: "mac_os", command: "brew install git" },
    { platform: "linux", command: "sudo apt-get install git" },
    { platform: "windows", command: "winget install --id Git.Git -e --source winget" },
  ],
};

test("the platform is read off the navigator, and unknown reads as linux", () => {
  assert.equal(platformOf({ platform: "MacIntel" }), "mac_os");
  assert.equal(platformOf({ userAgent: "Mozilla/5.0 (Macintosh; …)" }), "mac_os");
  assert.equal(platformOf({ platform: "Win32" }), "windows");
  assert.equal(platformOf({ platform: "Linux x86_64" }), "linux");
  assert.equal(platformOf(null), "linux");
  assert.equal(platformOf({}), "linux");
});

test("the lines shown are this platform's, in the page's order", () => {
  assert.deepEqual(commandsFor(hint, "mac_os"), ["brew install git"]);
  assert.deepEqual(commandsFor(hint, "windows"), ["winget install --id Git.Git -e --source winget"]);
  assert.deepEqual(commandsFor(null, "linux"), []);
});

test("the gate is nothing while ready or unknown, a banner where things are fixed, the modal everywhere else", () => {
  assert.equal(gateMode({ ready: true, screen: "pulse" }), "none");
  assert.equal(gateMode({ ready: null, screen: "pulse" }), "none", "unknown never blocks");
  assert.equal(gateMode({ ready: undefined, screen: "settings" }), "none");
  assert.equal(gateMode({ ready: false, screen: "settings" }), "banner");
  assert.equal(gateMode({ ready: false, screen: "agents" }), "banner");
  assert.equal(gateMode({ ready: false, screen: "agent" }), "banner");
  assert.equal(gateMode({ ready: false, screen: "pulse" }), "modal");
  assert.equal(gateMode({ ready: false, screen: "workbench" }), "modal");
  assert.equal(gateMode({ ready: false, screen: null }), "modal");
});

test("progress counts the ready checks of the node's five", () => {
  assert.deepEqual([...CHECK_IDS], ["git", "harness", "decision_making_agent", "general_agent", "workflow_agent"]);
  assert.equal(progressWords({ checks: [{ state: "ready" }, { state: "missing" }, { state: "unready" }, { state: "ready" }, { state: "ready" }] }), "3 of 5 ready");
  assert.equal(progressWords(null), "0 of 0 ready");
});

test("a check's card wears its glyph, its tone and its state word", () => {
  assert.deepEqual(checkWords({ id: "git", state: "ready" }), { icon: "branch", tone: "ok", word: "ready" });
  assert.deepEqual(checkWords({ id: "harness", state: "missing" }), { icon: "harness", tone: "danger", word: "not installed" });
  assert.deepEqual(checkWords({ id: "decision_making_agent", state: "unready" }), { icon: "decisions", tone: "warn", word: "not set up" });
  assert.equal(checkWords({ id: "general_agent", state: "unready" }).icon, "coreAgent");
  assert.equal(checkWords({ id: "workflow_agent", state: "unready" }).icon, "coreAgent");
  assert.equal(checkWords(null).icon, "warn");
});

test("a check id the node no longer says is no check of the gate's", () => {
  assert.equal(CHECK_IDS.includes("decision_maker"), false); // terminology-lint-ignore: decision-maker - proves the retired word is refused
  assert.equal(checkWords({ id: "decision_maker", state: "unready" }).icon, "warn"); // terminology-lint-ignore: decision-maker - proves the retired word is refused
});

test("a door opens Settings on its tab or Agents, and says so", () => {
  assert.deepEqual(doorTarget({ door: "settings", tab: "decision-making" }), { route: { name: "settings" }, search: { tab: "decision-making" } });
  assert.deepEqual(doorTarget({ door: "agents" }), { route: { name: "agents" } });
  assert.equal(doorTarget({ door: "none" }), null);
  assert.equal(doorTarget(null), null);
  assert.equal(doorLabel({ door: "settings", tab: "harnesses" }), "Open Settings › Capabilities › Harnesses", "the path as the rail shows it: group, then panel");
  assert.equal(doorLabel({ door: "settings" }), "Open Settings › Capabilities › Harnesses", "a door that names no tab opens Harnesses, and says so");
  assert.equal(doorLabel({ door: "settings", tab: "decision-making" }), "Open Settings › Decision Settings › Decision Making");
  assert.equal(doorLabel({ door: "settings", tab: "decisions" }), "Open Settings › You › Identity", "the retired tab names no door of its own: the button says where an unknown tab lands");
  assert.equal(doorLabel({ door: "agents" }), "Open Agents");
  assert.equal(doorLabel({ door: "none" }), "");
});

test("a fix is one call: workspace settings, or an agent's harness and plan", () => {
  assert.deepEqual(fixBody({ kind: "settings", label: "Use Mock", set: { "decisions.enabled": true } }), { kind: "settings", set: { "decisions.enabled": true } });
  const plan = { strategy: "fallback", models: [{ model: "m", weight: 1, enabled: true }] };
  assert.deepEqual(fixBody({ kind: "agent_harness", label: "Run on Mock", agent: "general-agent", harness: "mock", models: plan }), { kind: "agent", id: "general-agent", body: { harness: "mock", models: plan } });
  // The plan a fix writes is the node's — what the harness recommends — handed on whole, an effort included.
  const recommended = { strategy: "fallback", effort: "high", models: [{ model: "claude-opus-5-5[1m]", weight: 1, enabled: true }, { model: "claude-sonnet-5-5[1m]", weight: 1, enabled: true, effort: "max" }] };
  assert.deepEqual(fixBody({ kind: "agent_harness", label: "Run on Claude Code", agent: "workflow-agent", harness: "claude-code", models: recommended }).body, { harness: "claude-code", models: recommended });
  assert.deepEqual(fixBody({ kind: "settings", label: "Use Claude Code", set: { "decisions.harness.model": "claude-sonnet-5-5[1m]", "decisions.harness.effort": "high" } }).set, { "decisions.harness.model": "claude-sonnet-5-5[1m]", "decisions.harness.effort": "high" });
});

test("a fix that is no call is no button: nothing is made up for what the node did not say", () => {
  // An agent's move needs its agent, its harness and its plan — never a harness of "" or a plan invented empty, which would take the agent's models.
  assert.equal(fixBody({ kind: "agent_harness", label: "Run on Claude Code", harness: "claude-code", models: PLAN }), null, "no agent named");
  assert.equal(fixBody({ kind: "agent_harness", label: "Run on Claude Code", agent: "general-agent", models: PLAN }), null, "no harness named");
  assert.equal(fixBody({ kind: "agent_harness", label: "Run on Claude Code", agent: "general-agent", harness: "  ", models: PLAN }), null);
  assert.equal(fixBody({ kind: "agent_harness", label: "Run on Claude Code", agent: "general-agent", harness: "claude-code" }), null, "no plan: the agent's own is not replaced by none");
  assert.equal(fixBody({ kind: "agent_harness", label: "Run on Claude Code", agent: "general-agent", harness: "claude-code", models: [] }), null);
  // Settings that set nothing write nothing.
  assert.equal(fixBody({ kind: "settings", label: "Use it", set: {} }), null);
  assert.equal(fixBody({ kind: "settings", label: "Use it" }), null);
  // A kind this build does not know is never read as an agent's.
  assert.equal(fixBody({ kind: "install", label: "Install git", agent: "general-agent", harness: "claude-code", models: PLAN }), null);
  assert.equal(fixBody({ label: "Nameless" }), null);
  assert.equal(fixBody(null), null);
  // The kinds are the engine's two.
  const engine = rust("bisa-engine/src/readiness.rs");
  const fix = engine.slice(engine.indexOf("pub enum Fix"));
  assert.deepEqual([...fix.slice(0, fix.indexOf("\n}")).matchAll(/^ {4}([A-Z][A-Za-z]*) \{/gm)].map((m) => m[1]), ["Settings", "AgentHarness"], "`Fix` is settings written, or an agent moved to a harness");

  const check = { fixes: [
    { kind: "settings", label: "Use Claude Code · claude-sonnet-5-5[1m] as the Decision-Making Agent", set: { "decisions.enabled": true } },
    { kind: "agent_harness", label: "Run on Claude Code", agent: "general-agent", harness: "claude-code", models: PLAN },
    { kind: "agent_harness", label: "Run on Codex", agent: "general-agent", harness: "", models: PLAN },
    { kind: "agent_harness", label: "Run on Claude Code", agent: "general-agent", harness: "claude-code", models: PLAN },
    { kind: "settings", label: "", set: { "decisions.enabled": true } },
  ] };
  assert.deepEqual(offeredFixes(check).map((f) => f.label), ["Use Claude Code · claude-sonnet-5-5[1m] as the Decision-Making Agent", "Run on Claude Code"], "each call once by its label, in the node's order");
  assert.equal(offeredFixes(check)[1], check.fixes[1], "the node's own fix, handed on");
  assert.deepEqual(offeredFixes({ fixes: [] }), []);
  assert.deepEqual(offeredFixes(null), []);
});

test("every word a card says is the catalog's, the ready one included; the banner says how far along and the two ways on", () => {
  assert.equal(checkWords({ id: "git", state: "ready" }).word, "ready");
  const model = readFileSync(new URL("./setupModel.mjs", import.meta.url), "utf8");
  assert.ok(model.includes('state === "ready" ? t("shell-setup-ready-word")'), "said through the catalog, as the other two states are");
  assert.equal(bannerWords(five(["ready", "ready", "unready", "ready", "unready"])), "— 3 of 5 ready. Finish here, or go back to the setup.");
  assert.equal(bannerWords(null), "— 0 of 0 ready. Finish here, or go back to the setup.");
  const gate = readFileSync(new URL("./SetupGate.tsx", import.meta.url), "utf8");
  assert.ok(gate.includes("{bannerWords(readiness)}") && !gate.includes("Finish here"), "the banner's sentence is the model's, never the component's");
});

test("a read that landed is what the gate knows; one that failed keeps the last answer and says why", () => {
  assert.deepEqual(UNREAD, { readiness: null, error: null });
  const missing = five(["ready", "missing", "unready", "unready", "unready"]);
  let state = afterRead(UNREAD, { ok: true, readiness: missing });
  assert.deepEqual(state, { readiness: missing, error: null });
  // The node stops answering while the person installs a harness: the gate stays, with the reason.
  state = afterRead(state, { ok: false, error: "the node did not answer" });
  assert.equal(state.readiness, missing, "a node that stopped answering says nothing about git");
  assert.equal(state.error, "the node did not answer");
  // It answers again: everything is here, and the failure is forgotten.
  const ready = five(["ready", "ready", "ready", "ready", "ready"]);
  state = afterRead(state, { ok: true, readiness: ready });
  assert.deepEqual(state, { readiness: ready, error: null });
  // A first read that failed leaves the gate not knowing — and saying why.
  assert.deepEqual(afterRead(UNREAD, { ok: false, error: "503" }), { readiness: null, error: "503" });
});

test("the gate reads again on its own while something is missing and while it does not know — never once everything is here", () => {
  assert.equal(RECHECK_MS, 20_000, "every twenty seconds");
  assert.equal(recheckMs(UNREAD), null, "the first read is out: nothing to wait for yet");
  assert.equal(recheckMs({ readiness: five(["ready", "missing", "unready", "unready", "unready"]), error: null }), RECHECK_MS);
  assert.equal(recheckMs({ readiness: five(["ready", "ready", "ready", "ready", "ready"]), error: null }), null);
  // The first read failed — the node was not up yet when the desktop opened: asked again, or a machine with no harness never sees the gate.
  assert.equal(recheckMs({ readiness: null, error: "the node did not answer" }), RECHECK_MS);
  assert.equal(gateMode({ ready: undefined, screen: "inbox" }), "none", "and meanwhile what is unknown blocks nothing");
  // A re-read that failed keeps the answer, and the answer decides.
  assert.equal(recheckMs({ readiness: five(["ready", "missing", "ready", "ready", "ready"]), error: "the node did not answer" }), RECHECK_MS);
  assert.equal(recheckMs({ readiness: five(["ready", "ready", "ready", "ready", "ready"]), error: "the node did not answer" }), null);
});

test("a setting changed is worth reading the checks again; a fact about anything else is not", () => {
  assert.equal(movesReadiness({ type: "settings_changed", scope: "workspace", keys: ["decisions.enabled"] }), true);
  assert.equal(movesReadiness({ type: "goal_created", goal: "01G" }), false);
  assert.equal(movesReadiness({ type: "session_state" }), false);
  assert.equal(movesReadiness({}), false);
  assert.equal(movesReadiness(null), false);
  assert.ok(rust("bisa-engine/src/events.rs").includes("SettingsChanged"), "a fact the engine says");
});

test("the five checks are the engine's, in its order", () => {
  const engine = rust("bisa-engine/src/readiness.rs");
  const ids = engine.slice(engine.indexOf("pub enum CheckId"));
  const variants = [...ids.slice(0, ids.indexOf("\n}")).matchAll(/^ {4}([A-Z][A-Za-z]*),/gm)].map((m) => m[1].replace(/([a-z])([A-Z])/g, "$1_$2").toLowerCase());
  assert.deepEqual(variants, [...CHECK_IDS]);
});
