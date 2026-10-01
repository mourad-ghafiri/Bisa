/**
 * Every body the desktop sends fits the type the node reads it as.
 *
 * The node refuses a body carrying a key its type does not declare — a 400,
 * where it once dropped the key in silence. TypeScript holds a **fresh object
 * literal** to the generated type and nothing else: a body built with a
 * spread, a draft passed whole, a variable of a wider type or JSON that came
 * from elsewhere passes `tsc` and is refused at run time, and no test runs
 * the window. So a body is built from named keys by a function of a model,
 * and this guard holds what the models build to `api-schema.json` — the file
 * the types are generated from — with a validator written by hand
 * (`schemaFit.mjs`). Two facts are held:
 *
 * 1. what each model builds fits its definition, the ordinary body and one
 *    built from a draft that carries more;
 * 2. every call site that hands a body through `api.ts` hands it a literal
 *    of named keys or what a model built — or is listed here with where its
 *    body is built.
 *
 * Every body a screen sends has a definition: the node holds that on its
 * side (`layering::every_body_is_in_the_schema_the_desktop_is_held_to`).
 *
 * A source guard and a model test, no DOM. Run with
 * `node --test --import ./src/i18n/preload.mjs src/scenarios/bodiesFitTheSchema.test.mjs`
 * from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { sourceFiles } from "../testWalk.mjs";
import { definitionsOf, misfits, refusesUnknownKeys } from "./schemaFit.mjs";

import { decideBody } from "../askModel.mjs";
import { drawnResult, refusedResult, snapshotResult } from "../draw/drawRequestModel.mjs";
import { persistedAppState } from "../draw/drawModel.mjs";
import { EMPTY_BROWSERS, PERSON, openBrowser, titled } from "../shell/browsersModel.mjs";
import { PAGE_SILENT, answerResult, navigatedResult, screenshotResult, tabsResult, wireResult } from "../shell/browserBridgeModel.mjs";
import { fixBody } from "../shell/setupModel.mjs";
import { withoutMember } from "../views/rosterModel.mjs";
import { tryRequest } from "../views/_settings/decisionsModel.mjs";
import { accountBody, definitionOf, emptyDefinition } from "../views/_settings/connectorsModel.mjs";
import { specBody } from "../views/_settings/gitProfilesModel.mjs";
import { blank as blankServer, toTransport } from "../views/_settings/mcpFormModel.mjs";
import { allowBody, denyBody } from "../views/_studio/conversationAskModel.mjs";
import { closeBody } from "../views/_goal/goalPageModel.mjs";
import { draftOf, emptyDraft, saveOf } from "../views/_work/agentDraftModel.mjs";
import { diffWrites } from "../views/_work/gitConfigModel.mjs";
import { governanceBody } from "../views/_work/governanceModel.mjs";
import { goalBody } from "../views/_work/newGoalModel.mjs";
import { prRequest } from "../views/_work/prFormModel.mjs";
import { creationBody } from "../views/_work/projectForm.mjs";
import { planOf as rebasePlanOf } from "../views/_work/rebaseEditorModel.mjs";
import { planOf as retirePlanOf } from "../views/_work/retireModel.mjs";
import { renameBody } from "../views/_work/workstreamCardModel.mjs";
import { openBody, sourceBody } from "../views/_work/workstreamCreation.mjs";
import { placeBody } from "../views/_board/boardModel.mjs";
import { messageBody } from "../views/_workbench/editorAgentModel.mjs";
import { fileSettleTarget, hunkSettleTarget } from "../views/_workbench/reviewLensModel.mjs";
import { definitionBody, putBody } from "../views/_workflow/designerSession.mjs";
import { toggleInput, withFixed } from "../views/_workflow/forms/assigneeRefModel.mjs";
import { addBoundary } from "../views/_workflow/forms/boundaryModel.mjs";
import { give, onWorkflow } from "../views/_workflow/forms/spawnStepModel.mjs";
import { setStartEvent } from "../views/_workflow/forms/startForm.mjs";
import { againBody, turnOnBody } from "../views/_workflow/listeningModel.mjs";
import { runRequest } from "../views/_workflow/runDialogModel.mjs";
import { BOUNDARY_EVENTS, START_EVENTS, START_STEP_ID, STEP_KINDS, WAITS, blankStep, blankWait, blankWorkflow, mayCarryBoundaries } from "../views/_workflow/stepKinds.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const SRC = join(HERE, "..");
const ROOT = join(SRC, "..", "..");
const read = (path) => readFileSync(join(ROOT, path), "utf8");
const SCHEMA = JSON.parse(read("desktop/api-schema.json"));

/**
 * The shapes the core holds to their keys **by hand**: serde cannot refuse an
 * unknown key of a shape it flattens, so each is checked where it is read
 * (`refuse_unknown_keys`), and the schema cannot say so. Each with the word
 * the core's refusal names it by.
 */
const HELD_BY_HAND = Object.freeze({
  Step: "step",
  InputDef: "input",
  StartOn: "start event",
  WaitFor: "wait",
  Boundary: "boundary event",
  BoundaryOn: "boundary event's `on`",
});

const fits = (name, body) => misfits(SCHEMA, name, body, { closed: Object.keys(HELD_BY_HAND) });

// ---------------------------------------------------------------------------
// The validator itself
// ---------------------------------------------------------------------------

/** A schema file of the test's own: one definition of each shape the validator reads. */
const OWN = {
  definitions: {
    Strict: {
      type: "object",
      additionalProperties: false,
      required: ["name"],
      properties: {
        name: { type: "string" },
        note: { type: ["string", "null"], default: null },
        count: { type: "integer", format: "uint8", minimum: 0 },
        tags: { type: "array", items: { type: "string" }, default: [] },
        pair: { type: "array", items: [{ type: "string" }, { type: "integer" }], minItems: 2, maxItems: 2 },
        mode: { $ref: "#/definitions/Mode" },
        nested: { anyOf: [{ $ref: "#/definitions/Nested" }, { type: "null" }] },
        free: { type: "object", additionalProperties: true },
        map: { type: "object", additionalProperties: { type: "string" } },
        anything: {},
        named: { type: "object", patternProperties: { "^[a-z]+$": { type: "integer" } }, additionalProperties: false },
        defaulted: { default: "auto", allOf: [{ $ref: "#/definitions/Mode" }] },
        id: { type: "string", pattern: "^[0-9A-Z]{4}$" },
      },
    },
    Mode: { type: "string", enum: ["auto", "guided", "manual"] },
    Nested: { type: "object", additionalProperties: false, required: ["x"], properties: { x: { type: "integer", format: "int32" } } },
    Lenient: { type: "object", required: ["sha256"], properties: { sha256: { type: "string" } } },
    Tagged: {
      oneOf: [
        { type: "object", additionalProperties: false, required: ["kind"], properties: { kind: { type: "string", const: "new" }, slug: { type: "string" } } },
        { type: "object", additionalProperties: false, required: ["kind", "url"], properties: { kind: { type: "string", const: "clone" }, url: { type: "string" } } },
      ],
    },
    Flattened: {
      type: "object",
      required: ["id"],
      properties: { id: { type: "string" } },
      oneOf: [
        { type: "object", required: ["kind", "prompt"], properties: { kind: { type: "string", const: "human" }, prompt: { type: "string" } } },
        { type: "object", required: ["kind"], properties: { kind: { type: "string", const: "end" } } },
      ],
    },
  },
};
const own = (name, body, closed = []) => misfits(OWN, name, body, { closed });

test("the validator: a body fits when every key is declared, of its type, and nothing the type needs is missing", () => {
  assert.deepEqual(own("Strict", { name: "a" }), []);
  assert.deepEqual(own("Strict", { name: "a", note: null, count: 3, tags: ["x"], pair: ["k", 1], mode: "guided", nested: { x: -4 }, free: { any: [1, { deep: true }] }, map: { k: "v" }, anything: [1, "two"], named: { abc: 1 }, defaulted: "manual", id: "01AB" }), []);
  assert.deepEqual(own("Strict", { name: "a", nested: null, note: undefined }), [], "a key left undefined is a key that is absent: the wire never carries it");
  assert.deepEqual(own("Strict", { name: "a", extra: 1 }), ["$.extra: a key `Strict` does not declare"]);
  assert.deepEqual(own("Strict", {}), ["$: `name` is missing"]);
  assert.deepEqual(own("Strict", { name: 7 }), ["$.name: integer where string is taken"]);
  assert.deepEqual(own("Strict", { name: "a", note: 7 }), ["$.note: integer where string or null is taken"]);
  assert.deepEqual(own("Strict", []), ["$: array where object is taken"]);
  assert.deepEqual(own("Strict", null), ["$: null where object is taken"]);
  assert.deepEqual(own("Missing", {}), ["the schema defines no `Missing`"]);
});

test("the validator: numbers, lists, tuples, words and patterns are held to what the schema says of them", () => {
  assert.deepEqual(own("Strict", { name: "a", count: 1.5 }), ["$.count: number where integer is taken"]);
  assert.deepEqual(own("Strict", { name: "a", count: -1 }), ["$.count: -1 is under 0", "$.count: -1 is no uint8"]);
  assert.deepEqual(own("Strict", { name: "a", count: 256 }), ["$.count: 256 is no uint8"]);
  assert.deepEqual(own("Strict", { name: "a", nested: { x: 2147483648 } })[0], "$.nested: fits none of the 2 shapes taken here");
  assert.deepEqual(own("Strict", { name: "a", tags: ["x", 7] }), ["$.tags[1]: integer where string is taken"]);
  assert.deepEqual(own("Strict", { name: "a", pair: ["k"] }), ["$.pair: 1 items where at least 2 are taken"]);
  assert.deepEqual(own("Strict", { name: "a", pair: ["k", "v"] }), ["$.pair[1]: string where integer is taken"]);
  assert.deepEqual(own("Strict", { name: "a", mode: "interactive" }), ['$.mode: "interactive" is none of "auto", "guided", "manual"']);
  assert.deepEqual(own("Strict", { name: "a", defaulted: "off" }), ['$.defaulted: "off" is none of "auto", "guided", "manual"'], "a default beside a reference says nothing of the value");
  assert.deepEqual(own("Strict", { name: "a", map: { k: 7 } }), ["$.map.k: integer where string is taken"]);
  assert.deepEqual(own("Strict", { name: "a", named: { Abc: 1 } }), ["$.named.Abc: a key the type does not declare"]);
  assert.deepEqual(own("Strict", { name: "a", id: "01ab" }), ['$.id: "01ab" does not read as ^[0-9A-Z]{4}$']);
});

test("the validator: a nested type is held to its own keys, a lenient one to what it needs, a choice to the shape its tag names", () => {
  assert.deepEqual(own("Strict", { name: "a", nested: { x: 1, y: 2 } }), ["$.nested: fits none of the 2 shapes taken here", "$.nested.y: a key `Nested` does not declare"]);
  assert.deepEqual(own("Lenient", { sha256: "ab", preview: "blob:1" }), [], "a type the node does not hold to its keys takes what it does not know");
  assert.deepEqual(own("Lenient", { preview: "blob:1" }), ["$: `sha256` is missing"]);
  assert.deepEqual(own("Tagged", { kind: "clone", url: "https://x.example" }), []);
  assert.deepEqual(own("Tagged", { kind: "new", slug: "web", url: "https://x.example" }), ["$: fits none of the 2 shapes taken here", "$.url: a key the type does not declare"], "the shape its tag names says why");
  assert.deepEqual(own("Tagged", { kind: "attach" })[0], "$: fits none of the 2 shapes taken here");
  assert.equal(refusesUnknownKeys(OWN, "Strict"), true);
  assert.equal(refusesUnknownKeys(OWN, "Tagged"), true, "every shape of the choice refuses");
  assert.equal(refusesUnknownKeys(OWN, "Lenient"), false);
  assert.equal(refusesUnknownKeys(OWN, "Flattened"), false, "the schema cannot say what the core checks by hand");
});

test("the validator: a shape held by hand knows its own keys and the keys of the kind it is, and nothing else", () => {
  assert.deepEqual(own("Flattened", { id: "a", kind: "human", prompt: "?", selected: true }), [], "the schema alone takes it");
  assert.deepEqual(own("Flattened", { id: "a", kind: "human", prompt: "?", selected: true }, ["Flattened"]), ["$.selected: a key `Flattened` does not declare"]);
  assert.deepEqual(own("Flattened", { id: "a", kind: "end", prompt: "?" }, ["Flattened"]), ["$.prompt: a key `Flattened` does not declare"], "a key of another kind is no key of this one");
  assert.deepEqual(own("Flattened", { id: "a", kind: "human", prompt: "?" }, ["Flattened"]), []);
  assert.deepEqual(own("Flattened", { id: "a", kind: "human" }, ["Flattened"])[0], "$: fits none of the 2 shapes taken here");
});

// ---------------------------------------------------------------------------
// The schema file, and what the core holds by hand
// ---------------------------------------------------------------------------

/** Every keyword the validator reads, and the ones that say nothing about a value. */
const READ = Object.freeze(["$ref", "type", "properties", "required", "additionalProperties", "patternProperties", "items", "minItems", "maxItems", "enum", "const", "anyOf", "oneOf", "allOf", "minimum", "maximum", "pattern", "format"]);
const SILENT = Object.freeze(["description", "default", "title", "$schema", "definitions"]);

test("the schema file uses no keyword the validator does not read: a new one is a rule to add here, never one passed over", () => {
  const met = new Set();
  const walk = (node) => {
    if (Array.isArray(node)) return node.forEach(walk);
    if (node === null || typeof node !== "object") return;
    for (const [key, value] of Object.entries(node)) {
      met.add(key);
      // The names under these are a body's keys, or definitions' names — not keywords.
      if (key === "properties" || key === "patternProperties" || key === "definitions") Object.values(value).forEach(walk);
      else if (key !== "enum" && key !== "const" && key !== "default" && key !== "required") walk(value);
    }
  };
  walk(SCHEMA);
  assert.deepEqual([...met].filter((k) => !READ.includes(k) && !SILENT.includes(k)).sort(), []);
  const formats = new Set();
  JSON.stringify(SCHEMA, (key, value) => (key === "format" && typeof value === "string" ? (formats.add(value), value) : value));
  assert.deepEqual([...formats].filter((f) => /int/.test(f) && misfits({ definitions: { N: { type: "integer", format: f } } }, "N", 1).length > 0), [], "every integer format is one the validator bounds");
  assert.ok(Object.keys(definitionsOf(SCHEMA)).length > 600, "the definitions are read");
});

test("the shapes held by hand are the ones the core checks where it reads them, each named as its refusal names it", () => {
  const core = ["workflow.rs", "start.rs", "boundary.rs"].map((file) => read(`crates/bisa-core/src/${file}`)).join("\n");
  const named = [...core.matchAll(/refuse_unknown_keys::<[^>]+>\(\s*&?\w+,\s*"([^"]+)"/g)].map((m) => m[1]).sort();
  assert.deepEqual(named, Object.values(HELD_BY_HAND).sort());
  for (const name of Object.keys(HELD_BY_HAND)) {
    assert.ok(name in definitionsOf(SCHEMA), `${name} is a definition`);
    assert.equal(refusesUnknownKeys(SCHEMA, name), false, `${name}: were the schema to say it, the row here would go`);
  }
});

// ---------------------------------------------------------------------------
// What the models build
// ---------------------------------------------------------------------------

const shot = { sha256: "a".repeat(64), name: "shot.png", mime: "image/png", size: 10 };
/** What a draft, a row or a page may carry beside what the wire takes. */
const STRAY = { selected: true, touched: 3, open: ["a"], preview: "blob:1" };

let tabs = openBrowser(EMPTY_BROWSERS, { by: PERSON, home: { scope: "workstream", id: "w1" }, url: "http://localhost:5173/" });
tabs = titled(tabs, "b1", "Home");
const tab = tabs.sessions[0];
const scroll = { x: 0, y: 1200, width: 1280, height: 4800 };
const said = { ok: true, text: "Hello", selector: "#go", count: 7, waitedMs: 340, scroll, value: { n: [1, "two", null] }, dialogs: [{ kind: "confirm", message: "Sure?", answer: "true" }, { kind: "alert", message: "Hi", answer: null }], console: [{ level: "error", text: "boom", at: 12 }] };

const agent = { id: "reviewer", name: "Reviewer", description: "Reads a diff.", system_prompt: "You review.", harness: "claude-code", models: { strategy: "fallback", effort: "high", models: [{ model: "claude-opus-5-5[1m]", weight: 1, enabled: true, effort: "auto" }, { model: "claude-sonnet-5-5[1m]", weight: 1, enabled: true }] }, decision_making: true, respond: "owner_only", skills: ["code-review"], mcps: ["github"], tags: ["review"], photo: shot };

/** A workflow of every kind of step as the palette births it, every start event and wait as the forms set them, a boundary of every event. */
function everyShape() {
  const steps = STEP_KINDS.map((k) => blankStep(k.kind, k.kind === "start" ? START_STEP_ID : `a-${k.kind}`));
  for (const { event } of START_EVENTS) steps.push({ ...setStartEvent(blankStep("start", `on-${event}`), event) });
  for (const { until } of WAITS) steps.push({ ...blankStep("wait", `until-${until}`), until: blankWait(until) });
  let carrier = { ...blankStep("human", "carrier"), position: { x: 320, y: -40 } };
  assert.ok(mayCarryBoundaries(carrier));
  for (const { event } of BOUNDARY_EVENTS) carrier = addBoundary(carrier, event);
  steps.push(carrier);
  return { ...blankWorkflow("Every shape"), inputs: [{ name: "audience", label: "Audience", kind: "text", required: true }, { name: "tier", label: "Tier", kind: "choice", options: ["a", "b"], default: "a" }, { name: "account", label: "Account", kind: "account", connector: null }], steps, tags: ["ops"], decision_making: true };
}

/** A spawn step as its form leaves it: the child picked, what it is given, who carries it — fixed assignees and an input of the run. */
function spawning() {
  const asked = [{ name: "report", label: "Report", kind: "text", required: true }, { name: "project", label: "Project", kind: "project", required: true }];
  let step = onWorkflow(blankStep("spawn", "followup"), "01ARZ3NDEKTSV4RRFFQ69G5FAV", asked);
  step = give(give(step, "report", "{inputs.report}"), "project", "{inputs.project}");
  step = { ...step, assignees: toggleInput(withFixed(step.assignees, ["agent:reviewer", `human:${"ab".repeat(32)}`, "team:01ARZ3NDEKTSV4RRFFQ69G5FAW"]), "owner") };
  const wf = blankWorkflow("Open a follow-up");
  return { ...wf, inputs: [...asked, { name: "owner", label: "Owner", kind: "assignee", required: false }], steps: [{ ...wf.steps[0], then: [{ to: "followup" }] }, step] };
}

const scheduled = { ...blankWorkflow("Weekly"), inputs: [{ name: "audience", label: "Audience", kind: "text", required: true }], steps: [setStartEvent(blankStep("start", START_STEP_ID), "schedule")] };
const test_run = runRequest(scheduled, START_STEP_ID, { audience: "the team" }, '{ "at": 1000 }');
const rebaseRows = [{ id: "abc1234", action: "pick", message: "one" }, { id: "def5678", action: "reword", message: "two, said better" }, { id: "0a1b2c3", action: "squash", message: "" }];
const profile = { label: "Acme", host: "github.com", owner: "acme", aliases: [], name: "Ada", email: "ada@acme.example", ssh_key: "", account: "" };
const project = { provenance: "clone", placement: "copy", slug: "web", name: "Web", publish: "gated", tags: ["frontend"], gitConfig: { "user.name": "Ada" }, url: "https://example.com/acme/web.git", path: "/Users/ada/web" };
const stdio = { ...blankServer(), id: "files", name: "files", command: "npx", args: "-y\nserver", env: [{ k: "TOKEN", v: "••••••" }], cwd: "/srv" };
const http = { ...blankServer(), id: "remote", name: "remote", kind: "http", url: "https://mcp.example/mcp", headers: [{ k: "Authorization", v: "Bearer x" }] };
const question = { gate_id: "01GATE", step: "review", home: { home: "goal", goal: "01GOAL" }, expects: { kind: "answer", multi: true, options: [{ id: "a", label: "A" }, { id: "b", label: "B" }] } };
const gate = { gate_id: "01GATE", home: { home: "run", run: "01RUN" }, expects: { kind: "decision" } };

/**
 * One row per body a model builds: the definition it is read as, what built
 * it, and the body — the ordinary one, and one built from a draft that
 * carries more than the wire takes.
 * @type {[string, string, unknown][]}
 */
const BODIES = [
  ["NewGoalBody", "newGoalModel.goalBody", goalBody({ statement: " Ship the report ", mode: "guided", assignees: [], tags: [], documents: [] })],
  ["NewGoalBody", "newGoalModel.goalBody, everything set", goalBody({ statement: "Ship", mode: "auto", assignees: ["team:01TEAM"], tags: ["ops"], documents: [{ ...shot, ...STRAY }], ...STRAY })],
  ["GoalPlan", "retireModel.planOf, a goal", retirePlanOf("goal", { thing: "archive", projects: "keep", tree: true, ...STRAY })],
  ["WorkflowPlan", "retireModel.planOf, a workflow", retirePlanOf("workflow", { thing: "delete", projects: "delete", tree: true })],
  ["DecideBody", "askModel.decideBody, a gate approved with inputs", decideBody(gate, { approve: true, text: "looks right", inputs: { audience: "the team" }, ...STRAY })],
  ["DecideBody", "askModel.decideBody, a gate declined", decideBody(gate, { approve: false, inputs: { audience: "x" } })],
  ["DecideBody", "askModel.decideBody, a question answered", decideBody(question, { selected: ["a", "b", "z"], text: " both ", ...STRAY })],
  ["DecideBody", "askModel.decideBody, not sure", decideBody(question, { selected: ["a"], unsure: true })],
  ["ConversationAskAnswer", "conversationAskModel.allowBody", allowBody({ grantable: true, ...STRAY }, "conversation")],
  ["ConversationAskAnswer", "conversationAskModel.allowBody, once", allowBody({ grantable: false }, "conversation")],
  ["ConversationAskAnswer", "conversationAskModel.denyBody", denyBody(" run the tests instead ")],
  ["ConversationAskAnswer", "conversationAskModel.denyBody, no note", denyBody("")],
  ["ListeningBody", "listeningModel.turnOnBody", turnOnBody({ audience: "the team" }, { max_usd_cents: 500, max_tokens: null, max_wall_clock_secs: 90.5, ...STRAY })],
  ["ListeningBody", "listeningModel.turnOnBody, nothing asked", turnOnBody({}, null)],
  ["ListeningBody", "listeningModel.againBody", againBody({ inputs: { audience: "x" }, budget: { max_tokens: 1000 }, since: 1, paused: { reason: "failed" } })],
  ["StartRunBody", "runDialogModel.runRequest, a test run", test_run.kind === "test" ? test_run.body : null],
  ["NewMessageBody", "editorAgentModel.messageBody", messageBody({ mode: "edit", agentId: "reviewer", text: "tighten this", target: "the selection main.rs:12–18", chips: [{ kind: "file", path: "src/main.rs" }], ...STRAY })],
  ["NewAgentBody", "agentDraftModel.saveOf, a new agent", saveOf({ ...emptyDraft("claude-code"), name: "Scout", system_prompt: "You scout.", ...STRAY }, null, false).body],
  ["NewAgentBody", "agentDraftModel.saveOf, a new agent with everything", saveOf({ ...draftOf(agent), ...STRAY }, null, false).body],
  ["PatchAgentBody", "agentDraftModel.saveOf, an agent of yours", saveOf({ ...draftOf(agent), ...STRAY }, agent, false).body],
  ["PatchAgentBody", "agentDraftModel.saveOf, a core agent", saveOf({ ...draftOf(agent), ...STRAY }, agent, true).body],
  ["PatchAgentBody", "setupModel.fixBody, an agent moved to a harness", fixBody({ kind: "agent_harness", label: "Move it", agent: "general-agent", harness: "claude-code", models: agent.models }).body],
  ["SettingsWrite", "setupModel.fixBody, settings written", { values: fixBody({ kind: "settings", label: "Set it", set: { "decisions.provider": "harness", "decisions.harness": "claude-code" } }).set }],
  ["NewWorkflowBody", "stepKinds.blankWorkflow through designerSession.definitionBody", definitionBody(blankWorkflow("Untitled workflow"))],
  ["NewWorkflowBody", "designerSession.definitionBody, every kind, event, wait and boundary the forms birth", definitionBody({ ...everyShape(), ...STRAY, id: "01WF", revision: 3, origin: { origin: "workspace" } })],
  ["NewWorkflowBody", "spawnStepModel.give and onWorkflow, assigneeRefModel.withFixed and toggleInput, through designerSession.definitionBody", definitionBody(spawning())],
  ["PutWorkflowBody", "designerSession.putBody", putBody({ ...everyShape(), ...STRAY, id: "01WF", author: "ab".repeat(32), created_at: 1 }, 7)],
  ["SetWorkflowBody", "designerSession.definitionBody, a goal's own design", { definition: definitionBody({ ...everyShape(), ...STRAY }), revision: 2 }],
  ["ProfileSpec", "gitProfilesModel.specBody", specBody({ ...profile, slug: "acme", file: "/x", globs: ["a"], ...STRAY }, "github-acme, gh-acme")],
  ["ConnectorDefinition", "connectorsModel.emptyDefinition", emptyDefinition()],
  ["ConnectorDefinition", "connectorsModel.definitionOf, a record", definitionOf({ ...emptyDefinition(), origin: "local", created_at: 1700000000, ...STRAY })],
  ["NewConnectorAccount", "connectorsModel.accountBody, an account added", accountBody({ label: " work ", values: { site: "acme", blank: "" }, secrets: { username: "ada", password: "" }, ...STRAY }, null)],
  ["NewConnectorAccount", "connectorsModel.accountBody, an account edited with no secret typed", accountBody({ label: "work", values: { port: "27124" }, secrets: {}, ...STRAY }, { id: "01ARZ3NDEKTSV4RRFFQ69G5FAV", connector: "obsidian", label: "work", params: { port: 27124 }, default: true, secrets_set: ["token"], token_source: "file", ...STRAY })],
  ["NewProjectBody", "projectForm.creationBody, a clone", creationBody({ ...project, ...STRAY })],
  ["NewProjectBody", "projectForm.creationBody, a new project", creationBody({ ...project, provenance: "new", gitConfig: null })],
  ["NewProjectBody", "projectForm.creationBody, an import", creationBody({ ...project, provenance: "import" })],
  ["NewProjectBody", "projectForm.creationBody, an adoption", creationBody({ ...project, provenance: "import", placement: "link" })],
  ["PrBody", "prFormModel.prRequest", prRequest({ draft_prs: true, reviewers: true, labels: true }, { title: " Fix it ", body: "why", draft: true, reviewers: "@ada, bob", labels: "bug", ...STRAY })],
  ["PrBody", "prFormModel.prRequest, a host that takes a title and a body", prRequest(null, { title: "Fix it", body: "", draft: true, reviewers: "ada", labels: "bug" })],
  ["GitRebasePlan", "rebaseEditorModel.planOf", rebasePlanOf(rebaseRows.map((r) => ({ ...r, ...STRAY, subject: "x" })), "origin/main", null)],
  ["NewWorkstreamBody", "workstreamCreation.sourceBody, a new branch", { label: "Fix", source: sourceBody({ kind: "new", name: " fix/it ", start: "" }).source, base: "main" }],
  ["NewWorkstreamBody", "workstreamCreation.sourceBody, a local branch", { source: sourceBody({ kind: "branch", branch: "feat/a" }).source }],
  ["NewWorkstreamBody", "workstreamCreation.sourceBody, a remote branch", { source: sourceBody({ kind: "remote", remote: "origin", branch: "feat/a" }).source }],
  ["NewWorkstreamBody", "workstreamCreation.sourceBody, a tag made first", { source: sourceBody({ kind: "tag", newTag: true, tagName: "v1.2.0", tagAt: "main", branch: "" }).source }],
  ["NewWorkstreamBody", "workstreamCreation.sourceBody, a pull request", { source: sourceBody({ kind: "pr", pr: 42, head: "feat/a" }).source }],
  ["NewWorkstreamBody", "workstreamCreation.openBody, a derived branch with nothing typed", openBody({ git: true, label: "", fields: { kind: "new", name: "", start: "" }, base: "main", defaultBranch: "main" }).body],
  ["NewWorkstreamBody", "workstreamCreation.openBody, a new branch with a label, a name, a start and a base", openBody({ git: true, label: " Fix ", fields: { kind: "new", name: "fix/it", start: "v1", ...STRAY }, base: "release", defaultBranch: "main", ...STRAY }).body],
  ["NewWorkstreamBody", "workstreamCreation.openBody, a tag the repository has", openBody({ git: true, label: "left behind", fields: { kind: "tag", tag: "v1.2.0", newTag: false, tagName: "typed before", tagAt: "main", branch: "" }, base: "main", defaultBranch: "main" }).body],
  ["NewWorkstreamBody", "workstreamCreation.openBody, a pull request", openBody({ git: true, label: "", fields: { kind: "pr", pr: 42, head: "feat/a" }, base: "release", defaultBranch: "main" }).body],
  ["NewWorkstreamBody", "workstreamCreation.openBody, a copy of a plain folder", openBody({ git: false, label: "cart-total", fields: { kind: "branch", branch: "left behind" }, base: "release", defaultBranch: null }).body],
  ["NewWorkstreamBody", "workstreamCreation.openBody, a copy with no label", openBody({ git: false, label: "", fields: { kind: "new" }, base: "", defaultBranch: null }).body],
  ["PatchWorkstreamBody", "workstreamCardModel.renameBody, a name", renameBody(" Dark mode ")],
  ["PatchWorkstreamBody", "workstreamCardModel.renameBody, named by its branch again", renameBody("  ")],
  ["PlaceWorkstreamBody", "boardModel.placeBody", placeBody("doing", 3)],
  ["PlaceWorkstreamBody", "boardModel.placeBody, an index past what the wire takes", placeBody("archived", Number.MAX_SAFE_INTEGER)],
  ["DecisionRequest", "decisionsModel.tryRequest, a yes or no", tryRequest({ state: "the diff", kind: "noul", instructions: " Is it safe? ", options: [], ...STRAY })],
  ["DecisionRequest", "decisionsModel.tryRequest, a choice", tryRequest({ state: { n: 1 }, kind: "choice", instructions: "Which?", options: [{ id: "a", meaning: "the first" }, { id: "b", meaning: "the second" }, { id: "", meaning: "" }] })],
  ["BrowserResult", "browserBridgeModel.wireResult, the page's answer with every fact", wireResult({ ...answerResult(tab, said), tabs: [] })],
  ["BrowserResult", "browserBridgeModel.wireResult, the page's refusal", wireResult({ ...answerResult(tab, { ok: false, error: "nothing matches #x", selector: "#x" }), tabs: [] })],
  ["BrowserResult", "browserBridgeModel.wireResult, the tabs", wireResult({ ...tabsResult(tabs.sessions) })],
  ["BrowserResult", "browserBridgeModel.wireResult, a move", wireResult({ ...navigatedResult(tab), tabs: [] })],
  ["BrowserResult", "browserBridgeModel.wireResult, a screenshot", wireResult({ ...screenshotResult(tab, { ...shot, ...STRAY }, { width: 1280, height: 800 }), tabs: [] })],
  ["BrowserResult", "browserBridgeModel.wireResult, the bridge's refusal", wireResult({ ok: false, error: PAGE_SILENT, tabs: [], tab: "b1", url: tab.url, waited_ms: 10000 })],
  ["BrowserResult", "browserBridgeModel.wireResult, a result carrying what the wire does not know", wireResult({ ...answerResult(tab, said), ...STRAY, selector: "#go", path: "/tmp/x.png", session: tab, tabs: [{ ...tab }], scroll: { ...scroll, top: 1 } })],
  ["DrawResult", "drawRequestModel.drawnResult", drawnResult("01ARZ3NDEKTSV4RRFFQ69G5FAV", "ab".repeat(32), 12)],
  ["DrawResult", "drawRequestModel.snapshotResult", snapshotResult("01ARZ3NDEKTSV4RRFFQ69G5FAV", shot, { width: 1600, height: 900 })],
  ["DrawResult", "drawRequestModel.refusedResult", refusedResult("The drawing is gone.", "01ARZ3NDEKTSV4RRFFQ69G5FAV")],
  ["DrawResult", "drawRequestModel.refusedResult, about no drawing", refusedResult("The drawing is gone.")],
  ["PatchDrawingBody", "drawModel.persistedAppState, in a scene", { scene: { elements: [{ id: "e1", type: "rectangle" }], app_state: persistedAppState({ viewBackgroundColor: "#fdf8f0", gridModeEnabled: true, zoom: { value: 2 }, selectedElementIds: { e1: true }, collaborators: new Map() }) }, base_hash: "ab".repeat(32) }],
  ["NewMcpBody", "mcpFormModel.toTransport, a stdio server", { id: stdio.id, description: "", tags: [], transport: toTransport(stdio) }],
  ["PatchMcpBody", "mcpFormModel.toTransport, an http server", { description: "Remote", tags: ["ops"], transport: toTransport(http) }],
  ["ProbeMcpBody", "mcpFormModel.toTransport, a draft dialled", { transport: toTransport({ ...http, kind: "sse" }) }],
  ["GitConfigWrite", "gitConfigModel.diffWrites", diffWrites([{ key: "user.name", value: "Ada", editable: true }, { key: "user.email", value: "ada@example.com", editable: true }, { key: "core.editor", value: "vi", editable: false }], { "user.name": "Ada L.", "user.email": " ", "core.editor": "nano", "stray.key": "x" })],
  ["SettleChangesBody", "reviewLensModel.hunkSettleTarget", { verdict: "keep", target: hunkSettleTarget("src/main.rs", { id: "h1", header: "@@", lines: [] }, "ab".repeat(32)) }],
  ["SettleChangesBody", "reviewLensModel.fileSettleTarget", { verdict: "undo", target: fileSettleTarget("src/main.rs"), force: true }],  ["GovernanceBody", "governanceModel.governanceBody", governanceBody({ approval: { policy: "admins", pubkeys: ["x"] }, escalation: { policy: "listed", pubkeys: ["team:01TEAM"] }, publish: { policy: "owner" }, updated_at: 1, ...STRAY })],
  ["TeamPatch", "rosterModel.withoutMember", withoutMember({ id: "01TEAM", name: "Core", members: [{ agent: "reviewer" }, { human: "ab".repeat(32) }], tags: [], ...STRAY }, { agent: "reviewer" })],
  ["CloseBody", "goalPageModel.closeBody, abandoned with the person's word on why", closeBody({ rationale: " the client left ", replacedBy: null, ...STRAY }, "01ARZ3NDEKTSV4RRFFQ69G5FAV")],
  ["CloseBody", "goalPageModel.closeBody, superseded", closeBody({ rationale: "said before the goal was chosen", replacedBy: "01ARZ3NDEKTSV4RRFFQ69G5FAW", ...STRAY }, "01ARZ3NDEKTSV4RRFFQ69G5FAV")],
  ["CloseBody", "goalPageModel.closeBody, nothing said", closeBody({ rationale: "", replacedBy: null }, "01ARZ3NDEKTSV4RRFFQ69G5FAV")],
];

test("what the models build fits the type the node reads it as — the ordinary body, and one built from a draft that carries more", () => {
  const faults = [];
  for (const [name, from, body] of BODIES) {
    assert.notEqual(body, null, `${from}: there is a body to hold`);
    for (const problem of fits(name, body)) faults.push(`${name} ← ${from} — ${problem}`);
  }
  assert.deepEqual(faults, []);
});

test("the guard bites: a body the node holds to its keys is refused here the moment it carries one more", () => {
  const strict = BODIES.filter(([name]) => refusesUnknownKeys(SCHEMA, name));
  assert.ok(strict.length > 50, `most of what is held is a body the node holds to its keys: ${strict.length}`);
  for (const [name, from, body] of strict) {
    const refused = fits(name, { ...body, left_open: true });
    assert.ok(refused.some((p) => p.includes("left_open")), `${name} ← ${from}: a stray key passes`);
  }
  // A step, an input, an event: held by hand, and so held here.
  const stray = definitionBody(blankWorkflow("Untitled workflow"));
  assert.deepEqual(fits("NewWorkflowBody", { ...stray, steps: [{ ...stray.steps[0], selected: true }] }), ["$.steps[0].selected: a key `Step` does not declare"]);
  assert.deepEqual(fits("NewWorkflowBody", { ...stray, steps: [{ ...stray.steps[0], on: { event: "manual", cron: "* * * * *" } }] }).at(-1), "$.steps[0].on.cron: a key `StartOn` does not declare");
  assert.deepEqual(fits("NewWorkflowBody", { ...stray, inputs: [{ name: "a", label: "A", kind: "text", options: [] }] }), ["$.inputs[0].options: a key `InputDef` does not declare"]);
  // The fault this guard was written on: the element's path in a browser answer.
  assert.deepEqual(fits("BrowserResult", { ...answerResult(tab, { ok: true, text: "x" }), selector: "#go" }), ["$.selector: a key `BrowserResult` does not declare"]);
});

// ---------------------------------------------------------------------------
// The call sites
// ---------------------------------------------------------------------------

/** The arguments of the call whose `(` is at `open`, split at the top level; `null` when it never closes. */
function argumentsAt(text, open) {
  const args = [];
  let depth = 0;
  let from = open + 1;
  let quote = null;
  for (let i = open; i < text.length; i += 1) {
    const c = text[i];
    if (quote) {
      if (c === "\\") i += 1;
      else if (c === quote) quote = null;
      continue;
    }
    if (c === '"' || c === "'" || c === "`") quote = c;
    else if (c === "(" || c === "{" || c === "[") depth += 1;
    else if (c === ")" || c === "}" || c === "]") {
      depth -= 1;
      if (depth === 0) {
        const last = text.slice(from, i).trim();
        if (last !== "" || args.length > 0) args.push(last);
        return args;
      }
    } else if (c === "," && depth === 1) {
      args.push(text.slice(from, i).trim());
      from = i + 1;
    }
  }
  return null;
}

/**
 * Every function of `api.ts` that sends — `post`, `put`, `patch`, `del` —
 * with its own parameters' names and what it hands the request, in order.
 * @returns {{name: string, method: string, params: string[], sent: string[]}[]}
 */
function senders() {
  const text = readFileSync(join(SRC, "api.ts"), "utf8");
  const api = text.slice(text.indexOf("export const api = {"));
  const heads = [...api.matchAll(/^ {2}([A-Za-z0-9_]+): (?:async )?\(/gm)];
  const out = [];
  heads.forEach((head, n) => {
    const source = api.slice(head.index, heads[n + 1]?.index ?? api.length).replace(/^\s*\/\/\/.*$/gm, "");
    const params = (argumentsAt(source, source.indexOf("(")) ?? []).map((p) => p.split(/[?:=]/)[0].trim());
    const call = /\b(post|put|patch|del)</.exec(source);
    if (!call) return;
    // Past the answer's type, to the call's own arguments.
    let at = source.indexOf("<", call.index);
    for (let depth = 0; at < source.length; at += 1) {
      if (source[at] === "<") depth += 1;
      else if (source[at] === ">" && source[at - 1] !== "=") depth -= 1;
      if (depth === 0) break;
    }
    out.push({ name: head[1], method: call[1], params, sent: argumentsAt(source, source.indexOf("(", at)) ?? [] });
  });
  return out;
}

/** A route with every parameter a hole, its query gone: `/workstreams/{}/git/stage`. */
const routeShape = (path) => path.replace(/\$\{[^}]*\}/g, "{}").replace(/\{[^}]*\}/g, "{}").replace(/\?.*$/, "");

/**
 * The closed types by the route their description names — the node writes
 * `` `POST /workstreams/{wid}/git/stage` `` at the head of a body's
 * description — as `METHOD /shape` → the type's names.
 * @returns {Map<string, string[]>}
 */
function typesByRoute() {
  const out = new Map();
  for (const [name, def] of Object.entries(SCHEMA.definitions)) {
    const description = typeof def?.description === "string" ? def.description : "";
    for (const m of description.matchAll(/`(GET|POST|PUT|PATCH|DELETE) ([^`\s]+)[^`]*`/g)) {
      const key = `${m[1]} ${routeShape(m[2])}`;
      out.set(key, [...(out.get(key) ?? []), name]);
    }
  }
  return out;
}

/**
 * The routes whose body's type names another route first, or none: each with
 * the type the node reads the body as (`crates/bisa-node/src/dto.rs`).
 */
const ROUTE_TYPES = Object.freeze({
  "POST /workstreams/{}/git/unstage": "StageBody",
  "POST /workstreams/{}/git/continue": "GitOperation",
  "POST /workstreams/{}/git/skip": "GitOperation",
  "POST /workstreams/{}/git/stashes/{}/apply": "GitStashTarget",
  "POST /workstreams/{}/git/stashes/{}/pop": "GitStashTarget",
  "POST /workstreams/{}/git/stashes/{}/drop": "GitStashTarget",
  "POST /projects/{}/archive": "ArchiveBody",
  "POST /workflows/{}/archive": "ArchiveBody",
  "POST /projects/{}/attach": "ProjectAttachBody",
  "PUT /projects/{}/assignees": "AssigneesBody",
  "POST /messages/{}/react": "ReactBody",
  "POST /hosts/{}/messages/{}/react": "ReactBody",
  "POST /dms": "OpenDmBody",
  "POST /hosts/{}/dms": "OpenDmBody",
  "POST /read": "ScopeBody",
  "POST /unread": "ScopeBody",
  "POST /hosts/{}/read": "ScopeBody",
  "POST /ide/lsp/{}/{}/change": "LspDocument",
  // The same body as `POST /attachments/{sha256}/file` — the name the copy is made under — whose route the type's description names first.
  "POST /artifacts/{}/serve": "NamedFileBody",
});

/** The keys a literal names at its own level; a spread is `...`. */
function literalKeys(literal) {
  const parts = argumentsAt(`(${literal.slice(1, -1)})`, 0) ?? [];
  return parts.filter((part) => part !== "").map((part) => (part.startsWith("...") ? "..." : (/^["']?([A-Za-z_$][\w$]*)["']?\s*(?::|$)/.exec(part)?.[1] ?? `?${part}`)));
}

test("the bodies the client builds itself carry only keys the route's type declares", () => {
  const types = typesByRoute();
  const METHOD = { post: "POST", put: "PUT", patch: "PATCH", del: "DELETE" };
  const faults = [];
  const unheld = [];
  let held = 0;
  for (const { name, method, sent } of senders()) {
    const body = (sent[1] ?? "").trim();
    // A literal, or one of two chosen by a condition: every `{…}` the body spells is a body that may be sent.
    const literals = body.startsWith("{") && !body.includes("?") ? [body] : [...body.matchAll(/\{[^{}]*\}/g)].map((m) => m[0]);
    const keyed = literals.map(literalKeys).filter((keys) => keys.length > 0);
    if (keyed.length === 0) continue;
    const path = /^[`"]([^`"]*(?:\$\{[^}]*\}[^`"]*)*)[`"]$/.exec(sent[0] ?? "");
    const route = path ? `${METHOD[method]} ${routeShape(path[1])}` : null;
    const named = route === null ? [] : ROUTE_TYPES[route] ? [ROUTE_TYPES[route]] : (types.get(route) ?? []);
    if (named.length === 0) {
      unheld.push(`${name}: ${route ?? sent[0]}`);
      continue;
    }
    for (const keys of keyed) {
      held += 1;
      const fits = named.some((type) => keys.every((key) => key in (SCHEMA.definitions[type]?.properties ?? {})));
      if (!fits) faults.push(`api.${name} sends {${keys.join(", ")}} as ${route}, read as ${named.join(" | ")}`);
    }
  }
  assert.ok(held > 80, `the client's own bodies are read: ${held}`);
  assert.deepEqual(faults, [], "a key the node's type does not declare is a 400: name the keys the type takes");
  // The one body no type holds: `PUT /settings/{scope}` takes a free map of keys — whatever settings the panel writes — so no closed type could name them; every other sender's body is held above.
  assert.deepEqual(unheld, ["setSettings: `/settings/${scope}${project ? `?project=${encodeURIComponent(project)}` : \"\"}`"], "a body with no type to hold it to: name its type in ROUTE_TYPES");
  for (const [route, type] of Object.entries(ROUTE_TYPES)) assert.ok(refusesUnknownKeys(SCHEMA, type), `${route}: ${type} is a type the node holds to its keys`);
});

/**
 * The functions of `api.ts` that hand a parameter through as the body, each
 * with the place of that parameter among its own.
 * @returns {Map<string, number>}
 */
function passThroughSenders() {
  const through = new Map();
  for (const { name, params, sent } of senders()) {
    const body = (sent[1] ?? "").replace(/\s*\?\?\s*\{\}$/, "");
    const place = params.indexOf(body);
    if (place >= 0 && body !== "s") through.set(name, place);
  }
  return through;
}

test("no function of the client hands its abort signal over as the body", () => {
  // `del(path, s)` reads the signal as the body: the request cannot be given up, and what is sent is the signal, serialised.
  const all = senders();
  assert.ok(all.length > 150, `the senders are read: ${all.length}`);
  assert.deepEqual(all.filter(({ sent }) => sent[1] === "s").map(({ name }) => name), []);
  const withdraw = all.find(({ name }) => name === "withdrawRun");
  assert.deepEqual(withdraw?.sent.slice(1), ["undefined", "s"], "a queued run is withdrawn with no body, and the signal is the signal");
  // Every sender's signal is its last word, where it has one.
  assert.deepEqual(all.filter(({ params, sent }) => params.includes("s") && !sent.slice(2).includes("s")).map(({ name }) => name), []);
});

/** The names a source imports from a model — a `.mjs` beside a component. */
function modelImports(text) {
  const names = new Set();
  for (const m of text.matchAll(/import\s*\{([^}]*)\}\s*from\s*"[^"]+\.mjs"/g)) {
    for (const part of m[1].split(",")) {
      const word = part.trim().replace(/^type\s+/, "");
      if (word) names.add(word.split(/\s+as\s+/).pop());
    }
  }
  return names;
}

/**
 * Whether a body handed through is one the type can hold to its keys: a
 * literal of named keys — a spread only of a literal chosen by a condition —
 * or what a model's function built, said at the call or under a name of the
 * same file. `at` is where the call stands.
 */
function builtByName(arg, text, models, at) {
  const called = (expression) => {
    const fn = /^([A-Za-z_$][\w$]*)\(/.exec(expression);
    return fn !== null && models.has(fn[1]);
  };
  if (arg.startsWith("{")) {
    // Every spread at the literal's own level is of a literal chosen by a condition.
    let depth = 0;
    for (let i = 0; i < arg.length; i += 1) {
      const c = arg[i];
      if (c === "{" || c === "(" || c === "[") depth += 1;
      else if (c === "}" || c === ")" || c === "]") depth -= 1;
      else if (depth === 1 && arg.startsWith("...", i)) {
        if (!/^\.\.\.\([^?]+\?\s*\{[^}]*\}\s*:\s*\{\s*\}\s*\)/.test(arg.slice(i))) return false;
        i += 2;
      }
    }
    return true;
  }
  if (called(arg)) return true;
  // A name of this file, or a field of one: what it was given where it was last named before the call.
  const name = /^([A-Za-z_$][\w$]*)(?:\.[A-Za-z_$][\w$]*)?$/.exec(arg);
  if (!name) return false;
  const given = [...text.slice(0, at).matchAll(new RegExp(`\\b(?:const|let)\\s+${name[1]}(?::[^=]+)?\\s*=\\s*([^;\\n]+)`, "g"))].pop();
  return given !== undefined && called(given[1].trim());
}

/**
 * The call sites that hand through a body built elsewhere, each with where
 * it is built — a wrapper that takes the body as its own argument, or a
 * body that is a person's own words.
 */
const BUILT_ELSEWHERE = Object.freeze({
  "draw/drawBridge.ts: api.answerDrawingRequest(result)": "`drawRequestModel`'s three answers — `drawnResult`, `snapshotResult`, `refusedResult` — each by name, held above",
  "views/Agents.tsx: api.patchAgent(patch)": "the pane's two callers hand `{ skills }` or `{ mcps }`, the parameter's own type",
  "views/_board/BoardCenter.tsx: api.patchWorkstream(body)": "the card menu's literals — `{ due: null }`, `{ pinned }` — typed `PatchWorkstreamBody`",
  "views/_settings/ConnectorsPanel.tsx: api.createConnector(def)": "the definition a person typed as JSON: the node's refusal by name is the editor's answer",
  "views/_settings/ConnectorsPanel.tsx: api.updateConnector(def)": "the definition a person typed as JSON, opened on `connectorsModel.definitionOf`",
  "views/_settings/ConnectorsPanel.tsx: api.validateConnector(def)": "the definition a person typed as JSON: validating it is the point",
  "views/_settings/GlobalGitPanel.tsx: api.setGitConfig(form.write)": "`useConfigEdits`' write — `gitConfigModel.diffWrites`, held above",
  "views/_studio/AskCard.tsx: api.answerAsk(body)": "the card's buttons hand `conversationAskModel.allowBody` or `denyBody`, the parameter's own type, held above",
  "views/_studio/scope.ts: api.postChannelMessage(body)": "`useScopeMessages`' literal of named keys, typed `NewMessageBody`",
  "views/_studio/scope.ts: api.postConversationMessage(body)": "`useScopeMessages`' literal of named keys, typed `NewMessageBody`",
  "views/_studio/scope.ts: api.postGoalMessage(body)": "`useScopeMessages`' literal of named keys, typed `NewMessageBody`",
  "views/_work/ProjectGitDialog.tsx: api.setWorkstreamGitConfig(form.write)": "`useConfigEdits`' write — `gitConfigModel.diffWrites`, held above",
  "views/_work/gitOps.ts: api.gitRebasePlan(plan)": "`RebaseEditorDialog` hands `rebaseEditorModel.planOf`, held above",
  "views/_work/gitOps.ts: api.gitStashPush(body)": "`StashDialog`'s literal of the four named keys, typed `GitStashPush`",
  "views/_work/gitOps.ts: api.openPr(values)": "`PrForm` hands `prFormModel.prRequest`, held above",
  "views/_work/useGitConfigDraft.ts: api.setWorkstreamGitConfig(body)": "`useConfigEdits`' write — `gitConfigModel.diffWrites`, held above",
  "views/_work/useProjectSettingsDraft.ts: api.setWorkstreamGitConfig(w.write)": "`projectSettingsDraftModel.writes`' git config — `gitConfigModel.diffWrites`, held above",
});

test("every call site hands a body through as a literal of named keys or as what a model built — or is listed with where it is built", () => {
  const senders = passThroughSenders();
  assert.ok(senders.size > 60, `the senders are read: ${senders.size}`);
  for (const name of ["createGoal", "putWorkflow", "patchAgent", "answerBrowserRequest", "answerDrawingRequest", "setGovernance", "putGitProfile", "createProject", "decideIn"]) assert.ok(senders.has(name), `${name} hands its body through`);
  const isSource = (p) => /\.(ts|tsx|mjs)$/.test(p) && !p.endsWith(".d.mts") && !/\.test\.mjs$/.test(p) && !p.endsWith("api.ts");
  const open = [];
  const met = new Set();
  for (const file of sourceFiles(SRC, isSource)) {
    const text = readFileSync(file, "utf8");
    const models = modelImports(text);
    for (const m of text.matchAll(/\bapi\s*\.\s*([A-Za-z0-9_]+)\(/g)) {
      const place = senders.get(m[1]);
      if (place === undefined) continue;
      // A comment that names a call is no call.
      const line = text.slice(text.lastIndexOf("\n", m.index) + 1, m.index);
      if (/^\s*(\/\/|\*|\/\*)/.test(line)) continue;
      const args = argumentsAt(text, m.index + m[0].length - 1);
      const arg = args?.[place];
      if (arg === undefined || arg === "") continue;
      if (builtByName(arg, text, models, m.index)) continue;
      const key = `${relative(SRC, file).replaceAll("\\", "/")}: api.${m[1]}(${arg.replace(/\s+/g, " ")})`;
      met.add(key);
      if (!(key in BUILT_ELSEWHERE)) open.push(key);
    }
  }
  assert.deepEqual(open, [], "a body handed through that is neither a literal of named keys nor a model's: build it in a model, or list it with where it is built");
  assert.deepEqual(Object.keys(BUILT_ELSEWHERE).filter((key) => !met.has(key)), [], "a listed call site that is no longer there: take its row out");
});
