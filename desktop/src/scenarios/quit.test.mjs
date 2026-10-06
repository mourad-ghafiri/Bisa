/**
 * The way out, held, as the sources show it (ide/03, ide/13): on macOS the
 * shell gives tao's app delegate the `applicationShouldTerminate:` it lacks,
 * so ⌘Q, the application menu's Quit, the Dock's Quit and a logout wait for
 * the webview's question — the window brought forward first — and are
 * answered through `replyToApplicationShouldTerminate:` by the one flow:
 * `quit_app` is the yes, `quit_declined` the no, and `quit_ready` the word
 * that there is a webview to ask. Off macOS no OS asks an app to quit, so
 * the keymap's `quit` on Ctrl+Q opens the same door — global, from a
 * composer and a focused shell alike, and never on a Mac, where the menu's
 * key equivalent fires first. Source assertions, as `tray.test.mjs` makes
 * them — no DOM. Run with `node --test desktop/src/scenarios/quit.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { COMMANDS, chordFor, interceptsInTerminal, resolveKeymap } from "../shell/keymapModel.mjs";
import { CLOSE_OUTCOMES, declines } from "../shell/closeFlowModel.mjs";

const src = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (rel) => readFileSync(join(src, rel), "utf8");

/** The text from one marker up to the next, both of which must be there. */
const between = (text, from, to) => {
  const start = text.indexOf(from);
  assert.ok(start >= 0, `the source says \`${from}\``);
  const end = text.indexOf(to, start + from.length);
  assert.ok(end > start, `and \`${to}\` after it`);
  return text.slice(start, end);
};

/** Every marker is there, each after the one before it. */
const inOrder = (text, markers, why) => {
  let at = -1;
  for (const marker of markers) {
    const next = text.indexOf(marker, at + 1);
    assert.ok(next > at, `${why}: \`${marker}\``);
    at = next;
  }
};

test("on macOS every terminate is held: the delegate answers later, the window comes forward, and the webview is asked", () => {
  const quit = read("../src-tauri/src/quit.rs");
  assert.ok(quit.includes("sel!(applicationShouldTerminate:)"), "the method tao's delegate lacks");
  assert.ok(quit.includes("objc2::ffi::object_setClass(") && quit.includes("ns_app.setDelegate(Some(&delegate))"), "tao's own instance, in a subclass with the one method, set again so AppKit re-reads it");
  const decide = between(quit, "fn decide()", "pub fn reply(");
  inOrder(
    decide,
    ["HOLD.is_ready()", "TerminateNow", "crate::tray::main_window(app)", "HOLD.take()", "TerminateCancel", "crate::tray::show_window(app)", "crate::QUIT_REQUESTED", "TerminateLater"],
    "no webview or no window: the exit stands; a question already up: refused; else shown, asked, and answered later",
  );
  assert.ok(quit.includes("std::panic::catch_unwind(decide)"), "a panic never crosses into AppKit");
  assert.ok(quit.includes("replyToApplicationShouldTerminate(go)"), "the answer goes back to AppKit");
  assert.ok(quit.includes('#[cfg(not(target_os = "macos"))]') && quit.includes("pub fn reply(_app: &AppHandle, _go: bool) {}"), "nothing to hold elsewhere");
});

test("the shell installs the hold last in setup, and every way out ends in quit_app, which answers the hold before it exits on its own", () => {
  const main = read("../src-tauri/src/main.rs");
  assert.ok(main.includes("\nmod quit;\n"), "the module is the shell's");
  const setup = between(main, ".setup(|app| {", ".on_window_event(");
  inOrder(setup, ["tray::install(app.handle())", "app.manage(tray)", "quit::install(app.handle())"], "after the window and the icon it brings forward exist");
  const quitApp = between(main, "fn quit_app(", "fn quit_declined(");
  assert.ok(quitApp.includes("if !quit::answer(&app, true) {") && quitApp.includes("app.exit(0);"), "the yes to a held quit, else the shell's own exit");
  assert.ok(between(main, "fn quit_declined(", "fn quit_ready(").includes("quit::answer(&app, false)"), "the no");
  assert.ok(between(main, "fn quit_ready(", "\n}\n").includes("quit::ready()"), "the webview listens");
  const handler = between(main, "invoke_handler(", ".build(context)");
  for (const cmd of ["quit_app,", "quit_declined,", "quit_ready,"]) assert.ok(handler.includes(cmd), `${cmd} registered`);
  const tray = read("../src-tauri/src/tray/mod.rs");
  assert.ok(between(tray, "fn request_quit(", "\n}\n").includes("crate::quit::answer(app, true)"), "the icon's Quit with no window lets a held terminate finish rather than stopping the loop under it");
  assert.ok(main.includes("RunEvent::ExitRequested {") && main.includes("api.prevent_exit()"), "Tauri's own exit request stays held, for the letter of it");
});

test("the webview answers the shell: ready once it listens, a no on a cancelled quit or a failed save, nothing on a dropped second request", () => {
  const guard = read("shell/useCloseGuard.ts");
  assert.ok(guard.includes('tellShell("quit_ready")') && guard.includes("off = [close, quitRequested, keyboard];"), "said once the three listeners stand");
  assert.ok(guard.includes("if (declines(outcome)) decline();") && guard.includes('tellShell("quit_declined")'), "the no, by the model's rule");
  const failed = between(guard, '"the way out failed"', "\n  );");
  assert.ok(failed.includes("decline();"), "a flow that failed is a no too: AppKit must not wait forever");
  assert.ok(guard.includes("onDoor(QUIT_APP, () => leave(\"keyboard\"))"), "the keyboard's door, at which the guard stands");
  assert.equal(declines("cancelled"), true);
  assert.equal(declines("unsaved"), true);
  assert.equal(declines("busy"), false, "the question up answers for both");
  assert.equal(declines("closed"), false, "quit_app answered");
  assert.deepEqual(CLOSE_OUTCOMES.filter(declines), ["cancelled", "unsaved"]);
});

test("off macOS Ctrl+Q is the keymap's quit: global, from a composer and a focused shell, never on a Mac, and it opens the guard's door", () => {
  const quit = COMMANDS.find((c) => c.id === "quit");
  assert.ok(quit, "declared");
  assert.equal(quit.when, "global");
  assert.equal(quit.always, true, "from inside a composer too");
  assert.deepEqual(quit.mac, {}, "nothing on a Mac: the menu's ⌘Q is held for the same question");
  const elsewhere = resolveKeymap("default", null, false);
  assert.equal(chordFor(elsewhere, "quit"), "Mod+Q");
  assert.equal(chordFor(resolveKeymap("vscode", null, false), "quit"), "Mod+Q");
  assert.equal(chordFor(resolveKeymap("default", null, true), "quit"), null);
  const binding = elsewhere.bindings.find((b) => b.id === "quit");
  assert.equal(interceptsInTerminal(binding, false), true, "taken from a focused shell: Ctrl+Q means XON to a PTY, and quitting is the app's");
  const shortcuts = read("shell/shortcuts.ts");
  assert.ok(shortcuts.includes('export const QUIT_APP = "bisa:quit-app";'), "a door, like the other verbs");
  assert.ok(between(shortcuts, 'case "quit":', "return;").includes("fire(QUIT_APP)"), "dispatched to the door");
  const docs = read("../../docs/reference/keymap.md");
  assert.ok(docs.includes("| `quit` — Quit Bisa"), "in the generated reference");
  assert.ok(docs.includes("Mod+Q · not on macOS"), "and said to be everyone else's");
});
