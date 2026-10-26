/**
 * Annotations typed into a harness of the person's own (ide/18 §Annotating a
 * page, ide/06 §Start words), driven through the models — the words from a
 * draft, the terminals they can go to, a new tab that carries them until it
 * has typed them, the nudge machine that decides when, a live tab handed
 * them and typing them once its harness listens — reading after each step
 * what the screen would say; the wiring is read off the sources, no DOM.
 *
 * Run with `node --test --import ./src/i18n/preload.mjs src/scenarios/harnessHandoff.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { EMPTY_DRAFT, addAnnotation } from "../views/_workbench/annotationModel.mjs";
import { filePage } from "../views/_workbench/contextChips.mjs";
import { handOffTarget, harnessDoor, harnessTargets, harnessWords, sentToWords, targetRows } from "../views/_workbench/harnessHandoffModel.mjs";
import { emptyTerminals, handToTerminal, listensNow, noteListening, noteLive, noteStarted, openTerminal, restoreTerminals, serializeTerminals, showTerminal } from "../shell/terminalsModel.mjs";
import { START_QUIET_MS, armed, armedDrawn, due, isTyping, onInput, onOutput, pending, said } from "../terminal/resumeStartModel.mjs";
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
  assert.equal(harnessDoor({ ...rows[2].target, state: "running" }).words, "It is working — the request is typed into it once its turn ends and it listens again.");
});

test("a request handed to a live tab at work is held on the tab and typed once its harness listens and is quiet — never into its turn; the person typing first drops it, the emulator's focus report does not", () => {
  const words = harnessWords({ message: "", count: draft.annotations.length, page, annotations: draft.annotations });
  let terminals = openTerminal(emptyTerminals(), { scope: "workstream", id: "w1", harness: "claude-code" });
  terminals = noteLive(terminals, "t1", 0, "pty-1", "s1");
  const working = S("s1", "claude-code", { state: "running", tool: "Edit" });
  const target = harnessTargets([H("claude-code", "Claude Code")], [working], terminals.sessions, "w1").find((x) => x.kind === "tab");
  assert.deepEqual(harnessDoor(target), { ok: true, words: "It is working — the request is typed into it once its turn ends and it listens again." }, "a tab at work takes the request, with a word");
  // The store: the words ride the tab, the roster's word is noted, and the
  // tab is shown even though it already was the store's active one — the
  // browser tab took the centre since, and the centre follows `shown`.
  let state = handToTerminal(terminals, "t1", words);
  state = noteListening(state, "t1", listensNow(state.sessions[0], working));
  state = showTerminal(state, "t1");
  assert.equal(state.sessions[0].start, words);
  assert.equal(state.sessions[0].listening, false, "held: the harness is at work, no tick runs");
  assert.equal(state.shown, terminals.shown + 1, "shown anyway");
  assert.equal(sentToWords(target, LABELS, "annotations"), "Handed to Claude Code's terminal — the annotations are typed once its turn ends. They stay here until Clear.");
  // The roster moves: a sub-agent asks — still held — then the turn ends.
  const asking = { ...working, children: [{ id: "c1", name: "writer", state: { state: "waiting", on: { on: "question", text: "Which?" } }, since: 2, started: 2 }] };
  assert.equal(listensNow(state.sessions[0], asking), false, "a sub-agent's dialog holds it");
  state = noteListening(state, "t1", listensNow(state.sessions[0], S("s1", "claude-code", "idle")));
  assert.equal(state.sessions[0].listening, true, "the turn ended: the mount arms, the prompt taken as drawn");
  let start = armedDrawn(5000);
  assert.equal(due(start, 5000), false, "not at once: a beat of quiet first");
  start = onOutput(start, 5300);
  assert.equal(due(start, 5300 + START_QUIET_MS - 1), false, "the harness still drawing restarts the beat");
  assert.equal(due(start, 5300 + START_QUIET_MS), true, "quiet: typed now, as a paste, Enter apart");
  const typed = typedWords(state.sessions[0].start, true);
  assert.ok(typed.text.startsWith("\u001b[200~") && typed.enter === ENTER);
  state = noteStarted(state, "t1", 0);
  assert.equal(state.sessions[0].start, null, "nothing left to say");
  assert.equal(noteListening(state, "t1", false), state, "and the roster's word is no longer noted");
  // The person's words go first — but the emulator's own focus report, sent
  // when the tab is shown, is not their words.
  assert.equal(isTyping("\u001b[I"), false);
  assert.equal(isTyping("\r"), true);
  assert.equal(pending(onInput(armedDrawn(1000)), 1100), false, "dropped for good; the tab says so and the request is still in the tray");
  // A harness that does not report is typed into as it stands.
  assert.equal(listensNow({ ...state.sessions[0], sessionId: null }, working), true);
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
  assert.ok(menu.includes("harnessDoor(target)") && menu.includes("handToTerminalTab(target.key, words())") && menu.includes("openTerminalIn(handOffTarget(wid, target.harness, words()))"), "the door, then one of the two ways");
  assert.ok(!menu.includes("onDone"), "the draft stays: a terminal has no keep or undo");
  const terminal = src("../terminal/Terminal.tsx");
  assert.ok(terminal.includes("registerTerminalModes(sessionKey, () => ({ bracketed: term.modes.bracketedPasteMode }))"), "the mount says whether the program asked for bracketed paste");
  assert.ok(terminal.includes("typedWords(words, term.modes.bracketedPasteMode)") && terminal.includes("void session.write(typed.text)"), "start words go through the PTY");
  assert.ok(!terminal.includes("term.paste(") && !terminal.includes("onInput.current?.(typed"), "never as the person's keystrokes, which would answer a dialog");
  assert.ok(terminal.includes("refs.onStarted.current?.();") && terminal.includes("refs.onDropped.current?.();"), "and the host hears they were typed, or dropped");
  assert.ok(terminal.includes("}, [start, listening]);") && terminal.includes("armStartDrawn(Date.now())") && terminal.includes('live.mode === "fresh"'), "words handed to a live tab arm the machine by the roster's word, the prompt taken as drawn; a fresh shell's own words are never gated");
  assert.ok(terminal.includes("if (isTyping(data)) {") && terminal.includes("if (live.words !== null) dropWords(live, term, refs);"), "the person's words drop a request owed; the emulator's own replies drop nothing");
  const panel = src("../shell/TerminalPanel.tsx");
  assert.ok(
    panel.includes("start={s.start}") && panel.includes("listening={s.listening}") && panel.includes("onStarted={() => terminalStarted(s.key, s.generation)}") && panel.includes("onDropped={() => terminalStarted(s.key, s.generation)}"),
    "the panel carries the tab's words and the roster's word, and clears the words once typed or dropped",
  );
  const store = src("../shell/useTerminals.ts");
  assert.ok(
    store.includes("export function handToTerminalTab(") && store.includes("set(showTerminal(noteListening(handed, key, listensNow(tab, row)), key));") && !store.includes("pasteIntoTerminalTab") && !store.includes("writeToTerminalTab(key, typed"),
    "a live tab is handed the words, the roster's word noted, the tab shown — never written into at the click",
  );
  assert.ok(store.includes("onSessionsChange(() => {") && store.includes("next = noteListening(next, tab.key, listensNow(tab, row));"), "the roster's every commit is the word on whether a held tab's harness listens");
  assert.ok(store.includes('throw new Error(t("shell-use-terminals-terminal-not-on-screen"))') && store.includes('throw new Error(t("shell-use-terminals-terminal-not-live"))'), "a tab with no emulator on screen, or no live shell, says so");
  assert.ok(src("../views/_workbench/CenterDocuments.tsx").includes("}, [activeTerminal, shown]);"), "the centre follows a show of the tab that already was the active one");
});
