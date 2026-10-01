/**
 * The setup gate (16 — The setup gate) as the desktop wires it: mounted over
 * every screen in its own boundary, an alert dialog nobody dismisses, this
 * platform's official lines to copy and the page they come from, the fixes as
 * one call each — and Settings › Harnesses showing the same lines for a
 * missing harness — and as a person meets it on a machine with nothing
 * installed, read by read over the gate's model. Source guards and model
 * steps, no DOM.
 *
 * Run with `node --test desktop/src/scenarios/setup.test.mjs`.
 */
import { readFileSync } from "node:fs";
import { test } from "node:test";
import assert from "node:assert/strict";

import { CHECK_IDS, RECHECK_MS, UNREAD, afterRead, bannerWords, checkWords, commandsFor, doorLabel, doorTarget, fixBody, gateMode, movesReadiness, offeredFixes, platformOf, progressWords, recheckMs } from "../shell/setupModel.mjs";

test("the gate stands over every screen in its own boundary, and only Settings and Agents stay usable beneath it", () => {
  const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
  const app = src("../App.tsx");
  assert.ok(app.includes('<OverlayBoundary name="setup gate"') && app.includes("<SetupGate screen={route.name} />"), "mounted in its own boundary, told the screen");
  const gate = src("../shell/SetupGate.tsx");
  assert.ok(gate.includes('from "@radix-ui/react-alert-dialog"') && gate.includes("onEscapeKeyDown={(e) => e.preventDefault()}"), "an alert dialog that Escape does not close");
  assert.ok(!gate.includes("<A.Cancel") && !gate.includes("onOpenChange"), "no close");
  const hook = src("../shell/useReadiness.ts");
  assert.ok(gate.includes("useReadiness()") && !/api\s*\.readiness\(/.test(gate), "the read is a hook of its own, where the reconnect guard sees it");
  assert.ok(/api\s*\.readiness\(/.test(hook) && hook.includes("movesReadiness(e.payload)") && hook.includes("recheckMs(state)"), "read on mount, on a fact that moves the checks, on the model's cadence");
  assert.ok(hook.includes("useReloadOnReconnect(check)"), "and when the node comes back after it was away");
  assert.ok(hook.includes("afterRead(was, { ok: true, readiness })") && hook.includes("afterRead(was, { ok: false, error:"), "what a read leaves the gate knowing is the model's");
  assert.ok(hook.includes('log.warn("setup", "the checks could not be read"'), "a read that failed is said in the log, never swallowed");
  assert.ok(gate.includes("copyText(") && gate.includes("openExternal(hint.url)"), "the official line is copied, the page opened — nothing run");
  assert.ok(!gate.includes("<a ") && !gate.includes("href="), "a URL is a button through openExternal, never a link");
  assert.ok(gate.includes('api.setSettings("workspace", body.set)') && gate.includes("api.patchAgent(body.id, body.body)"), "a fix is one call");
  assert.ok(gate.includes("const fixes = offeredFixes(check);") && gate.includes("if (!body) return;"), "and a fix that is no call is no button");
  assert.ok(gate.includes("navigate(door.route, door.search)") && gate.includes('navigate({ name: "pulse" })'), "doors and the way back");
  assert.equal(gateMode({ ready: false, screen: "settings" }), "banner");
  assert.equal(gateMode({ ready: false, screen: "inbox" }), "modal");
  const panel = src("../views/_settings/HarnessesPanel.tsx");
  assert.ok(panel.includes("commandsFor(h.install") && panel.includes("openExternal(h.install!.url)"), "Settings › Harnesses shows the same lines for a missing harness");
  assert.ok(src("../types.hand.ts").includes("install: import(\"./types.gen\").InstallHint | null"), "the row carries the hint");
  assert.ok(src("../api.ts").includes('get<Readiness>("/readiness", s)'), "the client reads the five checks");
});

/** What `GET /readiness` answers, check by check. */
const PLAN = { strategy: "fallback", models: [{ model: "claude-opus-5-5[1m]", weight: 1, enabled: true }, { model: "claude-sonnet-5-5[1m]", weight: 1, enabled: true }] };
const GIT_HINT = { url: "https://git-scm.com/install", commands: [{ platform: "mac_os", command: "brew install git" }, { platform: "mac_os", command: "xcode-select --install" }, { platform: "linux", command: "sudo apt-get install git" }, { platform: "windows", command: "winget install --id Git.Git -e --source winget" }], verify: "git --version" };
const HARNESS_HINT = { url: "https://code.claude.com/docs/en/setup", commands: [{ platform: "mac_os", command: "curl -fsSL https://claude.ai/install.sh | bash" }], verify: "claude --version", sign_in: "run claude and follow the prompt" };
const DMA_FIX = { kind: "settings", label: "Use Claude Code · claude-sonnet-5-5[1m] as the Decision-Making Agent", set: { "decisions.enabled": true, "decisions.provider": "harness", "decisions.harness.id": "claude-code", "decisions.harness.model": "claude-sonnet-5-5[1m]" } };
const runOn = (agent) => ({ kind: "agent_harness", label: "Run on Claude Code", agent, harness: "claude-code", models: PLAN });
function answer(states, installed) {
  const [git, harness, dma, general, workflow] = states;
  const checks = [
    { id: "git", state: git, title: "git", detail: git === "ready" ? "git 2.47" : "git is not on your PATH", hint: git === "ready" ? null : GIT_HINT, door: { door: "none" }, fixes: [] },
    { id: "harness", state: harness, title: "A coding harness", detail: harness === "ready" ? "Claude Code is installed" : "no coding harness is installed", hint: harness === "ready" ? null : HARNESS_HINT, door: { door: "settings", tab: "harnesses" }, fixes: [] },
    { id: "decision_making_agent", state: dma, title: "Decision-Making Agent", detail: "", door: { door: "settings", tab: "decision-making" }, fixes: dma !== "ready" && installed ? [DMA_FIX] : [] },
    { id: "general_agent", state: general, title: "General Agent", detail: "", door: { door: "agents" }, fixes: general !== "ready" && installed ? [runOn("general-agent")] : [] },
    { id: "workflow_agent", state: workflow, title: "Workflow Agent", detail: "", door: { door: "agents" }, fixes: workflow !== "ready" && installed ? [runOn("workflow-agent")] : [] },
  ];
  return { ready: states.every((s) => s === "ready"), checks, checked_at: 1 };
}

test("a first launch on a machine with nothing installed: unknown blocks nothing and is asked again, then the gate stands until the five are ready", () => {
  const platform = platformOf({ platform: "MacIntel" });
  // The desktop opens before its node answers: the first read fails.
  let gate = afterRead(UNREAD, { ok: false, error: "the node did not answer" });
  assert.equal(gateMode({ ready: gate.readiness?.ready, screen: "inbox" }), "none", "what is unknown blocks nothing");
  assert.equal(recheckMs(gate), RECHECK_MS, "and is asked again: the gate is not unknown for the life of the window");

  // The node answers: nothing is here.
  gate = afterRead(gate, { ok: true, readiness: answer(["missing", "missing", "unready", "unready", "unready"], false) });
  assert.equal(gate.error, null);
  assert.equal(gateMode({ ready: gate.readiness.ready, screen: "inbox" }), "modal");
  assert.equal(progressWords(gate.readiness), "0 of 5 ready");
  assert.deepEqual(gate.readiness.checks.map((c) => c.id), [...CHECK_IDS], "the five, in the node's order");
  assert.deepEqual(gate.readiness.checks.map((c) => checkWords(c).word), ["not installed", "not installed", "not set up", "not set up", "not set up"]);
  // The git card: this platform's official lines to copy, the page to open — and nothing to press that would run one.
  const [git, harness, dma, general] = gate.readiness.checks;
  assert.deepEqual(commandsFor(git.hint, platform), ["brew install git", "xcode-select --install"]);
  assert.deepEqual(offeredFixes(git), [], "the platform installs nothing: git has no fix");
  assert.equal(doorTarget(git.door), null);
  assert.deepEqual(commandsFor(harness.hint, platform), ["curl -fsSL https://claude.ai/install.sh | bash"]);
  assert.equal(doorLabel(harness.door), "Open Settings › Capabilities › Harnesses");
  assert.deepEqual(offeredFixes(dma), [], "no harness yet: nothing to move the agents onto");
  assert.equal(recheckMs(gate), RECHECK_MS, "read again while the person installs in another window");

  // The door to Settings › Harnesses: the modal becomes the banner, and the screen stays usable.
  const door = doorTarget(harness.door);
  assert.deepEqual(door, { route: { name: "settings" }, search: { tab: "harnesses" } });
  assert.equal(gateMode({ ready: gate.readiness.ready, screen: door.route.name }), "banner");
  assert.equal(bannerWords(gate.readiness), "— 0 of 5 ready. Finish here, or go back to the setup.");

  // git and Claude Code are installed; the tick reads again, and the fixes appear.
  gate = afterRead(gate, { ok: true, readiness: answer(["ready", "ready", "unready", "unready", "unready"], true) });
  assert.equal(progressWords(gate.readiness), "2 of 5 ready");
  const fixes = gate.readiness.checks.flatMap((c) => offeredFixes(c));
  assert.deepEqual(fixes.map((f) => f.label), ["Use Claude Code · claude-sonnet-5-5[1m] as the Decision-Making Agent", "Run on Claude Code", "Run on Claude Code"]);
  assert.deepEqual(fixBody(fixes[0]), { kind: "settings", set: DMA_FIX.set }, "one call: the workspace's settings");
  assert.deepEqual(fixBody(fixes[1]), { kind: "agent", id: "general-agent", body: { harness: "claude-code", models: PLAN } }, "one call: the agent's harness and the plan the harness recommends");
  assert.deepEqual(fixBody(fixes[2]).id, "workflow-agent");

  // The Decision-Making Agent's fix lands: the node says a setting changed, and the gate reads again.
  assert.equal(movesReadiness({ type: "settings_changed", scope: "workspace", keys: Object.keys(DMA_FIX.set) }), true);
  gate = afterRead(gate, { ok: true, readiness: answer(["ready", "ready", "ready", "unready", "unready"], true) });
  assert.equal(progressWords(gate.readiness), "3 of 5 ready");
  // The node restarts in the middle: the read fails, and the gate stays as it stood, saying why.
  gate = afterRead(gate, { ok: false, error: "the node did not answer" });
  assert.equal(gateMode({ ready: gate.readiness.ready, screen: "goals" }), "modal");
  assert.equal(progressWords(gate.readiness), "3 of 5 ready");
  assert.equal(gate.error, "the node did not answer");
  assert.equal(recheckMs(gate), RECHECK_MS);

  // Both agents are moved onto Claude Code: everything is here, and the gate closes on its own.
  gate = afterRead(gate, { ok: true, readiness: answer(["ready", "ready", "ready", "ready", "ready"], true) });
  assert.equal(gateMode({ ready: gate.readiness.ready, screen: "goals" }), "none");
  assert.equal(gateMode({ ready: gate.readiness.ready, screen: "settings" }), "none");
  assert.equal(recheckMs(gate), null, "and reads no more on its own");
  assert.deepEqual(gate.readiness.checks.map((c) => checkWords(c)), [
    { icon: "branch", tone: "ok", word: "ready" },
    { icon: "harness", tone: "ok", word: "ready" },
    { icon: "decisions", tone: "ok", word: "ready" },
    { icon: "coreAgent", tone: "ok", word: "ready" },
    { icon: "coreAgent", tone: "ok", word: "ready" },
  ]);
  assert.equal(general.id, "general_agent");
});

test("a harness uninstalled later brings the gate back, and a setting changed is what it hears", () => {
  let gate = afterRead(UNREAD, { ok: true, readiness: answer(["ready", "ready", "ready", "ready", "ready"], true) });
  assert.equal(gateMode({ ready: gate.readiness.ready, screen: "workbench" }), "none");
  assert.equal(recheckMs(gate), null);
  // The node came back after it was away — the bus reconnected — and the gate read again: the harness is gone.
  gate = afterRead(gate, { ok: true, readiness: answer(["ready", "missing", "unready", "unready", "unready"], false) });
  assert.equal(gateMode({ ready: gate.readiness.ready, screen: "workbench" }), "modal");
  assert.equal(gateMode({ ready: gate.readiness.ready, screen: "agent" }), "banner", "an agent's own page stays usable too");
  assert.equal(progressWords(gate.readiness), "1 of 5 ready");
  assert.equal(recheckMs(gate), RECHECK_MS);
});
