/**
 * The node lives and dies with the desktop, and a boot is waited for while
 * it works — the sources held to it (guide/operating.md §After a crash,
 * architecture/10-runtime-flows.md §Boot). The shell starts its node
 * leashed and speaking its boot, waits as long as it keeps speaking, says
 * every phase and every failure to the webview, stops a node gracefully
 * before it kills it, and takes a stray node of its own back; the webview
 * says the shell's account wherever the node's absence is said, and the
 * root crash card works without the node.
 */
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

import { NODE_EVENTS } from "../shell/nodeBootModel.mjs";

const src = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (rel) => readFileSync(join(src, rel), "utf8");
const between = (text, start, end) => {
  const i = text.indexOf(start);
  assert.ok(i >= 0, `\`${start}\` is in the source`);
  const j = text.indexOf(end, i + start.length);
  assert.ok(j >= 0, `\`${end}\` follows \`${start}\``);
  return text.slice(i, j);
};

test("the shell starts its node leashed and speaking its boot, and waits for nothing in the spawn", () => {
  const sidecar = read("../src-tauri/src/sidecar.rs");
  const spawn = between(sidecar, "fn spawn_node(", "\n}\n");
  assert.ok(spawn.includes('.env("BISA_STOP_ON_STDIN_CLOSE", "1")'), "the leash: the node stops when the shell's end of its stdin goes");
  assert.ok(spawn.includes(".stdin(Stdio::piped())") && spawn.includes(".stdout(Stdio::piped())"), "stdin held, stdout read");
  assert.ok(spawn.includes('["--json", "node", "--listen", &listen]'), "the boot's phases come as JSON lines");
  assert.ok(/let leash = child\s*\.stdin\s*\.take\(\)/.test(spawn), "the leash is kept with the child");
  assert.ok(!spawn.includes("try_wait") && !spawn.includes("sleep"), "nothing waited for here: the window opens at once");
  assert.ok(sidecar.includes("pub const BOOT_BUDGET: Duration = Duration::from_secs(180);"), "three minutes since the node's last word");
  assert.ok(sidecar.includes("pub const BOOT_CAP: Duration = Duration::from_secs(15 * 60);"), "fifteen minutes in all");
  assert.ok(/fn boot_budget_exhausted\(\s*now: Instant,\s*spawned: Instant,\s*last_word: Option<Instant>,?\s*\) -> bool/.test(sidecar), "pure, so a test holds it");
  assert.ok(!sidecar.includes("fn await_health("), "no twenty-second wait kills a rebuild any more");
});

test("the three events are spelt the same on both sides, and the shell says every phase and every failure", () => {
  const sidecar = read("../src-tauri/src/sidecar.rs");
  assert.ok(sidecar.includes(`pub const NODE_RESTARTED: &str = "${NODE_EVENTS.restarted}";`));
  assert.ok(sidecar.includes(`pub const NODE_BOOT: &str = "${NODE_EVENTS.boot}";`));
  assert.ok(sidecar.includes(`pub const NODE_FAILED: &str = "${NODE_EVENTS.failed}";`));
  const supervise = between(sidecar, "pub fn supervise<R: tauri::Runtime>(", "\n    }\n");
  assert.ok(supervise.includes("app.emit(NODE_BOOT, word)"), "every boot word reaches the webview as it is said");
  assert.ok(supervise.includes("app.emit(NODE_BOOT, BootWord::ready())"), "and readiness");
  assert.ok(supervise.includes("app.emit(NODE_RESTARTED, restarted)"), "a restart once the node answers");
  assert.ok(sidecar.includes("app.emit(NODE_FAILED, failed)"), "a failure, with when the next try comes");
  assert.ok(sidecar.includes('#[serde(tag = "kind", rename_all = "snake_case")]\npub enum Failure {'), "the failure's kind on the wire");
  for (const kind of ["Exited {", "TimedOut {", "NoBinary {", "HeldByOther {"]) assert.ok(sidecar.includes(kind), `${kind} is a kind`);
  const store = read("shell/nodeBootStore.ts");
  for (const name of ["boot", "failed", "restarted"]) assert.ok(store.includes(`NODE_EVENTS.${name}`), `the webview listens for ${name}`);
  assert.ok(store.includes("forgetApiBase();"), "a restart drops the cached base");
  assert.ok(read("main.tsx").includes("installNodeBootStore();"), "installed once, above the boundary");
  assert.ok(!read("main.tsx").includes('"node:restarted"'), "the one listener is the store's");
});

test("a node is stopped gracefully before it is killed, a stray node of the desktop's own is taken back, and a person's is never signalled", () => {
  const sidecar = read("../src-tauri/src/sidecar.rs");
  const stop = between(sidecar, "fn stop(child: &mut Child, leash: Option<ChildStdin>, grace: Duration) {", "\n}\n");
  const order = ["drop(leash);", "sysinfo::Signal::Term", "child.try_wait()", "child.kill()", "child.wait()"];
  let at = -1;
  for (const marker of order) {
    const next = stop.indexOf(marker);
    assert.ok(next > at, `the leash, SIGTERM, the wait, then the kill: \`${marker}\``);
    at = next;
  }
  assert.ok(sidecar.includes("const RESTART_GRACE: Duration = Duration::from_secs(10);") && sidecar.includes("const QUIT_GRACE: Duration = Duration::from_secs(5);"), "its grace");
  assert.ok(sidecar.includes("fn reclaim_decision(") && sidecar.includes("Reclaim::HeldByOther(holder.pid)"), "the decision is pure");
  assert.ok(sidecar.includes('data_dir.join("run").join("desktop-node.json")'), "the record of the node this desktop spawned");
  assert.ok(sidecar.includes("facts.started_at.abs_diff(own.spawned_at) <= RECLAIM_START_SLACK"), "a pid the OS reused is never ours");
  const launch = between(sidecar, "fn launch(&self, restart: bool, requested: bool) -> Option<NodeFailed> {", "\n    }\n");
  assert.ok(launch.includes("Reclaim::StopOurs(pid) =>") && launch.includes("stop_stray(pid);"), "ours is stopped before a new one starts");
  assert.ok(launch.includes("Reclaim::HeldByOther(pid) =>") && launch.includes("Failure::HeldByOther { pid }"), "another's is named and left alone");
  assert.ok(!launch.includes("stop_stray(holder") , "and never signalled");
  assert.ok(read("../src-tauri/src/main.rs").includes("async fn restart_node(app: tauri::AppHandle) -> Result<NodeStatus, String> {"), "a restart runs off the main thread");
  const cli = read("../../crates/bisa-cli/src/main.rs");
  assert.ok(cli.includes('"BISA_STOP_ON_STDIN_CLOSE"'), "the node reads the leash");
  assert.ok(cli.includes('"parent gone"'), "and names why it stopped");
  assert.ok(cli.includes('"engine_holder"'), "paths --json names the holder");
});

test("the webview says the shell's account wherever the node's absence is said, and the root card works without the node", () => {
  const hook = read("shell/useWorkspaceData.ts");
  assert.ok(hook.includes("offlineLine(offline, conn, nodeFailureReason(), bootLine(boot))"), "the sidebar's footer and the rail's tooltip read it through the workspace");
  assert.ok(hook.includes("LOAD_SHAPES,"), "and an answer of the wrong shape never throws there");
  const sidebar = read("shell/Sidebar.tsx");
  const footer = between(sidebar, "function SidebarFooter() {", "\n}\n");
  assert.ok(footer.includes("nodeDoors(boot).map(") && footer.includes("openNodeDoor(door)"), "the doors beside the line");
  assert.ok(footer.includes("failure?.details") && footer.includes("<details"), "what the node said, in a fold");
  assert.ok(read("views/_settings/NodePanel.tsx").includes("bootLine(boot)"), "Settings › Node says it too");
  const main = read("main.tsx");
  assert.ok(main.includes("resetKey={address}"), "the root boundary resets on the address: moving on is a way out");
  assert.ok(main.includes("<RootCrashCard error={error} reset={reset} />"), "the card is the fallback");
  const card = read("shell/RootCrashCard.tsx");
  for (const door of ["reset();", "window.location.reload();", "await restartNode();", "await revealLog();", "await revealDataFolder();", 'requestQuit("crash");']) assert.ok(card.includes(door), `the door: ${door}`);
  assert.ok(card.includes("rootCrash.markCrashed(error);") && card.includes("rootCrash.clearCrash()"), "mounted is crashed");
  assert.ok(!card.includes("useToast") && !card.includes("useWorkspace"), "nothing from the tree that fell");
  const api = read("api.ts");
  const reveal = between(api, "export async function revealLog(): Promise<void> {", "\n}\n");
  assert.ok(reveal.includes('invoke("reveal_logs_dir")'), "the log's folder from the shell, no node needed");
  assert.ok(api.includes('invoke("reveal_data_dir")'), "and the data folder");
  const folders = read("../src-tauri/src/folders.rs");
  assert.ok(folders.includes("pub fn reveal_logs_dir(") && folders.includes("pub fn reveal_data_dir("), "the shell's two commands");
  assert.ok(!folders.includes("terminal::get("), "neither asks the node");
  const handler = between(read("../src-tauri/src/main.rs"), "invoke_handler(", ".build(context)");
  assert.ok(handler.includes("folders::reveal_logs_dir,") && handler.includes("folders::reveal_data_dir,"), "registered");
});

test("the kit's crash boundary speaks the catalog and is no longer exempt from the ratchet", () => {
  const boundary = read("ui/ErrorBoundary.tsx");
  for (const id of ["ui-error-boundary-screen-hit-error", "ui-error-boundary-rest-of-app-fine", "ui-error-boundary-details-in-log", "ui-error-boundary-try-again", "ui-error-boundary-reload", "ui-error-boundary-reveal-log", "ui-error-boundary-overlay-closed"]) {
    assert.ok(boundary.includes(`t("${id}"`), `${id} is said`);
  }
  assert.ok(!read("i18n/ratchetModel.mjs").includes('"ui/ErrorBoundary.tsx"'), "a file that speaks the catalog needs no exemption");
  const toast = read("ui/Toast.tsx");
  assert.ok(toast.includes("class RailBoundary extends Component") && toast.includes("<RailBoundary>"), "the toast rail has a silent boundary of its own");
});
