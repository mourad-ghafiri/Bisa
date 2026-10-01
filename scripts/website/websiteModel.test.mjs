/**
 * The website's rules: a fragment's facts, tokens filled and refused, an
 * image's size from its own bytes, the repository's facts read off their
 * sources, the README's block, the palette and the budgets. Run with
 * `node --test scripts/website/websiteModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  BUDGET,
  PALETTE,
  SHOT_EXTENSIONS,
  WEB_SHOT,
  cwebpArgs,
  README_SLIDESHOW,
  img2webpArgs,
  slideshowFrames,
  TODAY,
  atLeast,
  checkExample,
  decisionPointCount,
  displayName,
  escapeHtml,
  fillTokens,
  firstSentence,
  imageSize,
  macosVersion,
  paletteOffences,
  parseFragment,
  parseFtl,
  parseScenarios,
  parseStartEvents,
  parseStepKinds,
  parseWorkflowToml,
  presetLabel,
  replaceBlock,
  tally,
  tokensIn,
  tomlTable,
} from "./websiteModel.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const read = (rel) => readFileSync(join(root, rel), "utf8");

test("a reader that finds too little is broken, in words — never an empty page", () => {
  assert.deepEqual(atLeast([1, 2], 2, "things"), [1, 2]);
  assert.throws(() => atLeast([1], 2, "step kinds"), /the reader of step kinds is broken: found 1, expected at least 2/);
  assert.throws(() => atLeast(null, 1, "asks"), /found 0/);
});

test("every word from the repository is escaped before it is HTML", () => {
  assert.equal(escapeHtml(`<a href="x">Tom & Jerry's</a>`), "&lt;a href=&quot;x&quot;&gt;Tom &amp; Jerry&#39;s&lt;/a&gt;");
  assert.equal(escapeHtml(null), "");
});

test("a page fragment opens with its facts as JSON, and is refused without its path, title or description", () => {
  const { meta, body } = parseFragment('<!--{"path": "/", "title": "Bisa", "description": "A page."}-->\n<p>Hi</p>', "home.html");
  assert.deepEqual(meta, { path: "/", title: "Bisa", description: "A page." });
  assert.equal(body, "<p>Hi</p>");
  assert.throws(() => parseFragment("<p>no facts</p>", "x.html"), /x\.html: a page fragment opens with its facts/);
  assert.throws(() => parseFragment("<!--{path: /}-->", "x.html"), /x\.html: its facts are not JSON/);
  assert.throws(() => parseFragment('<!--{"path": "/", "title": " ", "description": "d"}-->', "x.html"), /its facts name no title/);
});

test("a token is filled by what answers it; one nobody answers stops the build with its name and its place", () => {
  const resolve = (name, arg) => ({ "count:agents": "32", macos: "11" })[arg ? `${name}:${arg}` : name];
  assert.equal(fillTokens("{{count:agents}} agents on macOS {{macos}}", resolve, "home"), "32 agents on macOS 11");
  assert.throws(() => fillTokens("{{count:agnets}}", resolve, "home.html (body)"), /home\.html \(body\): unknown token \{\{count:agnets\}\}/);
  assert.equal(fillTokens("no tokens, {not} {{ one }}", resolve, "x"), "no tokens, {not} {{ one }}", "only the token's own shape is read");
  assert.deepEqual(tokensIn("{{a}} {{b:c}} {{figure:ide-board}}"), ["a", "b:c", "figure:ide-board"]);
  assert.equal(fillTokens("{{zero}}", () => 0, "x"), "0", "a zero is an answer");
});

/** A PNG's signature and IHDR, enough for its size. */
function png(width, height) {
  const b = new Uint8Array(33);
  b.set([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, 0x49, 0x48, 0x44, 0x52]);
  new DataView(b.buffer).setUint32(16, width);
  new DataView(b.buffer).setUint32(20, height);
  return b;
}

/** A JPEG with an APP0 segment before its start-of-frame. */
function jpeg(width, height) {
  const app0 = [0xff, 0xe0, 0x00, 0x10, ...new Array(14).fill(0)];
  const sof = [0xff, 0xc0, 0x00, 0x11, 0x08, height >> 8, height & 0xff, width >> 8, width & 0xff, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0];
  return Uint8Array.from([0xff, 0xd8, ...app0, ...sof]);
}

/** A WebP of the given chunk, its size written as the format writes it. */
function webp(chunk, width, height) {
  const b = new Uint8Array(40);
  b.set([..."RIFF"].map((c) => c.charCodeAt(0)), 0);
  b.set([..."WEBP"].map((c) => c.charCodeAt(0)), 8);
  b.set([...chunk].map((c) => c.charCodeAt(0)), 12);
  if (chunk === "VP8X") {
    const w = width - 1;
    const h = height - 1;
    b.set([w & 0xff, (w >> 8) & 0xff, (w >> 16) & 0xff, h & 0xff, (h >> 8) & 0xff, (h >> 16) & 0xff], 24);
  } else if (chunk === "VP8L") {
    const bits = (width - 1) | ((height - 1) << 14);
    b.set([0x2f, bits & 0xff, (bits >> 8) & 0xff, (bits >> 16) & 0xff, (bits >> 24) & 0xff], 20);
  } else {
    b.set([0x9d, 0x01, 0x2a, width & 0xff, (width >> 8) & 0x3f, height & 0xff, (height >> 8) & 0x3f], 23);
  }
  return b;
}

test("an image's size is read from its own header — PNG, JPEG, WebP in its three chunks — and nothing else is guessed", () => {
  assert.deepEqual(imageSize(png(2400, 1500)), { type: "png", width: 2400, height: 1500 });
  assert.deepEqual(imageSize(jpeg(1600, 1000)), { type: "jpeg", width: 1600, height: 1000 });
  assert.deepEqual(imageSize(webp("VP8X", 1536, 1872)), { type: "webp", width: 1536, height: 1872 });
  assert.deepEqual(imageSize(webp("VP8L", 800, 500)), { type: "webp", width: 800, height: 500 });
  assert.deepEqual(imageSize(webp("VP8 ", 640, 400)), { type: "webp", width: 640, height: 400 });
  assert.equal(imageSize(new Uint8Array(40)), null, "zeros are no image");
  assert.equal(imageSize(png(10, 10).slice(0, 12)), null, "cut short");
  assert.equal(imageSize(Uint8Array.from([0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0, 0, 0x12, 0x34, 0, 0, 0, 0, 0, 0])), null, "a JPEG whose markers stop making sense");
  assert.equal(imageSize(null), null);
  // The pet the site copies is a VP8X WebP: eight frames of 192 by nine rows of 208.
  assert.deepEqual(imageSize(new Uint8Array(readFileSync(join(root, "library/pets/moonrice/spritesheet.webp")))), { type: "webp", width: 1536, height: 1872 });
});

test("a catalog's one-line messages are read; a message over several lines is left out", () => {
  const ftl = parseFtl("a-b = One\nmulti =\n    two lines\n# a comment\nc-d = Two words  \n");
  assert.equal(ftl.get("a-b"), "One");
  assert.equal(ftl.get("c-d"), "Two words");
  assert.equal(ftl.has("multi"), false);
});

test("the step kinds and the ways a run begins are read off the designer's palette and its words — and refused when a word is missing", () => {
  const ftl = parseFtl(read("locales/en/desktop/workflow.ftl"));
  const source = read("desktop/src/views/_workflow/stepKinds.mjs");
  const kinds = parseStepKinds(source, ftl);
  assert.equal(kinds.length, 18);
  assert.deepEqual([...new Set(kinds.map((k) => k.family))], ["event", "gateway", "loop", "task"]);
  assert.deepEqual(kinds[0], { kind: "start", family: "event", label: "Start", explain: ftl.get("workflow-step-kinds-one-way-run-begins-hand-event") });
  for (const k of kinds) assert.ok(k.label && k.explain && !k.explain.startsWith("workflow-"), `${k.kind} has words`);
  const starts = parseStartEvents(source, ftl);
  assert.equal(starts.length, 10);
  assert.deepEqual(starts[0], { event: "manual", label: "By hand" });
  const without = new Map(ftl);
  without.delete("workflow-step-kinds-judge");
  assert.throws(() => parseStepKinds(source, without), /the step kind judge has no words in the catalog \(workflow-step-kinds-judge\)/);
  assert.throws(() => parseStepKinds("export const STEP_KINDS = [];", ftl), /the reader of step kinds is broken/);
  assert.throws(() => parseStartEvents("nothing", ftl), /the reader of start events is broken/);
});

test("a catalog workflow is its name, its description and its steps in order — never its inputs or a step's sub-tables", () => {
  const toml = `[workflow]\nname = "Fix and review"\ndescription = "Fix it, then \\"review\\" it."\n\n[[workflow.inputs]]\nname = "report"\nkind = "text"\n\n[[workflow.steps]]\nid = "start"\nname = "Start"\nkind = "start"\n\n[[workflow.steps]]\nid = "fix"\nname = "Fix it"\nkind = "agent"\n\n[workflow.steps.output_schema]\ntype = "object"\n\n[workflow.steps.output_schema.properties.summary]\ntype = "string"\n\n[[workflow.steps]]\nid = "done"\nname = "Done"\nkind = "end"\n`;
  const wf = parseWorkflowToml(toml);
  assert.equal(wf.name, "Fix and review");
  assert.equal(wf.description, 'Fix it, then "review" it.');
  assert.deepEqual(wf.steps, [
    { id: "start", name: "Start", kind: "start" },
    { id: "fix", name: "Fix it", kind: "agent" },
    { id: "done", name: "Done", kind: "end" },
  ]);
  assert.throws(() => parseWorkflowToml('[[workflow.steps]]\nid = "x"\nname = "X"\nkind = "agent"\n'), /names itself under \[workflow\]/);
  assert.throws(() => parseWorkflowToml('[workflow]\nname = "W"\n[[workflow.steps]]\nid = "x"\n[[workflow.steps]]\nid = "y"\nname = "Y"\nkind = "end"\n'), /W: a step lacks its id, name or kind/);
  const bugFix = parseWorkflowToml(read("library/catalog/workflows/bug-fix.toml"));
  assert.equal(bugFix.name, "Bug fix");
  assert.equal(bugFix.steps[0].kind, "start");
  assert.ok(bugFix.steps.some((s) => s.name === "Reproduced?") && bugFix.steps.some((s) => s.kind === "human"), "the round back to the reporter is in it");
});

test("a catalog record is the string values of its own table — never another table's, never a prompt over several lines", () => {
  const toml = '# Built-in agent\n\n[agent]\nname = "Researcher"\ndescription = "Research work items: the landscape. Then more."\nskills = ["options-and-tradeoffs", "evidence-and-citation"]\nsystem_prompt = \"\"\"\nYou are\n\"\"\"\n\n[agent.extra]\nname = "not this"\n';
  assert.deepEqual(tomlTable(toml, "agent"), { name: "Researcher", description: "Research work items: the landscape. Then more.", skills: ["options-and-tradeoffs", "evidence-and-citation"] });
  assert.deepEqual(tomlTable(toml, "team"), {}, "a table that is not there is empty");
  assert.deepEqual(tomlTable('[[agent]]\nname = "an array of tables is not the table"\n', "agent"), {});
  const team = tomlTable(read("library/catalog/teams/venture.toml"), "team");
  assert.equal(team.name, "Venture");
  assert.ok(team.agents.includes("product-manager") && team.agents.length >= 5);
  assert.equal(tomlTable(read("library/catalog/connectors/gmail.toml"), "connector").name, "Gmail");
  assert.deepEqual(tomlTable(read("library/catalog/agents/mobile-developer.toml"), "agent").skills.slice(0, 3), ["workstream-workflow", "flutter-development", "app-store-publishing"]);
});

test("a description's first sentence is what a card shows", () => {
  assert.equal(firstSentence("Builds apps. Ships them."), "Builds apps.");
  assert.equal(firstSentence("Is it so? Then yes."), "Is it so?");
  assert.equal(firstSentence("No full stop"), "No full stop");
  assert.equal(firstSentence("v1.2 is out. Next."), "v1.2 is out.", "a point inside a word is no sentence's end");
  assert.equal(firstSentence(undefined), "");
});

/** A catalog with two agents, for the example's checks. */
const CATALOG = { kinds: new Set(["start", "agent", "decide", "end"]), agents: new Map([["developer", { skills: ["code-review-checklist"] }], ["researcher", { skills: [] }]]) };
const step = (id, kind, more = {}) => ({ id, kind, name: `Step ${id}`, ...more });

test("the worked example names only what ships: kinds of the palette, catalog agents, the skills each agent carries", () => {
  const ok = { steps: [step("s", "start"), step("a", "agent", { agent: "developer", skills: ["code-review-checklist"] }), step("e", "end")] };
  assert.equal(checkExample(ok, CATALOG), ok);
  const broken = (change) => ({ steps: ok.steps.map((s) => (s.id === "a" ? { ...s, ...change } : s)) });
  assert.throws(() => checkExample(broken({ kind: "task" }), CATALOG), /step a is a task, which is no kind of the designer's/);
  assert.throws(() => checkExample(broken({ agent: "wizard" }), CATALOG), /names the agent wizard, which the catalog does not ship/);
  assert.throws(() => checkExample(broken({ skills: ["flutter-development"] }), CATALOG), /gives developer the skill flutter-development, which it does not carry/);
  assert.throws(() => checkExample(broken({ name: "" }), CATALOG), /step a has no name/);
  assert.throws(() => checkExample({ steps: [ok.steps[0], { ...ok.steps[0] }] }, CATALOG), /s twice/);
  assert.throws(() => checkExample({ steps: [] }, CATALOG), /the reader of the example's steps is broken/);
});

test("the website's own example holds against the real catalog", () => {
  const example = JSON.parse(read("scripts/website/examples/mobile-app.json"));
  const kinds = new Set([...read("desktop/src/views/_workflow/stepKinds.mjs").matchAll(/\{ kind: "([a-z_]+)"/g)].map((m) => m[1]));
  const agents = new Map();
  for (const slug of new Set(example.steps.map((s) => s.agent).filter(Boolean))) agents.set(slug, tomlTable(read(`library/catalog/agents/${slug}.toml`), "agent"));
  assert.equal(checkExample(example, { kinds, agents }), example);
  assert.match(example.goal, /App Store and Google Play/);
  assert.ok(example.steps.some((s) => s.kind === "approval") && example.steps.some((s) => s.kind === "parallel"), "a gate before the stores, and both stores at once");
});

test("a harness's name is its adapter's own; a preset's its label; the decision points and the minimum macOS are the source's", () => {
  assert.equal(displayName('fn display_name(&self) -> &str {\n        "Claude Code"\n    }'), "Claude Code");
  assert.equal(displayName("fn display_name(&self) -> &str {\n        &self.label\n    }"), null, "a name read from a field is no fixed name");
  assert.equal(displayName(read("crates/bisa-adapters/src/copilot.rs")), "GitHub Copilot CLI");
  const catalog = read("crates/bisa-harness/src/catalog.rs");
  assert.equal(presetLabel(catalog, "goose"), "Goose");
  assert.equal(presetLabel(catalog, "cursor-agent"), "Cursor Agent");
  assert.equal(presetLabel(catalog, "nobody"), null);
  assert.equal(decisionPointCount("pub const ALL: [DecisionPoint; 11] = ["), 11);
  assert.equal(decisionPointCount("nothing here"), null);
  assert.equal(macosVersion('{"bundle": {"macOS": {"minimumSystemVersion": "11.0"}}}'), "11");
  assert.equal(macosVersion('{"bundle": {"macOS": {"minimumSystemVersion": "12.3"}}}'), "12.3");
  assert.throws(() => macosVersion('{"bundle": {}}'), /names no bundle\.macOS\.minimumSystemVersion/);
});

/** A scenarios page of `n` rows across two sections. */
function scenarios(n, today = (i) => TODAY[i % 3]) {
  const rows = Array.from({ length: n }, (_, i) => `| ${i < n / 2 ? "A" : "B"}${i + 1} | Ask number ${i + 1}, with *emphasis* and \`code\`. | the parts | **${today(i)}** |`);
  const half = Math.ceil(n / 2);
  return ["# Scenarios", "", "## A. Software delivery", "", "| # | The ask | The path | Today |", "|---|---|---|---|", ...rows.slice(0, half), "", "## B. Mobile apps", "", ...rows.slice(half), "", "## What the tally says", ""].join("\n");
}

test("the hundred asks are read by section, the sentence cleaned of its marks, the Today word one of three", () => {
  const asks = parseScenarios(scenarios(96));
  assert.equal(asks.length, 96);
  assert.deepEqual(asks[0], { id: "A1", section: "Software delivery", ask: "Ask number 1, with emphasis and code.", today: "runs" });
  assert.equal(asks.at(-1).section, "Mobile apps");
  assert.deepEqual(tally(asks), { runs: 32, "with you": 32, "custom connector": 32 });
  assert.throws(() => parseScenarios(scenarios(96, () => "someday")), /its Today column says "\*\*someday\*\*", not one of runs · with you · custom connector/);
  assert.throws(() => parseScenarios(scenarios(20)), /the reader of scenarios is broken: found 20/);
  const real = parseScenarios(read("docs/guide/real-world-scenarios.md"));
  const t = tally(real);
  assert.equal(t.runs + t["with you"] + t["custom connector"], real.length, "every ask counted once");
  assert.ok(new Set(real.map((a) => a.id)).size === real.length, "every id once");
});

test("the README's block is replaced between its markers, and nothing else moves", () => {
  const text = "# Title\n\n<!-- s -->\nold\nlines\n<!-- /s -->\n\nThe rest.\n";
  assert.equal(replaceBlock(text, "<!-- s -->", "<!-- /s -->", "new"), "# Title\n\n<!-- s -->\nnew\n<!-- /s -->\n\nThe rest.\n");
  assert.equal(replaceBlock(replaceBlock(text, "<!-- s -->", "<!-- /s -->", "new"), "<!-- s -->", "<!-- /s -->", "new"), "# Title\n\n<!-- s -->\nnew\n<!-- /s -->\n\nThe rest.\n", "the same content twice is the same file");
  assert.throws(() => replaceBlock("no markers", "<!-- s -->", "<!-- /s -->", "x"), /are not both there, in order/);
  assert.throws(() => replaceBlock("<!-- /s --> <!-- s -->", "<!-- s -->", "<!-- /s -->", "x"), /in order/);
});

test("the palette admits the platform's blues and cyans and its three status hues, and nothing purple", () => {
  assert.deepEqual(PALETTE.band, [180, 262]);
  const ok = [
    "oklch(0.52 0.17 240)", // the Glass accent
    "oklch(0.81 0.117 208)", // the logo's cyan rail
    "oklch(0.256 0.082 260)", // the logo's navy
    "oklch(0.53 0.13 155)", // ok
    "oklch(0.56 0.13 90)", // warn
    "oklch(0.49 0.2 22)", // danger
    "oklch(0.68 0.17 25)", // within 12° of danger
    "oklch(0.5 0.02 300)", // a grey, whatever its hue
    "oklch(1 0 0 / 0.5)",
  ];
  assert.deepEqual(paletteOffences(`a { color: ${ok.join("; color: ")}; }`), []);
  assert.equal(paletteOffences("a { color: oklch(0.52 0.19 265); }").length, 1, "Dune's indigo");
  assert.match(paletteOffences("a { color: oklch(0.6 0.2 295); }")[0], /hue 295 is outside the platform's palette/, "the violet accent");
  assert.match(paletteOffences("a { color: #8b5cf6; }")[0], /write colours as oklch/);
  assert.equal(paletteOffences("a { color: rgb(1 2 3); background: hsl(1 2% 3%); }").length, 2);
  assert.deepEqual(paletteOffences("a { fill: url(#grad); } /* #8b5cf6 in a comment */"), [], "an address and a comment are no colours");
});

test("the budgets and the screenshot formats are the ones the README promises", () => {
  assert.ok(BUDGET.screenshot === 2 * 1024 * 1024 && BUDGET.screenshotWarn === 600 * 1024);
  assert.ok(BUDGET.css >= 32 * 1024 && BUDGET.js <= 32 * 1024 && BUDGET.page <= 256 * 1024);
  assert.deepEqual(SHOT_EXTENSIONS, ["webp", "jpg", "png"]);
});

test("the README's slideshow holds the opening's screens that are here, in their order, each by the first of its screenshots", () => {
  const slides = [
    { label: "Goals", shots: ["overview"] },
    { label: "Projects", shots: ["ide-overview", "ide-agent"] },
    { label: "Notes", shots: ["notes"] },
    { label: "Pets", shots: ["pets"] },
  ];
  const here = { overview: 100, "ide-agent": 80, pets: 50 };
  assert.deepEqual(slideshowFrames(slides, (id) => here[id] ?? null), [
    { label: "Goals", id: "overview", bytes: 100 },
    { label: "Projects", id: "ide-agent", bytes: 80 },
    { label: "Pets", id: "pets", bytes: 50 },
  ], "a screen with no screenshot is left out — an animation has no placeholder");
  assert.deepEqual(slideshowFrames(undefined, () => 1), []);
  const args = img2webpArgs(["a.webp", "b.webp"], "out.webp");
  assert.deepEqual(args.slice(0, 2), ["-loop", "0"], "it loops for ever");
  assert.deepEqual(args.slice(-8), ["-d", String(README_SLIDESHOW.holdMs), "a.webp", "-d", String(README_SLIDESHOW.holdMs), "b.webp", "-o", "out.webp"], "each frame held, in order, then the file");
  assert.ok(args.includes("-lossy"));
});

test("a screenshot is made for the web at most 2000 px wide, shrunk and never enlarged, its metadata dropped", () => {
  const wide = cwebpArgs("in.png", "out.webp", 3680);
  assert.deepEqual(wide.slice(wide.indexOf("-resize"), wide.indexOf("-resize") + 3), ["-resize", String(WEB_SHOT.width), "0"], "the height follows the width");
  assert.deepEqual(wide.slice(-3), ["in.png", "-o", "out.webp"]);
  assert.ok(wide.includes("-sharp_yuv") && wide.includes("-quiet"));
  assert.equal(wide[wide.indexOf("-metadata") + 1], "none");
  assert.equal(wide[wide.indexOf("-q") + 1], "88");
  assert.ok(!cwebpArgs("in.png", "out.webp", 2000).includes("-resize"), "as wide as the web needs: kept");
  assert.ok(!cwebpArgs("in.png", "out.webp", 1200).includes("-resize"), "narrower: never enlarged");
});
