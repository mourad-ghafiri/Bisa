/**
 * One Bisa at a time, as the sources show it (02 §I64): the shell carries
 * the single-instance plugin and registers it before any other, so a
 * second launch hands its arguments — a `bisa://join/…` link among them —
 * to the running app and ends inside `build`; the node is started from
 * `setup`, after that door, never from `main`; the hand-off brings the
 * window back through the menu bar icon's one door, on the main thread;
 * and the sidecar's wait on a node ends the moment the node does. Source
 * assertions, as `tray.test.mjs` makes them — no DOM. Run with
 * `node --test desktop/src/scenarios/secondLaunch.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const src = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (rel) => readFileSync(join(src, rel), "utf8");

/** The text from one marker up to the next, both of which must be there. */
const between = (text, from, to) => {
  const start = text.indexOf(from);
  assert.ok(start >= 0, `the shell says \`${from}\``);
  const end = text.indexOf(to, start + from.length);
  assert.ok(end > start, `and \`${to}\` after it`);
  return text.slice(start, end);
};

/** The source without its comment lines: what runs, not what is said about it. */
const code = (text) =>
  text
    .split("\n")
    .filter((line) => !line.trimStart().startsWith("//"))
    .join("\n");

test("the shell is single-instance: the official plugin, with the link forwarded, registered before any other", () => {
  const cargo = read("../src-tauri/Cargo.toml");
  const dep = cargo.split("\n").find((l) => l.startsWith("tauri-plugin-single-instance = "));
  assert.ok(dep, "the dependency is the shell's");
  assert.ok(dep.includes('"deep-link"'), "a link opened on Windows or Linux reaches the running app's deep-link plugin");
  const main = read("../src-tauri/src/main.rs");
  assert.ok(main.includes("\nmod second_launch;\n"), "the hand-off is a module of its own");
  const first = main.indexOf(".plugin(");
  assert.ok(first >= 0, "plugins are registered");
  assert.ok(
    main.startsWith(".plugin(tauri_plugin_single_instance::init(second_launch::hand_off))", first),
    "the single-instance plugin is the first plugin: a second launch ends before any other plugin's setup",
  );
});

test("the node is started from setup, after the single-instance door, and never from main", () => {
  const main = code(read("../src-tauri/src/main.rs"));
  assert.ok(!main.includes("NodeState::start("), "main never starts the node itself");
  const beforeBuild = between(main, "fn main() {", "tauri::Builder::default()");
  assert.ok(!beforeBuild.includes("sidecar::") && !beforeBuild.includes("attach_from_binary"), "nothing before build spawns a process or writes a file");
  const setup = between(main, ".setup(|app| {", ".on_window_event(");
  const boot = setup.indexOf("let node = sidecar::boot(app.handle());");
  const managed = setup.indexOf("app.manage(node);");
  const window = setup.indexOf("window_state::WindowState::load(app.handle())");
  assert.ok(boot >= 0 && managed > boot && window > managed, "booted, managed, then the window — a command's State panics on a state nobody manages");
  const exit = between(main, "RunEvent::Exit => {", "_ => {}");
  assert.ok(exit.includes("app.state::<NodeState>().shutdown()"), "the node still goes with Exit");
});

test("a second launch brings the window back through the icon's one door, on the main thread, and logs no link", () => {
  const hand = read("../src-tauri/src/second_launch.rs");
  assert.ok(hand.includes("pub fn hand_off(app: &AppHandle, argv: Vec<String>, _cwd: String)"), "the plugin's callback");
  assert.ok(hand.includes("app.run_on_main_thread(move || crate::tray::show_window(&handle))"), "shown, un-minimised and focused as the menu bar icon does it, on the main thread");
  assert.ok(!hand.includes("get_webview_window("), "a browser tab is a child webview of main — the window is found by its label");
  const logged = between(hand, "tracing::info!(", ");");
  assert.ok(logged.includes("link = link_in(&argv).is_some()") && !logged.includes("cwd"), "whether there was a link, never the link or the folder");
});

test("the sidecar's wait on a node ends the moment the node does, with what it said", () => {
  const sidecar = read("../src-tauri/src/sidecar.rs");
  assert.ok(sidecar.includes("pub fn boot<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> NodeState"), "the boot is the sidecar's");
  assert.ok(sidecar.includes("fn await_health(") && sidecar.includes("Awaited::Exited(exit) =>"), "a child that ended is seen on the tick that sees it");
  const spawn = between(sidecar, "fn spawn_node(", "\n}\n");
  assert.ok(spawn.includes("child.try_wait()") && spawn.includes("before answering its health check"), "the child is read each tick, and its end is said in the reason");
});
