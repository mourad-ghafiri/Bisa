/**
 * Annotations typed into a harness of the person's own (ide/18, ide/06 §Start
 * words): the words, the terminals they can go to, the door per state.
 * Run with `node --test --import ./src/i18n/preload.mjs src/views/_workbench/harnessHandoffModel.test.mjs` from `desktop/`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { contextBlock, handOffTarget, handoffContent, harnessDoor, harnessTargets, harnessWords, sentToWords, targetRows } from "./harnessHandoffModel.mjs";
import { annotationChips, annotationsContent, annotationsTarget } from "./annotationModel.mjs";
import { filePage, urlPage } from "./contextChips.mjs";

const ANNOTATIONS = [
  { id: 1, selector: "body > main > button:nth-of-type(1)", tag: "button", excerpt: '<button class="cta">Buy</button>\n', text: "Buy", note: "make it blue " },
  { id: 2, selector: "#price", tag: "span", excerpt: '<span id="price">$9</span>', text: "$9", note: "show two decimals" },
];
const LABELS = { "claude-code": "Claude Code", codex: "Codex", gemini: "Gemini CLI" };
const H = (id, label, installed = true, launch = { program: id, args: [], resume_args: [] }, detail = null) => ({ id, label, installed, launch, detail });
const S = (id, harness, state, workstream = "w1", extra = {}) => ({
  id,
  kind: "terminal",
  origin: "terminal",
  state,
  since: 1,
  harness,
  model: null,
  effort: null,
  agent: null,
  session_id: null,
  work_item: null,
  conversation: null,
  goal: null,
  run: null,
  workstream,
  project: "p1",
  cwd: null,
  transcript_path: null,
  started: 1,
  pid: null,
  cost: null,
  children: [],
  last_activity: 1,
  revision: 1,
  ...extra,
});
const T = (key, id, harness, sessionId = null, status = "live", extra = {}) => ({
  key,
  scope: "workstream",
  id,
  harness,
  label: null,
  login: null,
  resume: false,
  run: false,
  mobileDevelopment: null,
  generation: 0,
  liveness: status === "live" ? { status } : { status: "exited", code: 0 },
  restoring: false,
  terminalId: status === "live" ? `pty-${key}` : null,
  sessionId,
  running: null,
  start: null,
  openedAt: 1,
  exitedAt: null,
  ...extra,
});

test("the block is the engine's own, byte for byte: the header, then each chip as framing.rs prints it", () => {
  const framing = readFileSync(new URL("../../../../crates/bisa-engine/src/framing.rs", import.meta.url), "utf8");
  assert.ok(framing.includes('"--- context the person attached ---\\n"'), "the header the engine prints");
  assert.ok(framing.includes('"[annotation] {} · {selector}\\n```html\\n{}\\n```\\n{}\\n"'), "the annotation arm the engine prints");
  assert.ok(framing.includes("excerpt.trim_end()") && framing.includes("note.trim()"), "the trims the engine applies");
  const chips = annotationChips(filePage("src/index.html"), ANNOTATIONS);
  assert.equal(
    contextBlock(chips),
    '--- context the person attached ---\n[annotation] src/index.html · body > main > button:nth-of-type(1)\n```html\n<button class="cta">Buy</button>\n```\nmake it blue\n[annotation] src/index.html · #price\n```html\n<span id="price">$9</span>\n```\nshow two decimals\n',
  );
  assert.equal(contextBlock([]), "");
  assert.equal(contextBlock([{ kind: "file", path: "a" }]), "", "only annotation chips are the tray's");
});

test("the words a harness gets are the agent's request — the person's lead, the edit contract kept by hand — then the block", () => {
  const page = filePage("src/index.html");
  const words = harnessWords({ message: "  keep the layout ", count: 2, page, annotations: ANNOTATIONS });
  const head = handoffContent(annotationsContent("keep the layout", 2), annotationsTarget(2, page));
  assert.ok(words.startsWith(head), "the same lead and target the agent gets");
  assert.ok(
    head.startsWith(
      "keep the layout\n\nEach of the 2 annotated elements is attached as a chip with the change wanted for it.\n\nEdit the 2 annotated elements in index.html directly in the file. Do not commit or push — I will keep or undo the change myself.",
    ),
    head,
  );
  assert.equal(words, `${head}\n\n${contextBlock(annotationChips(page, ANNOTATIONS)).trimEnd()}`);
  assert.ok(!words.endsWith("\n"), "no trailing newline: Enter is the tab's to add");
  const url = harnessWords({ message: "", count: 1, page: urlPage("http://localhost:5173/pricing"), annotations: ANNOTATIONS.slice(0, 1) });
  assert.ok(url.includes("Edit the annotated element of the page at http://localhost:5173/pricing directly in the file."), url);
  assert.ok(url.includes("[annotation] http://localhost:5173/pricing · body > main > button:nth-of-type(1)"));
  assert.equal(harnessWords({ message: "", count: 0, page, annotations: [] }), handoffContent(annotationsContent("", 0), annotationsTarget(0, page)), "no chips, no block");
});

test("the targets: a new terminal per harness with an interactive form, installed first, then the workstream's live harness tabs; unknown while the list is", () => {
  const harnesses = [H("codex", "Codex"), H("claude-code", "Claude Code"), H("gemini", "Gemini CLI", false), { id: "acp:goose", label: "Goose", installed: true, launch: null, detail: null }];
  const asked = { state: "waiting", on: { on: "question", text: "Overwrite it?" } };
  const sessions = [
    S("s1", "claude-code", "idle"),
    S("s2", "codex", { state: "waiting", on: { on: "permission", tool: "Bash" } }),
    S("s3", "claude-code", "running", "w2"),
    S("s4", "codex", "idle", "w1", { origin: "worker" }),
    S("s6", "codex", { state: "running", tool: "Task" }, "w1", { children: [{ id: "c1", name: "explorer", state: "running", since: 2, started: 2 }, { id: "c2", name: "writer", state: asked, since: 3, started: 2 }] }),
  ];
  const terminals = [T("t1", "w1", "claude-code", "s1"), T("t2", "w1", "codex", "s2"), T("t3", "w1", "gemini"), T("t4", "w2", "claude-code", "s3"), T("t5", "w1", "codex", null, "exited"), T("t6", "w1", "codex", "s6")];
  const targets = harnessTargets(harnesses, sessions, terminals, "w1");
  assert.deepEqual(
    targets.map((x) => (x.kind === "new" ? `new:${x.harness}:${x.installed}` : `tab:${x.key}:${x.harness}:${x.state === null ? "-" : typeof x.state === "string" ? x.state : x.state.state}`)),
    ["new:claude-code:true", "new:codex:true", "new:gemini:false", "tab:t3:gemini:-", "tab:t1:claude-code:idle", "tab:t2:codex:waiting", "tab:t6:codex:waiting"],
    "the rail's order — a shell whose harness does not report first — and no ACP row, no other workstream's tab, no exited tab, no headless session; a sub-agent's dialog is its session's wait",
  );
  assert.deepEqual(targets[6].state, asked, "the child's own dialog, so the door can name it");
  assert.deepEqual(harnessDoor(targets[6]), { ok: false, words: "It is waiting on you — Overwrite it? — answer that first." }, "held, like the session's own wait");
  assert.equal(harnessTargets(null, sessions, terminals, "w1"), null, "unknown is not empty");
  assert.deepEqual(harnessTargets([], [], [], "w1"), []);
});

test("the door: installed or not, listening, working with a word, starting and waiting refused with theirs, a silent harness taken as it stands", () => {
  assert.deepEqual(harnessDoor({ kind: "new", harness: "codex", label: "Codex", installed: true, detail: null }), { ok: true, words: null });
  assert.deepEqual(harnessDoor({ kind: "new", harness: "gemini", label: "Gemini CLI", installed: false, detail: null }), { ok: false, words: null }, "the label has said it all");
  assert.deepEqual(harnessDoor({ kind: "new", harness: "gemini", label: "Gemini CLI", installed: false, detail: "npm i -g @google/gemini-cli" }), { ok: false, words: null }, "the label already says where to get it: no second line");
  const tab = (state) => ({ kind: "tab", key: "t1", harness: "codex", state, sessionId: "s1" });
  assert.deepEqual(harnessDoor(tab("idle")), { ok: true, words: null });
  assert.deepEqual(harnessDoor(tab("thinking")), { ok: true, words: "It is working — the request is typed into it once its turn ends and it listens again." });
  assert.equal(harnessDoor(tab({ state: "running", tool: "Bash" })).ok, true);
  assert.deepEqual(harnessDoor(tab("starting")), { ok: false, words: "It is still starting — there is no prompt to type into yet." });
  assert.deepEqual(harnessDoor(tab({ state: "waiting", on: { on: "permission", tool: "Bash" } })), { ok: false, words: "It is waiting on you — permission: Bash — answer that first." });
  assert.deepEqual(harnessDoor(tab({ state: "waiting", on: null })), { ok: false, words: "It is waiting on you — answer that first." });
  assert.deepEqual(harnessDoor(tab(null)), { ok: true, words: "This harness does not report what it is doing — the request is typed at its prompt as it stands." });
  assert.deepEqual(harnessDoor(tab("done")), { ok: false, words: "That terminal's harness has ended." });
});

test("the rows: the harness's words, the live tabs after a rule, a row disabled with the door's reason as its hint", () => {
  const targets = [
    { kind: "new", harness: "claude-code", label: "Claude Code", installed: true, detail: null },
    { kind: "new", harness: "gemini", label: "Gemini CLI", installed: false, detail: "brew install gemini-cli" },
    { kind: "tab", key: "t1", harness: "claude-code", state: "idle", sessionId: "s1" },
    { kind: "tab", key: "t2", harness: "codex", state: { state: "waiting", on: { on: "question", text: "Which file?" } }, sessionId: "s2" },
    { kind: "tab", key: "t3", harness: "custom-x", state: null, sessionId: null },
    { kind: "tab", key: "t4", harness: "codex", state: "running", sessionId: "s4" },
    { kind: "tab", key: "t5", harness: "codex", state: "starting", sessionId: "s5" },
    { kind: "tab", key: "t6", harness: "codex", state: "failed", sessionId: "s6" },
  ];
  const rows = targetRows(targets, LABELS);
  assert.deepEqual(
    rows.map((r) => [r.id, r.label, r.disabled, r.separatorBefore]),
    [
      ["new:claude-code", "Claude Code — new session", false, false],
      ["new:gemini", "Gemini CLI — brew install gemini-cli", true, false],
      ["tab:t1", "Claude Code · listening", false, true],
      ["tab:t2", "Codex · waiting on you", true, false],
      ["tab:t3", "custom-x · open", false, false],
      ["tab:t4", "Codex · working", false, false],
      ["tab:t5", "Codex · starting", true, false],
      ["tab:t6", "Codex · ended", true, false],
    ],
  );
  assert.equal(rows[6].hint, "It is still starting — there is no prompt to type into yet.");
  assert.equal(rows[7].hint, "That terminal's harness has ended.");
  assert.equal(rows[3].hint, "It is waiting on you — Which file? — answer that first.");
  assert.equal(rows[5].hint, "It is working — the request is typed into it once its turn ends and it listens again.");
  assert.equal(rows[0].hint, null);
  assert.equal(rows[1].hint, null, "said once, in the label");
  assert.equal(targetRows([{ kind: "new", harness: "gemini", label: "Gemini CLI", installed: false, detail: null }], LABELS)[0].label, "Gemini CLI — not installed");
  assert.deepEqual(targetRows(null, LABELS), []);
});

test("a fresh terminal for the request is never resumed and carries the words; the toasts name the harness and where the rows stay", () => {
  assert.deepEqual(handOffTarget("w1", "codex", "Edit it."), { scope: "workstream", id: "w1", harness: "codex", resume: false, start: "Edit it." });
  assert.equal(
    sentToWords({ kind: "new", harness: "codex", label: "Codex", installed: true, detail: null }, LABELS, "annotations"),
    "Opening Codex in a new terminal — the annotations are typed once it listens. They stay here until Clear.",
  );
  assert.equal(sentToWords({ kind: "tab", key: "t1", harness: "claude-code", state: "idle", sessionId: "s1" }, LABELS, "annotations"), "Typed into Claude Code's terminal. The annotations stay here until Clear.");
  assert.equal(sentToWords({ kind: "tab", key: "t3", harness: "custom-x", state: null, sessionId: null }, LABELS, "annotations"), "Typed into custom-x's terminal. The annotations stay here until Clear.");
  assert.equal(
    sentToWords({ kind: "tab", key: "t4", harness: "codex", state: { state: "running", tool: "Bash" }, sessionId: "s4" }, LABELS, "annotations"),
    "Handed to Codex's terminal — the annotations are typed once its turn ends. They stay here until Clear.",
    "a tab at work holds them",
  );
  assert.equal(sentToWords({ kind: "tab", key: "t4", harness: "codex", state: "thinking", sessionId: "s4" }, LABELS, "annotations"), "Handed to Codex's terminal — the annotations are typed once its turn ends. They stay here until Clear.");
});
