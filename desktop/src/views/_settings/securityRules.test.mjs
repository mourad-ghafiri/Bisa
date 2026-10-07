/**
 * Settings › Security's words and shapes, tested where they live. Nothing here
 * renders; what can be wrong in a way a person notices is a rule saved with a
 * pattern that cannot compile, an id two rules share, or a readiness line
 * that lies about the classifier.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { ACTIONS, HOSTS, KEYS, REMEMBERED_REASON, SCALARS, actionWords, appliesWords, blankGuardRule, blankRedactRule, classifierFieldsFor, detectorWords, envDetectorWords, guardRuleProblem, harnessGuardWords, judgeWords, matcherWords, moveRule, offWords, problemLines, readinessLine, redactRuleProblem, ruleWords, slugOf, toggleBuiltin, verdictWords, draftOf, withDraft, withoutDraft, rowKey, keepRowKey } from "./securityRules.mjs";

test("the keys are the registry's twenty, and each panel shows its own scalars", () => {
  const all = [
    ...Object.values(KEYS.redactor),
    ...Object.values(KEYS.guard),
    ...Object.values(KEYS.classifier),
    ...Object.values(KEYS.content),
    ...Object.values(KEYS.net),
  ];
  assert.equal(all.length, 20);
  assert.equal(new Set(all).size, all.length, "each key once");
  for (const k of all) assert.match(k, /^security\.(redactor|guard|classifier|content|net)\./);
  const registry = readFileSync(new URL("../../../../crates/bisa-core/src/settings.rs", import.meta.url), "utf8");
  for (const k of all) assert.ok(registry.includes(`"${k}"`), `${k} is a registered setting`);
  assert.equal(KEYS.classifier.effort, "security.classifier.effort", "how hard the classifier's own model works");
  assert.deepEqual(Object.values(KEYS.content), ["security.content.screen", "security.content.on_harmful"], "what an agent reads from outside: the switch and what a harmful reading does");
  assert.deepEqual(SCALARS.redactor, ["security.redactor.enabled", "security.redactor.env_auto"]);
  assert.deepEqual(SCALARS.net, ["security.net.deny_hosts", "security.net.allow_hosts"]);
  assert.ok(SCALARS.guard.includes("security.guard.terminal_hooks"));
  assert.equal(SCALARS.classifier.length, 3);
  assert.ok(!SCALARS.classifier.includes(KEYS.classifier.agent), "the agent has a picker of its own");
  assert.ok(!SCALARS.classifier.includes(KEYS.classifier.provider), "the provider has a registry row of its own, drawn beside the harness/agent fields it gates");
  assert.deepEqual(ACTIONS, ["allow", "deny", "ask", "classify"]);
});

test("the classifier's own fields follow its provider: only a harness names a harness, a model and its effort", () => {
  assert.deepEqual(classifierFieldsFor("harness"), [KEYS.classifier.harness, KEYS.classifier.model, KEYS.classifier.effort]);
  assert.deepEqual(classifierFieldsFor("agent"), []);
  assert.deepEqual(classifierFieldsFor("decision_making_agent"), [], "the Decision-Making Agent takes no field here: its own are in Settings › Decision Making");
});

test("a redaction rule needs a name, a well-formed id and a pattern or a variable name", () => {
  const blank = blankRedactRule();
  assert.equal(blank.origin, "user");
  assert.match(redactRuleProblem(blank), /give the rule a name/);
  const named = { ...blank, label: "Team key", id: slugOf("Team key") };
  assert.equal(named.id, "team_key");
  assert.match(redactRuleProblem(named), /pattern is needed/);
  assert.equal(redactRuleProblem({ ...named, detector: { kind: "pattern", regex: "TEAM-[0-9]{6}" } }), null);
  assert.match(redactRuleProblem({ ...named, detector: { kind: "pattern", regex: "(" } }), /not a pattern/);
  assert.equal(redactRuleProblem({ ...named, detector: { kind: "env_value", name: "MY_API_KEY" } }), null);
  assert.match(redactRuleProblem({ ...named, detector: { kind: "env_value", name: "my key" } }), /capitals/);
  assert.match(redactRuleProblem({ ...named, id: "Team Key" }), /lowercase/);
  assert.match(redactRuleProblem({ ...named, detector: { kind: "pattern", regex: "x" } }, ["team_key", "team_key"]), /already called/);
});

test("a guard rule needs an action and a matcher that says something", () => {
  const blank = blankGuardRule();
  assert.equal(blank.action, "deny");
  const named = { ...blank, label: "No fake tool", id: "no_fake_tool" };
  assert.match(guardRuleProblem(named), /pattern is needed/);
  assert.equal(guardRuleProblem({ ...named, matcher: { kind: "command", regex: "^fake-tool\\b" } }), null);
  assert.match(guardRuleProblem({ ...named, matcher: { kind: "path", glob: "" } }), /glob/);
  assert.equal(guardRuleProblem({ ...named, matcher: { kind: "path", glob: "~/.ssh/**" } }), null);
  assert.match(guardRuleProblem({ ...named, matcher: { kind: "tool", name: " " } }), /tool's name/);
  assert.equal(guardRuleProblem({ ...named, matcher: { kind: "any" } }), null);
  assert.match(guardRuleProblem({ ...named, action: "maybe", matcher: { kind: "any" } }), /what the rule does/);
  assert.match(guardRuleProblem({ ...named, matcher: { kind: "regexp" } }), /what the rule looks at/);
  // Where it applies is optional, and either host when said; "everywhere" is the absence of a word, never a stored value.
  assert.equal(guardRuleProblem({ ...named, matcher: { kind: "any" }, applies_to: "platform" }), null);
  assert.equal(guardRuleProblem({ ...named, matcher: { kind: "any" }, applies_to: "terminal" }), null);
  assert.equal(guardRuleProblem({ ...named, matcher: { kind: "any" }, applies_to: null }), null);
  assert.match(guardRuleProblem({ ...named, matcher: { kind: "any" }, applies_to: "everywhere" }), /where the rule applies/);
  assert.deepEqual(HOSTS, ["platform", "terminal"]);
  assert.equal(blank.applies_to, undefined, "a new rule applies everywhere until its person says otherwise");
});

test("a rule says where it applies: the platform's agents, a terminal's harnesses, or nothing for everywhere", () => {
  assert.equal(appliesWords({ applies_to: "platform" }), "the platform's agents only");
  assert.equal(appliesWords({ applies_to: "terminal" }), "harnesses in a terminal only");
  assert.equal(appliesWords({}), "");
  assert.equal(appliesWords({ applies_to: null }), "");
  // A built-in that steers an agent to the platform's own tools wears the third phrase; one that protects the machine does not.
  const steering = { action: "deny", matcher: { kind: "command", regex: "--headless" }, applies_to: "platform" };
  assert.equal(ruleWords(steering), "refused — the agent hears why · the platform's agents only · commands matching --headless", "the scope before the pattern, which is what a row truncates");
  const protecting = { action: "deny", matcher: { kind: "path", glob: "~/.ssh/**" } };
  assert.equal(ruleWords(protecting), "refused — the agent hears why · paths matching ~/.ssh/**");
});

test("rules move one place and stay put at the ends; built-ins toggle by id", () => {
  const list = ["a", "b", "c"];
  assert.deepEqual(moveRule(list, 0, 1), ["b", "a", "c"]);
  assert.deepEqual(moveRule(list, 2, 1), list, "the last cannot move down");
  assert.deepEqual(moveRule(list, 0, -1), list, "the first cannot move up");
  assert.deepEqual(moveRule(list, 5, -1), list);
  assert.deepEqual(toggleBuiltin([], "publishing", false), ["publishing"]);
  assert.deepEqual(toggleBuiltin(["publishing", "dotenv"], "publishing", true), ["dotenv"]);
  assert.deepEqual(toggleBuiltin(["dotenv"], "dotenv", false), ["dotenv"], "switching off twice is once");
});

test("verdicts, actions, matchers and detectors read as phrases", () => {
  assert.deepEqual(verdictWords("deny"), { tone: "danger", text: "refused" });
  assert.deepEqual(verdictWords("denied"), { tone: "danger", text: "refused" });
  assert.deepEqual(verdictWords("ask"), { tone: "warn", text: "asks you" });
  assert.deepEqual(verdictWords("classify"), { tone: "warn", text: "classifier reads it" });
  assert.deepEqual(verdictWords("allowed"), { tone: "ok", text: "allowed" });
  assert.deepEqual(verdictWords("fallthrough"), { tone: "quiet", text: "no rule applies" });
  assert.equal(actionWords("ask"), "put to you in the Inbox");
  assert.equal(matcherWords({ kind: "path", glob: "**/.env*" }), "paths matching **/.env*");
  assert.equal(matcherWords({ kind: "tool", name: "WebFetch" }), "the tool WebFetch");
  assert.equal(matcherWords({ kind: "any" }), "every call");
  assert.equal(detectorWords({ kind: "env_value", name: "MY_KEY" }), "the value of MY_KEY");
  assert.equal(detectorWords({ kind: "pattern", regex: "x+" }), "pattern x+");
});

test("a harness is judged before it runs, judged and rewritable, or only observed", () => {
  assert.equal(harnessGuardWords({ tool_guard: true, input_rewrite: true }).text, "judged before it runs · restores placeholders");
  assert.equal(harnessGuardWords({ tool_guard: true, input_rewrite: false }).text, "judged before it runs");
  const observed = harnessGuardWords({ tool_guard: false, input_rewrite: false });
  assert.equal(observed.tone, "quiet");
  assert.match(observed.text, /^observed only/);
  assert.match(observed.text, /cannot veto/);
});

test("a feature switched off is a banner, never a silence", () => {
  assert.equal(offWords("redactor").tone, "warn");
  assert.match(offWords("redactor").text, /^Off: nothing is redacted on this node/);
  assert.equal(offWords("guard").tone, "warn");
  assert.match(offWords("guard").text, /^Off: no tool call is judged on this node/);
});

test("the environment detectors read as a count, never a name or a value", () => {
  assert.equal(envDetectorWords(null).tone, "quiet");
  assert.match(envDetectorWords({ env_auto: false, env_detectors: 3 }).text, /not read/);
  assert.match(envDetectorWords({ env_auto: true, env_detectors: 0 }).text, /^No variable/);
  const one = envDetectorWords({ env_auto: true, env_detectors: 1 });
  assert.equal(one.tone, "ok");
  assert.equal(one.text, "1 value of this node's environment is recognised wherever it appears.");
  assert.equal(envDetectorWords({ env_auto: true, env_detectors: 12 }).text, "12 values of this node's environment are recognised wherever they appear.");
});

test("a decision names its judge, and a remembered answer says so", () => {
  assert.equal(judgeWords({ by: "rule", rule: "dotenv" }), "dotenv");
  assert.equal(judgeWords({ by: "rule", rule: null }), "rule");
  assert.equal(judgeWords({ by: "classifier", rule: "publishing" }), "classifier");
  assert.equal(judgeWords({ by: "person", rule: "ask_push", reason: null }), "you");
  assert.equal(judgeWords({ by: "person", rule: "ask_push", reason: REMEMBERED_REASON }), "you · remembered");
});

test("the readiness line says ready, why not, or off — and never guesses before the status arrives", () => {
  assert.equal(readinessLine(null).tone, "quiet");
  const ready = readinessLine({ classifier: { enabled: true, agent: "general-agent", deadline_secs: 20 }, classifier_ready: true });
  assert.deepEqual(ready, { tone: "ok", text: "general-agent answers within 20s — ready." });
  const missing = readinessLine({
    classifier: { enabled: true, agent: "reviewer", deadline_secs: 20 },
    classifier_ready: false,
    classifier_note: 'agent "reviewer" is missing or disabled',
  });
  assert.equal(missing.tone, "warn");
  assert.match(missing.text, /missing or disabled — a classify rule asks you instead/);
  assert.match(readinessLine({ classifier: { enabled: false, agent: "x", deadline_secs: 5 }, classifier_ready: true }).text, /^Off/);
  assert.deepEqual(
    problemLines([{ feature: "guard", rule: "g", reason: "bad glob" }, { feature: "redactor", rule: "r", reason: "bad regex" }], "redactor"),
    ["r: bad regex"],
  );
});

test("a rule list's draft is of its scope: the picker moving shows the other scope's rules, and Save never writes one scope's draft over another's list", () => {
  const workspace = [{ id: "a", label: "A" }];
  const machine = [{ id: "m", label: "M" }];
  let drafts = {};
  assert.deepEqual(draftOf(drafts, "workspace", workspace), { rules: workspace, dirty: false });
  // A rule is added at the workspace.
  const edited = [...workspace, { id: "b", label: "B" }];
  drafts = withDraft(drafts, "workspace", edited);
  assert.deepEqual(draftOf(drafts, "workspace", workspace), { rules: edited, dirty: true });
  // The picker moves to this machine: its own rules, clean — the workspace's draft is not what Save would write here.
  assert.deepEqual(draftOf(drafts, "machine", machine), { rules: machine, dirty: false });
  // And back: the draft is still there.
  assert.deepEqual(draftOf(drafts, "workspace", workspace).rules, edited);
  // Saved, or discarded: gone for that scope alone.
  drafts = withDraft(drafts, "machine", []);
  const saved = withoutDraft(drafts, "workspace");
  assert.deepEqual(saved, { machine: [] });
  assert.equal(withoutDraft(saved, "workspace"), saved, "nothing to drop: the same object");
  assert.deepEqual(drafts, { workspace: edited, machine: [] }, "the drafts handed in are not changed");
  // The panel: Save writes the scope and the list that were on screen when it was pressed, and Discard drops the draft rather than marking the stored list edited.
  const panel = readFileSync(new URL("./SecurityPanels.tsx", import.meta.url), "utf8");
  const rules = panel.slice(panel.indexOf("function UserRules<"), panel.indexOf("return (", panel.indexOf("function UserRules<")));
  assert.ok(rules.includes("const { rules: draft, dirty } = draftOf(drafts, scope, rules);"));
  assert.ok(rules.includes("const at = scope;") && rules.includes("const list = draft;") && rules.includes("api.setSettings(at, { [settingKey]: list })"));
  assert.ok(!rules.includes("useEffect("), "no effect copies a scope's rules into a draft that is another scope's");
  assert.ok(panel.includes("onDiscard={() => setDrafts((all) => withoutDraft(all, scope))}"), "Discard, in the footer every explicit-save form shares");
});

test("a rule row keeps its key while its label, id and place move — the box being typed in keeps its caret", () => {
  const a = { id: "a", label: "A" };
  const b = { id: "b", label: "B" };
  assert.equal(rowKey(a), rowKey(a), "asked twice, one key");
  assert.notEqual(rowKey(a), rowKey(b), "two rules, two rows");
  const typed = keepRowKey(a, { ...a, id: "ab", label: "AB" });
  assert.equal(rowKey(typed), rowKey(a), "the label and the id it follows changed; the row did not");
  const moved = moveRule([a, b], 0, 1);
  assert.deepEqual(moved.map(rowKey), [rowKey(b), rowKey(a)], "Up and Down move rows, not keys");
  assert.equal(rowKey(blankRedactRule()) === rowKey(blankRedactRule()), false, "every new rule is its own row");
  const panel = readFileSync(new URL("./SecurityPanels.tsx", import.meta.url), "utf8");
  assert.ok(panel.includes("key={rowKey(r)}") && !panel.includes("key={i}"), "rows are keyed by the rule, never the index");
  assert.ok(!panel.includes('label=""'), "every switch has a name");
});

