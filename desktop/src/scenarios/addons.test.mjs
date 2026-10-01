/**
 * Addons as the sources show them (18 — Addons): a folder of HTML that runs
 * behind three walls and reaches the app through one door. The journey is
 * driven through the models — install, grant, enable, show, drag, resize,
 * close, disable — reading after each step what the screen would say; the
 * walls are read off the sources — no DOM.
 *
 * Run with `node --test --import ./src/i18n/preload.mjs src/scenarios/addons.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { ADDON_ROUTES, EVENTS, METHOD_NAMES, PERMISSIONS, handleCall, newSession, parseAddonMessage, permissionWord } from "../addons/addonBridgeModel.mjs";
import { defaultPlacement, placementAfterResize, sizeBounds, sizeFrom, windowPrefFrom } from "../addons/addonWindowModel.mjs";
import { grantToggle, isGranted, overlayRows, reviewWords, stateWords, switchWords, titleWords, visibleAddons, withEnabled } from "../addons/addonsModel.mjs";
import { dockBox, placementOf } from "../ui/dockModel.mjs";

const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
const root = new URL("../../../", import.meta.url);

const BUILT_INS = ["calculator", "clock", "cpu", "disk", "gpu", "memory", "needs-you", "pomodoro", "sticky-note", "stopwatch", "tic-tac-toe", "unit-converter", "weather"];

test("the thirteen built-ins are folders the store embeds, each a page that loads the library and nothing remote", () => {
  const dir = new URL("library/addons/", root);
  const folders = readdirSync(dir, { withFileTypes: true }).filter((d) => d.isDirectory()).map((d) => d.name).sort();
  assert.deepEqual(folders, BUILT_INS);
  const catalog = src("../../../crates/bisa-store/src/catalog.rs");
  assert.match(catalog, /the_catalog_ships_thirteen_addons/, "the bundle test pins the folders");
  for (const f of folders) {
    const manifest = JSON.parse(readFileSync(new URL(`${f}/addon.json`, dir), "utf8"));
    assert.equal(manifest.id, f, `${f}: a built-in's id is its folder`);
    assert.equal(manifest.entry ?? "index.html", "index.html");
    assert.equal(manifest.license, "MIT");
    for (const p of manifest.permissions ?? []) assert.ok(permissionWord(p) !== null && PERMISSIONS.includes(permissionWord(p)), `${f}: ${JSON.stringify(p)}`);
    const page = readFileSync(new URL(`${f}/index.html`, dir), "utf8");
    assert.ok(page.includes('src="bisa-addon.js"'), `${f}: the page loads the library`);
    assert.ok(!/https?:\/\//.test(page), `${f}: nothing remote in the page`);
    assert.ok(existsSync(new URL(`${f}/main.js`, dir)), `${f}: a script beside the page`);
    const scripts = readdirSync(new URL(`${f}/`, dir)).filter((n) => n.endsWith(".js")).map((n) => readFileSync(new URL(`${f}/${n}`, dir), "utf8"));
    for (const script of scripts) {
      for (const forbidden of ["XMLHttpRequest", "localStorage", "document.cookie", "WebSocket", "eval(", "window.open("]) {
        assert.ok(!script.includes(forbidden), `${f}: ${forbidden} has no place in an addon — the library is the door`);
      }
      assert.ok(!/(^|[^.\w])fetch\(/.test(script), `${f}: a bare fetch has no place in an addon — bisa.network.fetch is the door`);
    }
    // The clock's lesson: an author `display` wins over `hidden`, and an SVG
    // has no `hidden` at all — so every page fences the attribute, and no
    // script toggles `.hidden` on an SVG element.
    const style = readFileSync(new URL(`${f}/style.css`, dir), "utf8");
    assert.ok(/\[hidden\]\s*\{\s*display:\s*none\s*!important;?\s*\}/.test(style), `${f}: the stylesheet fences [hidden]`);
    const svgIds = [...page.matchAll(/<svg[^>]*\bid="([^"]+)"/g)].map((m) => m[1]);
    for (const id of svgIds) {
      for (const script of scripts) assert.ok(!new RegExp(`\\b${id}\\.hidden\\s*=`).test(script), `${f}: #${id} is an <svg>: it has no hidden`);
    }
  }
});

test("the window is a sandboxed frame with an opaque origin and a token-less source, on the pet's tier", () => {
  const win = src("../addons/AddonWindow.tsx");
  const sandbox = /sandbox="([^"]*)"/.exec(win);
  assert.ok(sandbox, "the frame is sandboxed");
  assert.equal(sandbox[1], "allow-scripts", "scripts, and nothing else — no same-origin, popups, forms or top navigation");
  assert.ok(win.includes("api.addonFileUrl(") && !win.includes("withToken"), "never a token in the frame's URL");
  assert.ok(win.includes("fixed z-30"), "under the notes panel and every dialog, as the pet is");
  const api = src("../api.ts");
  assert.match(api, /addonFileUrl: \(id: string, path: string\) => `\$\{apiBaseSync\(\)\}\/addons\//);
  const app = src("../App.tsx");
  assert.ok(app.indexOf("<PetCompanion />") < app.indexOf("<AddonLayer />"), "the layer mounts after the pet, in its own boundary");
  assert.ok(app.includes('name="addons"'));
  const bar = src("../shell/StatusBar.tsx");
  assert.ok(bar.includes("<AddonsStat />"), "the footer's read-out is mounted");
  const stat = src("../shell/AddonsStat.tsx");
  assert.ok(stat.includes("open && <AddonsOverlay"), "content mounted only while open");
  const layer = src("../addons/AddonLayer.tsx");
  assert.ok(!layer.includes("useSurface"), "a window is never a surface: it yields to a browser layer by intersection instead");
  // One source for what shows: the store's `switchedOn` (the node's word,
  // re-read on the setting's frame), read by the layer, the read-out and the
  // popover through one rule; the sync mounted once.
  const store = src("../addons/addonsStore.ts");
  const overlay = src("../shell/AddonsOverlay.tsx");
  const panel = src("../views/_settings/AddonsPanel.tsx");
  for (const [name, text] of [["AddonLayer", layer], ["AddonsStat", stat], ["AddonsOverlay", overlay]]) {
    assert.ok(text.includes("visibleAddons(") || text.includes("overlayRows("), `${name} reads the one rule`);
    assert.ok(!text.includes("addons_enabled") && !text.includes("useResolvedSettings"), `${name} reads the switch from the store alone`);
  }
  assert.ok(store.includes('e.payload.type === "settings_changed"') && store.includes("ADDONS_ENABLED_KEY"), "the store re-reads when the machine's switch moves");
  assert.ok(store.includes("withEnabled(state.addons, id, enabled)") && store.includes("withEnabled(state.addons, id, was)"), "a switch is optimistic and comes back on a refusal — that switch alone, on the list as it stands");
  assert.ok(layer.includes("useAddonsSync()") && !panel.includes("useAddonsSync") && !stat.includes("useAddonsSync") && !overlay.includes("useAddonsSync"), "the sync is mounted once, by the layer");
  const bridge = src("../addons/addonBridge.ts");
  assert.ok(bridge.includes("[addon.id, addon.installed_at, grantsKey]") && !bridge.includes("}, [addon]);"), "the bridge's session lives as long as its frame — never reset by a re-read");
  const windowSrc = src("../addons/AddonWindow.tsx");
  assert.ok(windowSrc.includes("useDockDrag(box, viewport, setDock)") && windowSrc.includes("wasDragging.current && !drag.dragging"), "a drag writes once, when it ends");
  assert.ok(!windowSrc.includes("onShield(drag.dragging), [drag.dragging"), "the shield moves on a drag's edges, never on mount");
  assert.ok(panel.includes('t("settings-addons-panel-import-addon")') && !panel.includes("import-folder"), "the panel imports an addon, not a folder");
});

test("the bridge trusts the frame's own window alone and renders nothing an addon says as HTML", () => {
  const bridge = src("../addons/addonBridge.ts");
  assert.ok(bridge.includes("e.source !== frame.contentWindow"), "a message from any other window is dropped unread");
  assert.ok(bridge.includes('postMessage(message, "*")'), "the origin is opaque: no string names it");
  assert.ok(!bridge.includes("innerHTML") && !bridge.includes("dangerouslySetInnerHTML"));
  const sdk = readFileSync(new URL("addons/sdk/bisa-addon.js", root), "utf8");
  for (const m of METHOD_NAMES) assert.ok(sdk.includes(`"${m}"`), `the library names ${m}`);
  for (const e of EVENTS) assert.ok(sdk.includes(`"${e}"`), `the library names ${e}`);
  const node = src("../../../crates/bisa-node/src/addons.rs");
  assert.ok(node.includes("connect-src 'none'") && node.includes("sandbox allow-scripts"), "the node's policy on every file");
  assert.ok(node.includes(`include_str!("../../../addons/sdk/bisa-addon.js")`), "the library served is the platform's");
  const auth = src("../../../crates/bisa-node/src/auth.rs");
  assert.ok(auth.includes("is_addon_file(path)"), "the files route is the one open door");
  const shell = src("../../../desktop/src-tauri/src/navigation.rs");
  assert.ok(shell.includes("/addons/") && shell.includes("/files/"), "the shell refuses every other navigation");
});

test("the reference names every method and every event the bridge knows, and nothing else", () => {
  const doc = readFileSync(new URL("docs/reference/addon-api.md", root), "utf8");
  const table = doc.slice(doc.indexOf("## Methods"), doc.indexOf("## Events"));
  const documentedMethods = [...table.matchAll(/^\| `([a-z]+(?:\.[a-zA-Z]+)*)` \|/gm)].map((m) => m[1]);
  assert.deepEqual(documentedMethods.sort(), [...METHOD_NAMES].sort(), "the method table equals the registry");
  for (const e of EVENTS) assert.ok(doc.includes(`\`${e}\``), `the event ${e} is in the reference`);
  for (const r of ADDON_ROUTES) assert.ok(doc.includes(`\`${r}\``), `the route ${r} is in the reference`);
});

test("a person installs, grants, shows, moves, resizes, closes and switches off an addon", () => {
  // Install: the review names what the weather addon asks for.
  const manifest = JSON.parse(readFileSync(new URL("library/addons/weather/addon.json", root), "utf8"));
  const review = reviewWords(manifest);
  assert.match(review.title, /Weather/);
  assert.equal(review.lines.length, 2);
  assert.ok(review.lines.some((l) => l.includes("api.open-meteo.com")), "the hosts are named before anything is granted");

  // Imported with nothing granted: it runs, and every call is refused with a sentence.
  let addon = { id: "acme.byte", manifest: { ...manifest, id: "acme.byte" }, origin: "local", enabled: true, granted: [], installed_at: 1, files_present: true, active: true };
  let session = { ...newSession(addon), ready: true };
  const ctx = { now: 1, storage: {}, platform: { version: "0", locale: "en" }, scheme: "light", summary: null };
  const refused = handleCall(session, { id: "c1", method: "storage.set", params: { key: "city", value: "Lisbon" } }, ctx);
  assert.equal(refused.reply.ok, false);
  assert.equal(refused.reply.error.code, "permission");
  assert.ok(refused.reply.error.message.length > 0, "a sentence, not a code");

  // Grant storage: the toggle adds exactly the declaration.
  const declared = manifest.permissions;
  const granted = grantToggle(addon.granted, declared, "storage", true);
  assert.deepEqual(granted, ["storage"]);
  assert.equal(isGranted(granted, "storage"), true);
  addon = { ...addon, granted };
  session = { ...newSession(addon), ready: true };
  const stored = handleCall(session, { id: "c2", method: "storage.set", params: { key: "city", value: "Lisbon" } }, ctx);
  assert.equal(stored.reply.ok, true);
  assert.deepEqual(stored.storage, { city: "Lisbon" });
  // Network stays refused until granted; granted, it only ever goes to the node.
  assert.equal(handleCall(session, { id: "c3", method: "network.fetch", params: { url: "https://api.open-meteo.com/x" } }, ctx).reply.error.code, "permission");
  const withNet = { ...newSession({ ...addon, granted: grantToggle(granted, declared, declared[0], true) }), ready: true };
  const fetch = handleCall(withNet, { id: "c4", method: "network.fetch", params: { url: "https://api.open-meteo.com/x" } }, ctx);
  assert.equal(fetch.reply, null);
  assert.equal(fetch.effect.type, "fetch");

  // Show: the footer counts it; the layer draws it where the manifest opens it.
  assert.deepEqual(visibleAddons([addon], [], true, true).map((a) => a.id), ["acme.byte"]);
  assert.equal(titleWords(1, 1), "1 of 1 addons showing");
  assert.equal(stateWords(addon), "running");
  const viewport = { width: 1200, height: 800, top: 40 };
  const opening = defaultPlacement(manifest.window.default_dock, 0);
  const bounds = sizeBounds(manifest.window, viewport);
  let pref = windowPrefFrom(null, { width: manifest.window.width, height: manifest.window.height }, opening, bounds);
  assert.deepEqual(pref.dock, { h: "right", x: 160, v: "bottom", y: 24 });

  // Drag: the box moves, the placement re-anchors to the nearest edges.
  const vp = { ...viewport, sizeX: pref.size.width, sizeY: pref.size.height };
  const box = dockBox(pref.dock, vp);
  const moved = placementOf({ left: box.left - 700, top: box.top - 500 }, vp);
  assert.equal(moved.h, "left");
  assert.equal(moved.v, "top");

  // Resize from the corner: the size grows inside the manifest's bounds, the other corner stays.
  const grown = sizeFrom(pref.size, 500, 500, bounds);
  assert.deepEqual(grown, { width: manifest.window.max_width, height: manifest.window.max_height });
  const after = placementAfterResize(pref.dock, pref.size, grown);
  assert.equal(after.h, "right");
  assert.ok(after.x < pref.dock.x, "a right-anchored window grows inward");
  pref = windowPrefFrom(JSON.stringify({ dock: after, size: grown }), pref.size, opening, bounds);
  assert.deepEqual(pref.size, grown, "what the person left is what comes back");

  // Close: put away, it leaves the layer and the footer says so; the eye brings it back.
  assert.deepEqual(visibleAddons([addon], ["acme.byte"], true, true), []);
  assert.equal(titleWords(0, 1), "0 of 1 addons showing");
  assert.equal(overlayRows([addon], ["acme.byte"], true, true)[0].putAway, true);
  assert.equal(switchWords(false).label, "Show addons");
  assert.deepEqual(visibleAddons([addon], [], false, true), [], "the layer's switch hides every window");

  // Disable — from the footer's popover as from Settings: the list reads as
  // the node will, nothing draws, its files are still here; the row stays
  // listed with its switch, so it can be turned on again from the same place.
  const off = withEnabled([addon], "acme.byte", false)[0];
  assert.equal(off.enabled, false);
  assert.equal(off.active, false, "enabled off means not active, as the node computes it");
  assert.equal(stateWords(off), "off");
  assert.deepEqual(visibleAddons([off], [], true, true), []);
  const rows = overlayRows([off], [], true, true);
  assert.equal(rows.length, 1);
  assert.deepEqual([rows[0].running, rows[0].shown, rows[0].canEnable], [false, false, true]);
  assert.equal(titleWords(0, 1), "0 of 1 addons showing", "an installed addon that is off still counts as installed");
  assert.equal(withEnabled([off], "acme.byte", true)[0].active, true);
  // A stranger's message never becomes a call.
  assert.equal(parseAddonMessage({ v: 1, kind: "bisa:call", id: "x", method: "storage.get", params: {} }).kind, "call");
  assert.equal(parseAddonMessage({ kind: "bisa:call", id: "x", method: "storage.get" }), null, "no version, no reading");
});
