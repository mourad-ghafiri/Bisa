/**
 * Settings that change what the IDE offers, followed through the models a
 * person would move: the Board switch, the mode cycle and its memory, the
 * links into panels, the proxy choice and its bypass list — with the
 * registry read from the Rust source where a fact is the node's (ide/13).
 * Run with `node --test desktop/src/scenarios/settings.test.mjs`.
 */
import { readFileSync } from "node:fs";
import { test } from "node:test";
import assert from "node:assert/strict";

import { BOARD_DEFAULTS, BOARD_ENABLED_KEY } from "../views/_board/boardSettings.mjs";
import { KEYS as DECISION_KEYS, blankKey, fieldsFor, keyFor, keySaved, keyTyped, maySaveKey, takesKey } from "../views/_settings/decisionsModel.mjs";
import { KEYS, MODES, modeSegments } from "../views/_settings/networkModel.mjs";
import { MASK, blank, toTransport, validate } from "../views/_settings/mcpFormModel.mjs";
import { healthTone, healthWords } from "../views/_settings/mcpHealthModel.mjs";
import { SETTINGS_GROUPS, SETTINGS_TABS, settingsPath, settingsSearch, settingsTab } from "../views/_settings/settingsLink.mjs";
import { DEFAULT_MODE, DEFAULT_MODE_KEY, availableModes, modeFor, nextMode, parseRememberedModes, rememberMode } from "../views/_workbench/ideModeModel.mjs";
import { englishFiles } from "../i18n/testing.mjs";
import { sourceFiles } from "../testWalk.mjs";

const registry = readFileSync(new URL("../../../crates/bisa-core/src/settings.rs", import.meta.url), "utf8");
const ROOT = "workstream:01JROOT";

test("turning the Board off hides the mode, drops a remembered Board mode to the default, and the cycle skips it", () => {
  // The switch is a registry key whose default is on.
  assert.ok(registry.includes(`"${BOARD_ENABLED_KEY}",`));
  assert.equal(BOARD_DEFAULTS.enabled, true);
  assert.ok(registry.includes(`"${DEFAULT_MODE_KEY}",`), "the default mode is a registry key too");

  // On: three modes, the cycle goes round.
  let memory = rememberMode({}, ROOT, "board");
  assert.deepEqual(availableModes(true), ["project", "agent", "board"]);
  assert.equal(modeFor(memory[ROOT], "project", true), "board", "the root remembers Board");
  assert.equal(nextMode("board", true), "project");

  // The person switches the Board off in Settings › Project IDE › Board.
  const boardEnabled = false;
  assert.deepEqual(availableModes(boardEnabled), ["project", "agent"]);
  assert.equal(modeFor(memory[ROOT], "project", boardEnabled), DEFAULT_MODE, "a remembered Board falls back rather than showing a centre that is hidden");
  assert.equal(nextMode("agent", boardEnabled), "project", "the cycle skips Board");
  assert.equal(modeFor(undefined, "board", boardEnabled), DEFAULT_MODE, "even the settings' own default is clamped");

  // The memory itself is untouched and comes back when the Board returns.
  memory = parseRememberedModes(JSON.parse(JSON.stringify(memory)));
  assert.equal(modeFor(memory[ROOT], "project", true), "board");
});

test("the mode memory is bounded and survives a round trip through storage, dropping what is not a root or a mode", () => {
  let memory = {};
  for (let i = 0; i < 12; i++) memory = rememberMode(memory, `workstream:${String(i).padStart(4, "0")}`, "agent", 10);
  assert.equal(Object.keys(memory).length, 10, "the cap holds");
  assert.ok(!("workstream:0000" in memory) && "workstream:0011" in memory, "the oldest went first");
  const stored = JSON.parse(JSON.stringify({ ...memory, "not-a-root": "agent", "goal:01": "carousel", [ROOT]: "project" }));
  const back = parseRememberedModes(stored);
  assert.equal(back[ROOT], "project");
  assert.ok(!("not-a-root" in back), "a key with no scope is not a root");
  assert.ok(!("goal:01" in back), "a word that is not a mode is dropped");
  assert.deepEqual(parseRememberedModes("[]"), {});
  assert.deepEqual(parseRememberedModes([["a", "b"]]), {});
});

test("a link into Settings lands on its panel, an unknown one on the first, and empty extras write nothing", () => {
  for (const tab of SETTINGS_TABS) assert.equal(settingsTab(tab), tab);
  assert.equal(settingsTab("nope"), SETTINGS_TABS[0]);
  assert.equal(settingsTab(null), SETTINGS_TABS[0]);
  assert.ok(SETTINGS_TABS.includes("board") && SETTINGS_TABS.includes("network") && SETTINGS_TABS.includes("ide"), "the panels this scenario walks exist");
  assert.deepEqual(settingsSearch("catalog-agent", { kind: "agent", extra: "", gone: null, missing: undefined }), { tab: "catalog-agent", kind: "agent" });
  assert.deepEqual(settingsSearch("network"), { tab: "network" });
});

test("the proxy is one of the registry's three words, drawn in its order, and the bypass list is what the person typed minus the noise", () => {
  const at = registry.indexOf(`"${KEYS.mode}",`);
  const choice = registry.slice(at, registry.indexOf("]),", at) + 3).match(/Choice\(&\[([^\]]*)\]\)/);
  assert.ok(choice);
  const words = choice[1].split(",").map((w) => w.trim().replace(/"/g, "")).filter(Boolean);
  assert.deepEqual([...MODES], words);
  assert.deepEqual(modeSegments().map((s) => s.id), words, "the segmented control is the registry's order");
  for (const key of Object.values(KEYS)) assert.ok(registry.includes(`"${key}",`), `${key} is registered`);
  assert.equal(KEYS.publicIpUrl, "network.public_ip_url", "the echo service is a registry key the panel draws as its own row");
});

test("every decisions.* key the panel edits is a registered setting, and a provider shows only its own", () => {
  const flat = [
    DECISION_KEYS.enabled,
    DECISION_KEYS.pointsOff,
    DECISION_KEYS.provider,
    DECISION_KEYS.harness.id,
    DECISION_KEYS.harness.model,
    DECISION_KEYS.harness.effort,
    DECISION_KEYS.agent.id,
    DECISION_KEYS.jev.model,
    DECISION_KEYS.rlcd.endpoint,
    DECISION_KEYS.rlcd.model,
    DECISION_KEYS.rlcd.auth,
    DECISION_KEYS.deadline,
    DECISION_KEYS.retries,
    DECISION_KEYS.confidenceAct,
    DECISION_KEYS.confidenceSecurity,
  ];
  for (const key of flat) assert.ok(registry.includes(`"${key}",`), `${key} is registered`);
  assert.ok(SETTINGS_TABS.includes("decision-making"), "Settings has a panel for it");
  assert.ok(!SETTINGS_TABS.includes("decisions"), "under one id");

  // A person choosing a provider sees only its own fields, and only Jev and a
  // bearer-auth RLCD ask for a key.
  assert.deepEqual(fieldsFor("harness"), [DECISION_KEYS.harness.id, DECISION_KEYS.harness.model, DECISION_KEYS.harness.effort]);
  assert.deepEqual(fieldsFor("agent"), [DECISION_KEYS.agent.id]);
  assert.ok(takesKey("jev"));
  assert.ok(!takesKey("harness"));
  assert.ok(takesKey("rlcd", "bearer") && !takesKey("rlcd", "none"));
});

test("Settings draws the group Decision Settings over its one panel, Decision Making — two messages, one panel file", () => {
  // The rail is the model's (`SETTINGS_GROUPS`); the screen adds the glyph and draws the panel.
  const group = SETTINGS_GROUPS.find((g) => g.id === "decisions");
  assert.equal(group.label, "Decision Settings", "the group's heading");
  assert.deepEqual(group.panels.map((p) => [p.id, p.label]), [["decision-making", "Decision Making"]], "its one panel, under the tab's id");
  const model = readFileSync(new URL("../views/_settings/settingsLink.mjs", import.meta.url), "utf8");
  assert.equal(model.split('t("screens-settings-decision-settings")').length, 2, "the heading is said once");
  assert.equal(model.split('t("screens-settings-decision-making")').length, 2, "and so is the panel's label");
  const settings = readFileSync(new URL("../views/Settings.tsx", import.meta.url), "utf8");
  assert.ok(settings.includes('"decision-making": ICON.decisions,'), "the panel's glyph");
  assert.ok(settings.includes('{panel.id === "decision-making" && <DecisionsPanel />}'), "the tab draws the panel");
  assert.ok(settings.includes('import { DecisionsPanel } from "./_settings/DecisionsPanel";'), "the panel's file keeps its name");
  const words = new Map(englishFiles().flatMap(([, text]) => [...text.matchAll(/^([a-z0-9-]+) = (.*)$/gm)].map((m) => [m[1], m[2]])));
  assert.equal(words.get("screens-settings-decision-settings"), "Decision Settings");
  assert.equal(words.get("screens-settings-decision-making"), "Decision Making");
  assert.ok(!words.has("screens-settings-decisions"), "the one message both labels shared is gone");
  // Where it is, as the rail shows it: the doors say the model's path, and no message repeats it.
  assert.equal(settingsPath("decision-making"), "Settings › Decision Settings › Decision Making");
  assert.ok(!words.has("settings-security-panels-settings-decision-making") && !words.has("shell-setup-open-settings-decision-making"), "the two messages that spelt a shorter path are gone");
  // Every door into the panel names the tab it opens.
  for (const door of ["../views/Agents.tsx", "../views/_settings/SecurityPanels.tsx"]) {
    const text = readFileSync(new URL(door, import.meta.url), "utf8");
    assert.ok(text.includes('settingsSearch("decision-making")') && !text.includes('settingsSearch("decisions")'), `${door} opens Settings › Decision Making`);
  }
});

test("the Agents screen draws the three core agents in one block: the two that hold a record, then the Decision-Making Agent", () => {
  const agents = readFileSync(new URL("../views/Agents.tsx", import.meta.url), "utf8");
  const from = agents.indexOf("{cores.length > 0 && (");
  const to = agents.indexOf('t("screens-agents-three-core-agents-pinned-above-filters")');
  assert.ok(from > 0 && to > from, "the block, closed by the note under the three");
  const block = agents.slice(from, to);
  const two = block.indexOf("{cores.map((core) => (");
  const card = block.indexOf("<AgentCard");
  const third = block.indexOf("<DecisionMakingAgentCard />");
  assert.ok(two > 0 && card > two && third > card, "the General Agent and the Workflow Agent, then the Decision-Making Agent");
  assert.equal(block.split("<AgentCard").length, 2, "one card drawn per core agent that holds a record");
  assert.ok(agents.includes("const cores = useMemo(() => coreAgents(agents), [agents]);"), "the two are the model's");
  assert.ok(agents.includes("function DecisionMakingAgentCard() {"), "the third card is the screen's own");
  assert.ok(agents.includes("status.data?.agent.name") && agents.includes("status.data?.agent.description"), "its name and description are the status's agent");
  assert.ok(!agents.includes("status.data?.model"), "and nothing reads the field the status no longer has");
  assert.ok(agents.includes('t("screens-agents-decision-making-agent")') && agents.includes('t("screens-agents-reading")'), "while the status is read, the catalog names it");
  assert.ok(agents.includes("coreLine(status.data, status.error)"), "the line under them is the model's, a failed read included");
  const panel = readFileSync(new URL("../views/_settings/DecisionsPanel.tsx", import.meta.url), "utf8");
  assert.ok(panel.includes("status.data.agent.name") && panel.includes("status.data.agent.description") && !panel.includes("status.data.model"), "the panel's header reads the same agent");
});

test("nothing that speaks of the Decision-Making Agent says it is anything but the third core agent", () => {
  const src = new URL("..", import.meta.url).pathname;
  const isSource = (p) => /\.(ts|tsx|mjs|mts)$/.test(p) && !/\.test\.mjs$/.test(p) && !/[\\/]scenarios[\\/]/.test(p) && !p.endsWith("types.gen.ts");
  const speaksOfIt = (text) => /decision-making agent|decision_making|decision-making/i.test(text);
  const files = [
    ...sourceFiles(decodeURIComponent(src), isSource).map((path) => [path, readFileSync(path, "utf8")]),
    ...englishFiles().filter(([path]) => /[\\/]desktop[\\/]/.test(path)),
  ].filter(([, text]) => speaksOfIt(text));
  assert.ok(files.length > 10, "the sources and the catalog speak of it");
  const faults = [];
  for (const [path, text] of files) {
    for (const phrase of ["not an agent", "not a third agent", "the core model", "third permanent object"]) {
      if (text.toLowerCase().includes(phrase)) faults.push(`${path}: “${phrase}”`);
    }
  }
  assert.deepEqual(faults, []);
});

test("an MCP server is registered, tested, checked and edited without a secret ever coming back", () => {
  // Register: a Streamable HTTP server with a bearer header.
  const d = { ...blank(), kind: "http", id: "docs", name: "docs", url: "https://mcp.example.test/mcp", headers: [{ k: "Authorization", v: "Bearer real" }] };
  assert.deepEqual(validate(d, false), {});
  const sent = toTransport(d);
  assert.equal(sent.headers.Authorization, "Bearer real", "the value goes up once");
  // The node answers it masked, with a health nobody asked for yet.
  const answered = { id: "docs", name: "docs", description: "", transport: { ...sent, headers: { Authorization: MASK } }, enabled: true, created_at: 1, health: { state: "unknown" } };
  assert.equal(healthWords(answered.health), "not checked yet");
  assert.equal(healthTone(answered.health), "quiet");
  // Check: the probe answered.
  const checked = { ...answered.health, state: "ok", server: { name: "docs", version: "2.0" }, protocol_version: "2025-06-18", tool_count: 4, checked_at: 1 };
  assert.equal(healthWords(checked), "docs 2.0 · 2025-06-18 · 4 tools");
  // Edit: the URL moves, the header stays masked and goes back as the mask.
  const again = toTransport({ ...blank(), kind: "http", id: "docs", name: "docs", url: "https://mcp.example.test/v2/mcp", headers: [{ k: "Authorization", v: MASK }] });
  assert.equal(again.headers.Authorization, MASK, "the node keeps the stored value");
});

test("the MCP panel keeps its shape: a switch for enabled, a three-way transport picker, one pair editor for env and headers, a test before saving, and no reveal of a secret", () => {
  const panel = readFileSync(new URL("../views/_settings/McpPanel.tsx", import.meta.url), "utf8");
  assert.ok(panel.includes("<Switch") && !panel.includes('server.enabled ? "Disable" : "Enable"'), "enabled is a switch, not a text button");
  assert.ok(panel.includes("<SegmentedControl") && panel.includes("KINDS.map("), "the transport picker offers the three kinds");
  assert.equal((panel.match(/<PairEditor/g) ?? []).length, 2, "one editor for the environment and for the headers");
  assert.ok(panel.includes("api.probeMcp({ transport: toTransport(d) })"), "Test connection dials the draft as typed");
  assert.ok(panel.includes("api.probeMcpById(id)") && panel.includes("Check all"), "a row checks itself, the panel checks every row");
  assert.ok(panel.includes('e.payload.type === "mcp_probed"'), "a probe anywhere refreshes the rows");
  assert.ok(panel.includes("<SecretInput") && panel.includes("stored={isMasked(p.v)}"), "a value is the kit's secret field; the node's mask is its stored state, with nothing to show back");
  assert.ok(!panel.includes('type="password"'), "no bare password box beside the kit's field");
  const refs = readFileSync(new URL("../views/_work/LibraryRefs.tsx", import.meta.url), "utf8");
  assert.ok(refs.includes('m.health?.state === "failing"') && refs.includes("healthWords(m.health)"), "the agent picker says what Settings knows");
});

test("every secret is typed into one kit field, hidden by default, with an eye — and nothing ever asks the node for a secret back", () => {
  const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
  const sites = {
    "../views/_settings/DecisionsPanel.tsx": "<SecretInput",
    "../views/_settings/ConnectorsPanel.tsx": "<SecretInput",
    "../views/_settings/CodeHostPanel.tsx": "<SecretInput",
    "../views/_settings/McpPanel.tsx": "<SecretInput",
    "../views/_settings/SettingControl.tsx": "<SecretInput",
  };
  for (const [file, needle] of Object.entries(sites)) {
    const text = src(file);
    assert.ok(text.includes(needle), `${file} types secrets into the kit's field`);
    assert.ok(!text.includes('type="password"'), `${file} has no bare password box`);
  }
  const kit = src("../ui/SecretInput.tsx");
  assert.ok(kit.includes("useState(false)") && kit.includes("secretView("), "hidden by default; the rules are the model's");
  assert.ok(kit.includes("ICON.inspect") && kit.includes("ICON.hidden") && kit.includes("aria-pressed"), "an eye that says whether it is pressed");
  assert.ok(!kit.includes("localStorage") && !kit.includes("writeSessionDraft"), "a secret is never remembered");
  const decisions = src("../views/_settings/DecisionsPanel.tsx");
  assert.ok(decisions.includes("setBox(keySaved(box, provider));") && decisions.includes("disabled={busy || !maySaveKey(box, provider)}"), "the key stays in its box after Save; Save waits for a change");
  const kept = keySaved(keyTyped(blankKey("jev"), "jev", "not-a-real-key"), "jev");
  assert.deepEqual([kept.typed, maySaveKey(kept, "jev")], ["not-a-real-key", false]);
  assert.deepEqual(keyFor(kept, "rlcd"), blankKey("rlcd"), "and it is its provider's own: another provider's box is empty");
  assert.ok(!/get<[^>]*>\(`\/decisions\/key/.test(src("../api.ts")), "no route reads a key back");
  assert.ok(src("../views/_settings/ConnectorsPanel.tsx").includes("<SecretTextArea") && src("../views/_settings/ConnectorsPanel.tsx").includes("account?.secrets_set"), "a PEM block folds; a set field draws the mask");
  assert.ok(src("../views/_settings/SettingControl.tsx").includes("isSecretSetting(def.key)"), "a proxy URL is typed hidden");
  assert.ok(src("../views/_settings/mcpFormModel.mjs").includes('from "../../ui/secretInputModel.mjs"'), "one mask, the kit's");
});
