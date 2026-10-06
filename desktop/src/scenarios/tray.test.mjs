/**
 * The menu bar icon as the sources show it: the shell is built with the
 * tray, holds both doors and spells the events the webview listens for; the
 * red button hides or quits and never destroys; the Dock badge and the icon
 * wear the one count; the mark is one file rasterised once; the two switches
 * are registered. Source assertions, as `browser.test.mjs` makes them — no
 * DOM. Run with `node --test desktop/src/scenarios/tray.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { TRAY_EVENTS, TRAY_KEYS } from "../shell/trayModel.mjs";

const src = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (rel) => readFileSync(join(src, rel), "utf8");

test("the shell is built with the tray, and the icon is the shell's — no tray capability, no destroy, the shell's own hide", () => {
  assert.ok(read("../src-tauri/Cargo.toml").includes('"tray-icon"'), "the tauri feature");
  const cap = JSON.parse(read("../src-tauri/capabilities/default.json"));
  assert.ok(!cap.permissions.includes("core:window:allow-hide"), "the webview never hides the window itself: the shell's hide_window decides how the app is put away");
  assert.ok(!cap.permissions.includes("core:window:allow-destroy"), "a close never destroys the window");
  assert.ok(cap.permissions.includes("core:window:allow-set-badge-count"), "the Dock badge");
  assert.ok(!cap.permissions.some((p) => String(p).startsWith("core:tray")), "the webview never touches the icon itself");
  const main = read("../src-tauri/src/main.rs");
  assert.ok(main.includes("mod tray;") && main.includes("tray::install(app.handle())") && main.includes("app.manage(tray)"), "installed in setup, held as state");
  assert.ok(main.includes("tray_report,") && main.includes("tray_dock,") && main.includes("hide_window,"), "the three commands are registered");
});

test("every door back finds the window as a window — a browser tab is a child webview of `main`, and `get_webview_window` would find nothing", () => {
  // Tauri's `get_webview_window` answers only for a window whose one webview
  // is its own; the IDE's browser tabs are `add_child`ed to `main`. The one
  // lookup is `main_window` over `get_window`, and nothing in the shell may
  // reach for the other.
  const shell = join(src, "../src-tauri/src");
  const walk = (dir) => readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(join(dir, e.name)) : e.name.endsWith(".rs") ? [join(dir, e.name)] : []));
  const offenders = walk(shell).filter((p) => readFileSync(p, "utf8").includes("get_webview_window("));
  assert.deepEqual(offenders, [], "the shell looks the main window up as a window, never as a webview window");
  const tray = read("../src-tauri/src/tray/mod.rs");
  assert.ok(tray.includes("pub fn main_window(app: &AppHandle) -> Option<Window>") && tray.includes("app.get_window(MAIN_WINDOW)"), "one lookup, by the window's label");
  assert.ok((tray.match(/main_window\(app\)/g) ?? []).length >= 4, "show, quit, hide and the ink read it");
  const main = read("../src-tauri/src/main.rs");
  assert.ok(main.includes("tray::main_window(app)"), "⌘Q's held door reads it too");
  const browser = read("../src-tauri/src/browser.rs");
  assert.ok(browser.includes("pub(crate) fn label_of") && browser.includes('format!("browser-{key}")'), "a tab's label is its own, never main");
  // How the app is put away is the platform's: the app on macOS, so ⌘Tab and
  // the Dock bring it back; the window elsewhere.
  const platform = read("../src-tauri/src/tray/platform.rs");
  assert.ok(platform.includes("fn conceal(&self, app: &AppHandle, window: &Window)"), "conceal is the trait's");
  assert.ok(platform.includes("app.hide()") && platform.includes("window.hide()"), "the app on macOS, the window elsewhere");
  assert.ok(tray.includes("pub fn hide_window(app: &AppHandle)") && tray.includes("platform::current().conceal(app, &window)"), "the command hands it to the platform");
  const guard = read("shell/useCloseGuard.ts");
  assert.ok(guard.includes('invoke("hide_window")') && !guard.includes("getCurrentWindow().hide()"), "the webview asks the shell, never the window");
});

test("both doors are held by the shell and decided by the webview under the one spelling", () => {
  const main = read("../src-tauri/src/main.rs");
  assert.ok(main.includes("WindowEvent::CloseRequested { api, .. }") && main.includes("api.prevent_close()") && main.includes("window.emit(CLOSE_REQUESTED, ())"), "the red button is held and handed over");
  assert.ok(main.includes("RunEvent::ExitRequested {") && main.includes("api.prevent_exit()") && main.includes("window.emit(QUIT_REQUESTED, ())"), "Tauri's own exit request — a destroyed last window — is held and handed over; ⌘Q itself is AppKit's terminate, held in quit.rs (scenarios/quit.test.mjs)");
  assert.ok(!main.includes("WindowEvent::Destroyed"), "nothing waits on a destroyed window any more");
  assert.ok(main.includes("RunEvent::Reopen {") && main.includes("has_visible_windows: false"), "the Dock icon brings the window back");
  const guard = read("shell/useCloseGuard.ts");
  assert.ok(guard.includes("TRAY_EVENTS.close") && guard.includes("TRAY_EVENTS.quit"), "the webview listens for both");
  assert.ok(guard.includes("closeVerb(trayPrefs())") && guard.includes("hide()"), "hide while the switch is on");
  assert.ok(!guard.includes("onCloseRequested") && !guard.includes("destroy()"), "no close of its own, no destroy");
  assert.ok(guard.includes('invoke("quit_app")'), "every quit ends in quit_app");
  const tray = read("../src-tauri/src/tray/mod.rs");
  assert.ok(tray.includes("window.emit(crate::QUIT_REQUESTED, ())"), "the menu's Quit goes through the same flow");
  for (const id of ["tray:status", "tray:needs", "tray:open", "tray:dock", "tray:quit"]) assert.ok(tray.includes(`"${id}"`), id);
  assert.ok(tray.includes(`"${TRAY_EVENTS.go}"`) && tray.includes(`"${TRAY_EVENTS.dock}"`));
  assert.ok(tray.includes("show_menu_on_left_click(false)") && tray.includes("MouseButton::Left"), "a left click opens the window; the menu is the right click's");
});

test("one count outside the window: the tray hook wears the Dock badge, notifications only notify", () => {
  const tray = read("shell/useTray.ts");
  assert.ok(tray.includes("setBadgeCount(") && tray.includes('invoke("tray_report"'), "the badge and the report, from one report");
  assert.ok(tray.includes("trayReport({") && tray.includes("useWorkspace()") && tray.includes("useSettledSessions()"), "the roster as the tabs say it: a harness whose tab exited is not working");
  assert.ok(tray.includes("TRAY_EVENTS.go") && tray.includes('navigate({ name: "inbox" })'), "the needs line lands on the Inbox");
  assert.ok(tray.includes("TRAY_EVENTS.dock") && tray.includes(`[TRAY_KEYS.dockIcon]`), "the menu's Dock toggle becomes the setting");
  const notes = read("shell/notifications.ts");
  assert.ok(!notes.includes("setBadgeCount") && !notes.includes("useSessions"), "notifications no longer count");
  // One door for every notice — the shell's, an addon's, the app's own — and
  // the person's switches (Settings › System › Notifications) read at it.
  assert.ok(notes.includes("allowed(notifyPrefs(), notice.category)"), "the switches are honoured at the door");
  assert.ok(notes.includes('category: "addons"') && notes.includes('category: "app"'), "an addon's notice and the app's own wear their categories");
  const guard = read("shell/useCloseGuard.ts");
  assert.ok(guard.includes("deliverAppNotice(") && !guard.includes("sendNotification"), "the close guard's notice goes through the door");
  assert.ok(read("addons/addonBridge.ts").includes("deliverNotice(effect.title, effect.body)"), "the bridge hands its notify effect to the door");
  const appSrc = read("App.tsx");
  assert.ok(appSrc.includes("useNotifySettings();") && appSrc.includes("useNotifications();"), "the switches are followed once, in App");
  assert.ok(read("views/_settings/SystemPanel.tsx").includes("<NotificationsCard"), "the switches are drawn under System");
  const settingsRs = read("../../crates/bisa-core/src/settings.rs");
  for (const key of ["notifications.enabled", "notifications.asks", "notifications.failures", "notifications.done", "notifications.workflows", "notifications.addons"]) assert.ok(settingsRs.includes(`"${key}",`), key);
  assert.ok(!settingsRs.includes("agents.notify"), "the old keys are gone");
  const prefs = read("shell/trayPrefs.ts");
  assert.ok(prefs.includes('invoke<{ visible: boolean; supported: boolean; detail?: string }>("tray_dock"'), "the setting applies itself to the Dock");
  const app = read("App.tsx");
  assert.ok(app.includes("useTrayPrefs();") && app.includes("useTray(conn);"), "both mounted once, in App");
  const model = read("shell/trayModel.mjs");
  assert.ok(model.includes("inboxBadge(inbox).needs"), "the count is the Inbox's rows that need you");
});

test("the mark is one file rasterised once, and the two switches are registered under Desktop", () => {
  assert.ok(existsSync(join(src, "..", "..", "logo", "tray-mark.svg")), "logo/tray-mark.svg");
  const svg = readFileSync(join(src, "..", "..", "logo", "tray-mark.svg"), "utf8");
  assert.ok(svg.includes('fill="#000"') && !svg.includes("Gradient") && !svg.includes("<filter"), "one ink, no glass");
  assert.ok(read("../src-tauri/src/tray/glyph.rs").includes('include_bytes!("../../icons/tray/36x36.png")'), "the shell ships the raster");
  const just = readFileSync(join(src, "..", "..", "Justfile"), "utf8");
  assert.ok(just.includes("tray-icon:") && just.includes("npx tauri icon ../logo/tray-mark.svg -o src-tauri/icons/tray -p 36"), "one recipe writes it");
  const registry = readFileSync(join(src, "..", "..", "crates", "bisa-core", "src", "settings.rs"), "utf8");
  for (const key of Object.values(TRAY_KEYS)) assert.ok(registry.includes(`"${key}",`), key);
  assert.ok(read("views/_settings/settingsLink.mjs").includes('panel("desktop", t("screens-settings-desktop"), t("screens-settings-desktop-app-s-own-behaviour-machine"))'), "the Desktop tab says so");
});
