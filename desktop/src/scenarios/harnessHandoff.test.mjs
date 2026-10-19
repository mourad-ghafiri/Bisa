/**
 * Annotations typed into a harness of the person's own (ide/18 §Annotating a
 * page, ide/06 §Start words), driven through the models — the words from a
 * draft, the terminals they can go to, a new tab that carries them until it
 * has typed them, the nudge machine that decides when, a live tab typed into
 * — reading after each step what the screen would say; the wiring is read
 * off the sources, no DOM.
 *
 * Run with `node --test --import ./src/i18n/preload.mjs src/scenarios/harnessHandoff.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { EMPTY_DRAFT, addAnnotation } from "../views/_workbench/annotationModel.mjs";
import { filePage } from "../views/_workbench/contextChips.mjs";
import { handOffTarget, harnessDoor, harnessTargets, harnessWords, sentToWords, targetRows } from "../views/_workbench/harnessHandoffModel.mjs";
import { emptyTerminals, noteLive, noteStarted, openTerminal, restoreTerminals, serializeTerminals } from "../shell/terminalsModel.mjs";
import { START_QUIET_MS, armed, due, onInput, onOutput, pending, said } from "../terminal/resumeStartModel.mjs";
import { ENTER, typedWords } from "../terminal/typedWordsModel.mjs";

const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
const page = filePage("src/index.html");
const draft = addAnnotation(
  addAnnotation(EMPTY_DRAFT, { selector: "body > main > button:nth-of-type(1)", tag: "button", excerpt: '<button class="cta">Buy</button>', text: "Buy" }, "make it blue"),
  { selector: "#price", tag: "span", excerpt: '<span id="price">$9</span>', text: "$9" },
  "two decimals",
);
const H = (id, label, installed = true) => ({ id, label, installed, launch: { program: id, args: [], resume_args: [] }, detail: null });
const S = (id, harness, state, workstream = "w1") => ({
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
});
const LABELS = { "claude-code": "Claude Code", codex: "Codex" };

test("a draft of two annotations becomes the words a harness reads: the agent's request, then the engine's own block", () => {
  const words = harnessWords({ message: "keep the layout", count: draft.annotations.length, page, annotations: draft.annotations });
  assert.ok(
    words.startsWith(
      'keep the layout\n\nEach of the 2 annotated elements is attached as a chip with the change wanted for it.\n\nEdit the 2 annotated elements in index.html directly in the file. Do not commit or push — I will keep or undo the change myself.\n\n--- context the person attached ---\n[annotation] src/index.html · body > main > button:nth-of-type(1)\n```html\n<button class="cta">Buy</button>\n```\nmake it blue\n[annotation] src/index.html · #price',
    ),
    words,
  );
});

test("the menu lists a new terminal per harness and the workstream's live tabs; a waiting tab is held with its reason, a working one carries a word", () => {
  let terminals = emptyTerminals();
  terminals = openTerminal(terminals, { scope: "workstream", id: "w1", harness: "claude-code" });
  terminals = noteLive(terminals, "t1", 0, "pty-1", "s1");
  terminals = openTerminal(terminals, { scope: "workstream", id: "w1", harness: "codex" });
  terminals = noteLive(terminals, "t2", 0, "pty-2", "s2");
  const sessions = [S("s1", "claude-code", "idle"), S("s2", "codex", { state: "waiting", on: { on: "question", text: "Which file?" } })];
  const rows = targetRows(harnessTargets([H("claude-code", "Claude Code"), H("codex", "Codex")], sessions, terminals.sessions, "w1"), LABELS);
  assert.deepEqual(
    rows.map((r) => [r.label, r.disabled, r.hint]),
    [
      ["Claude Code — new session", false, null],
      ["Codex — new session", false, null],
      ["Claude Code · listening", false, null],
      ["Codex · waiting on you", true, "It is waiting on you — Which file? — answer that first."],
    ],
  );
  assert.equal(harnessDoor(rows[3].target).ok, false, "read again at the click: still held");
  assert.equal(harnessDoor({ ...rows[2].target, state: "running" }).words, "It is working — the request waits at its prompt and is read after its turn.");
});

test("a new terminal carries the words until it has typed them: never into storage, never into a restart; typed as a paste once the prompt is drawn", () => {
  const words = harnessWords({ message: "", count: draft.annotations.length, page, annotations: draft.annotations });
  let state = openTerminal(emptyTerminals(), handOffTarget("w1", "codex", words));
  const tab = state.sessions[0];
  assert.equal(tab.harness, "codex");
  assert.equal(tab.resume, false, "a request opens a session of its own");
  assert.equal(tab.start, words);
  assert.equal(JSON.parse(JSON.stringify(serializeTerminals(state))).sessions[0].start, undefined);
  assert.equal(restoreTerminals(JSON.parse(JSON.stringify(serializeTerminals(state)))).sessions[0].start, null);
  // The mount arms the nudge machine; the words go once the harness has drawn its prompt.
  let start = armed(1000);
  assert.equal(due(start, 1000 + START_QUIET_MS), false, "nothing drawn yet");
  start = onOutput(start, 1200);
  assert.equal(due(start, 1200 + START_QUIET_MS), true, "quiet: the prompt is drawn");
  const typed = typedWords(tab.start, true);
  assert.ok(typed.text.startsWith("\u001b[200~") && typed.text.endsWith("\u001b[201~"), "a bracketed paste when the program asked");
  assert.ok(!typed.text.includes("\n"), "no newline reaches the program as Enter");
  assert.equal(typed.enter, ENTER);
  start = said(start);
  assert.equal(pending(start, 2000), false, "said once, the tick stops");
  state = noteStarted(state, tab.key, 0);
  assert.equal(state.sessions[0].start, null, "nothing left to say");
  // The person typing first drops the words.
  let other = onOutput(armed(1000), 1200);
  other = onInput(other);
  assert.equal(due(other, 5000), false);
  assert.equal(pending(other, 1300), false, "dropped for good");
  assert.equal(
    sentToWords({ kind: "new", harness: "codex", label: "Codex", installed: true, detail: null }, LABELS, "annotations"),
    "Opening Codex in a new terminal — the annotations are typed once it listens. They stay here until Clear.",
  );
});

test("the wiring: the tray draws the door only when its rows read as words, the mount types start words through the PTY and never as keystrokes, the store pastes through the tab's bytes", () => {
  const tray = src("../views/_workbench/ContextTray.tsx");
  assert.ok(tray.includes("harness?: { words: () => string } | null;"), "an optional door");
  assert.ok(tray.includes("{harness && count > 0 && !over && <HarnessHandoffMenu"), "drawn only with words to type and rows to type");
  assert.ok(src("../views/_workbench/AnnotationTray.tsx").includes("harness={{ words: () => harnessWords("), "the annotation tray says how its rows read");
  assert.ok(!src("../views/_workbench/CaptureTray.tsx").includes("harness="), "a capture's picture cannot be typed");
  const menu = src("../views/_workbench/HarnessHandoffMenu.tsx");
  assert.ok(menu.includes("harnessDoor(target)") && menu.includes("pasteIntoTerminalTab(target.key, words())") && menu.includes("openTerminalIn(handOffTarget(wid, target.harness, words()))"), "the door, then one of the two ways");
  assert.ok(!menu.includes("onDone"), "the draft stays: a terminal has no keep or undo");
  const terminal = src("../terminal/Terminal.tsx");
  assert.ok(terminal.includes("registerTerminalModes(sessionKey, () => ({ bracketed: term.modes.bracketedPasteMode }))"), "the mount says whether the program asked for bracketed paste");
  assert.ok(terminal.includes("typedWords(startWords, term.modes.bracketedPasteMode)") && terminal.includes("void session.write(typed.text)"), "start words go through the PTY");
  assert.ok(!terminal.includes("term.paste(startWords") && !terminal.includes("onInput.current?.(typed"), "never as the person's keystrokes, which would answer a dialog");
  assert.ok(terminal.includes("refs.onStarted.current?.();"), "and the host hears they were typed");
  assert.ok(
    src("../shell/TerminalPanel.tsx").includes("start={s.start}") && src("../shell/TerminalPanel.tsx").includes("onStarted={() => terminalStarted(s.key, s.generation)}"),
    "the panel carries the tab's words and clears them once typed",
  );
  const store = src("../shell/useTerminals.ts");
  assert.ok(
    store.includes("export async function pasteIntoTerminalTab(") && store.includes("await writeToTerminalTab(key, typed.text);") && store.includes("await writeToTerminalTab(key, typed.enter);"),
    "a live tab is typed into through its bytes, Enter a beat later",
  );
  assert.ok(store.includes('throw new Error(t("shell-use-terminals-terminal-not-on-screen"))'), "a tab with no emulator on screen says so");
});
