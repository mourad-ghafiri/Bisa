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
/** Where `needle` stands in `text` — and that it stands there at all, so an order pin never passes on two absences. */
const at = (text, needle) => {
  const i = text.indexOf(needle);
  assert.notEqual(i, -1, `missing: ${needle}`);
  return i;
};
const root = new URL("../../../", import.meta.url);

const BUILT_INS = ["bug-hunt", "calculator", "clock", "cpu", "disk", "gpu", "linefeed", "linerun", "media-player", "memory", "needs-you", "pomodoro", "sticky-note", "stopwatch", "tetrominal", "tic-tac-toe", "unit-converter", "weather"];

test("the eighteen built-ins are folders the store embeds, each a page that loads the library and nothing remote", () => {
  const dir = new URL("library/addons/", root);
  const folders = readdirSync(dir, { withFileTypes: true }).filter((d) => d.isDirectory()).map((d) => d.name).sort();
  assert.deepEqual(folders, BUILT_INS);
  const catalog = src("../../../crates/bisa-store/src/catalog.rs");
  assert.match(catalog, /the_catalog_ships_eighteen_addons/, "the bundle test pins the folders");
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

test("the three terminal games share one kit, word for word, so a fix lands in all three", () => {
  const dir = new URL("library/addons/", root);
  const slice = (text, open, close) => {
    const a = text.indexOf(open), b = text.indexOf(close);
    assert.ok(a >= 0 && b > a, `${open} … ${close}`);
    return text.slice(a, b + close.length);
  };
  const games = ["tetrominal", "linefeed", "linerun"];
  const scripts = games.map((g) => slice(readFileSync(new URL(`${g}/main.js`, dir), "utf8"), "// --- kit ---", "// --- end kit ---"));
  const styles = games.map((g) => slice(readFileSync(new URL(`${g}/style.css`, dir), "utf8"), "/* --- kit --- */", "/* --- end kit --- */"));
  for (let i = 1; i < games.length; i++) {
    assert.equal(scripts[i], scripts[0], `${games[i]}: the script's kit is tetrominal's`);
    assert.equal(styles[i], styles[0], `${games[i]}: the stylesheet's kit is tetrominal's`);
  }
  for (const name of ["Screen", "Keys", "Clock", "Bell", "Prompt", "Fit", "WaitRow", "Store", "Help"]) assert.ok(scripts[0].includes(`function ${name}(`), name);
  assert.ok(styles[0].includes("--mono:") && styles[0].includes("prefers-reduced-motion"));
});

test("the window is a sandboxed frame with an opaque origin and a token-less source, on the pet's tier", () => {
  const win = src("../addons/AddonWindow.tsx");
  const windowSrc = win;
  const sandbox = /sandbox="([^"]*)"/.exec(win);
  assert.ok(sandbox, "the frame is sandboxed");
  assert.equal(sandbox[1], "allow-scripts", "scripts, and nothing else — no same-origin, popups, forms or top navigation");
  assert.ok(win.includes("api.addonFileUrl(") && !win.includes("withToken"), "never a token in the frame's URL");
  assert.ok(win.includes("fixed z-30"), "under the notes panel and every dialog, as the pet is");
  const api = src("../api.ts");
  assert.match(api, /addonFileUrl: \(id: string, path: string\) => `\$\{apiBaseSync\(\)\}\/addons\//);
  const app = src("../App.tsx");
  const host = src("../addons/AddonsHost.tsx");
  assert.ok(at(app, "<PetCompanion />") < at(app, "<AddonsHost "), "the addons mount after the pet");
  assert.ok(host.includes('name="addons"') && host.includes('name="addons sync"'), "the layer and the list's sync each in a boundary of their own");
  assert.ok(host.includes("resetKey={`${resetKey}:${layerShown}`}"), "the layer's switch revives a layer that fell");
  const bar = src("../shell/StatusBar.tsx");
  assert.ok(bar.includes("<AddonsStat />"), "the footer's read-out is mounted");
  const stat = src("../shell/AddonsStat.tsx");
  assert.ok(stat.includes("open && <AddonsOverlay"), "content mounted only while open");
  const layer = src("../addons/AddonLayer.tsx");
  assert.ok(!layer.includes("useSurface"), "a window is never a surface: the browser layer cuts around it where it can (macOS), and it yields by intersection elsewhere");
  const addonWindow = src("../addons/AddonWindow.tsx");
  assert.ok(addonWindow.includes("useBrowserClear(`addon:${addon.id}`, shell, visible)") && addonWindow.includes("hiddenByLayer(box, total, slots, cutsAround)") && addonWindow.includes("useBrowserCutsAround()"), "a window says its box to the browser layer, and hides by intersection only where the layer cannot cut");
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
  assert.ok(store.includes("withEnabled(state.addons, id, enabled)") && store.includes("restoreAddon(state.addons, prior)"), "a switch is optimistic and a refusal puts the object held before back — that addon alone, on the list as it stands");
  assert.ok(host.includes("useAddonsSync()") && !layer.includes("useAddonsSync") && !panel.includes("useAddonsSync") && !stat.includes("useAddonsSync") && !overlay.includes("useAddonsSync"), "the sync is mounted once, by the host, outside the layer's boundary");
  assert.ok(layer.includes("<AddonBoundary key={addon.id} addon={addon}>") && layer.includes("setAddonHidden(addon.id, true);"), "each addon in a boundary of its own, put away when it throws");
  const bridge = src("../addons/addonBridge.ts");
  assert.ok(bridge.includes("}, [epoch, session]);") && !bridge.includes("}, [addon]);"), "the bridge's session lives as long as its frame — never reset by a re-read");
  assert.ok(bridge.includes("session.current = { ...resetSession(session.current, latest.current.addon), ready: true };"), "a hello is a page loaded afresh: the session starts over, its clocks kept");
  assert.ok(bridge.includes("}, START_DEADLINE_MS);") && windowSrc.includes('onStartMissed: () => setStart("missed")'), "a page that never says hello is said in the box");
  const mediaHost = src("../addons/addonMediaHost.ts");
  assert.ok(mediaHost.includes("const chains = new Map<string, Promise<void>>();") && mediaHost.includes("enqueue(chains, key, step)"), "one chain per key, across instances");
  assert.ok(at(mediaHost, "await closeBrowserView(key).catch(nothingToClose(key));") < at(mediaHost, "await openBrowserPage(key, html, NO_SCRIPT, false);"), "the key is closed before it is opened");
  assert.ok(windowSrc.includes("useDockDrag(box, viewport, setDock)") && windowSrc.includes("wasDragging.current && !drag.dragging"), "a drag writes once, when it ends");
  assert.ok(!windowSrc.includes("onShield(drag.dragging), [drag.dragging"), "the shield moves on a drag's edges, never on mount");
  assert.ok(windowSrc.includes("frameKeyOf(addon, reloads)") && src("../addons/AddonScene.tsx").includes("frameKeyOf(addon)"), "the frame's key is the model's, on both kinds");
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
  assert.equal(stored.reply, null, "the host answers with whether this machine's storage took it");
  assert.deepEqual(stored.effect, { type: "storage_set", id: "c2" });
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
  // On again: the switch reads on and the row says *starting*; the window
  // waits for the node's word — mounted sooner, its frame asked the node for
  // files the node would not serve yet, and never asked again.
  const back = withEnabled([off], "acme.byte", true)[0];
  assert.equal(back.active, false);
  assert.equal(stateWords(back), "starting");
  // A stranger's message never becomes a call.
  assert.equal(parseAddonMessage({ v: 1, kind: "bisa:call", id: "x", method: "storage.get", params: {} }).kind, "call");
  assert.equal(parseAddonMessage({ kind: "bisa:call", id: "x", method: "storage.get" }), null, "no version, no reading");
});

test("a hunt can always be left: no panel over the app, Esc and the strip, the footer left to the app, a new page back at work", () => {
  const scene = src("../addons/AddonScene.tsx");
  // The panel at the top centre is gone, with its words: the strip the addon draws is where a hunt is left.
  assert.ok(!scene.includes("huntWords") && !scene.includes("<Button") && !/top-2[^"]*-translate-x-1\/2/.test(scene), "no host-drawn panel in hunt mode");
  assert.ok(!src("../addons/addonsModel.mjs").includes("addons-addons-model-leave"), "nor its button's word");
  // What stays: Esc while the app has the keyboard — unless a dialog took the key — and the keyboard handed back on every way out.
  assert.match(scene, /if \(e\.key !== "Escape" \|\| e\.defaultPrevented\) return;/, "Esc leaves, and a key a dialog took is the dialog's");
  assert.ok(scene.includes("blurFrame();") && scene.includes('dispatch({ type: "leave" })'), "leaving hands the keyboard back");
  // A hunt hides the pointer from the host, so the last point it saw is stale when the hunt ends: it is forgotten,
  // or the scene would take the pointer back at work for a strip the pointer left long ago.
  assert.match(scene, /const giveBack = useCallback\(\(\) => \{\s*lastPoint\.current = null;\s*blurFrame\(\);/, "leaving a hunt forgets where the pointer was");
  assert.equal(scene.split("giveBack()").length - 1, 3, "on the page's ask, on Esc, and when the page starts again");
  // The state is the model's: a new frame and a new hello are events, so a page that reloaded mid-hunt is at work.
  assert.ok(scene.includes("useReducer(sceneAfter, SCENE_REST)"), "one reducer, the model's");
  assert.match(scene, /dispatch\(\{ type: "frame" \}\);\s*\}, \[frameKey\]\)/, "another frame is another page");
  assert.ok(scene.includes('dispatch({ type: "hello" })'), "and so is a page that said hello again");
  // While hunting the scene gives up the footer: the Addons control there can always put the scene away.
  assert.match(scene, /clipPath: kept > 0 \? `inset\(0 0 \$\{kept\}px 0\)` : undefined/, "the footer's band is cut from the scene, paint and pointer alike");
  assert.ok(scene.includes("footerKept(mode, vp.top)"), "by the chrome's own height — the footer is the same token");
  // A see-through frame says the scheme its page has when the page declares none — light — whatever the app's own.
  assert.ok(scene.includes('style={{ colorScheme: "light" }}') && src("../addons/AddonWindow.tsx").includes('w.transparent ? { colorScheme: "light" } : undefined'), "never `normal`: under a dark app one engine reads it as dark and paints the pane opaque");
  // Who has the pointer is said to a screen reader from a line that is always there.
  assert.match(scene, /<p role="status" className="sr-only">\{huntStatus\(name, mode\)\}<\/p>/);

  // The addon's half: its own ways out stand first in the strip, before the tools, so no short glass scrolls them away.
  const page = readFileSync(new URL("library/addons/bug-hunt/index.html", root), "utf8");
  const tools = at(page, 'class="tools"');
  assert.ok(at(page, 'id="mode"') < tools && at(page, 'id="leave"') < tools, "Work and the way to put the scene away come before the tools");
  assert.ok(page.includes('aria-label="Put Bug hunt away"') && !page.includes("Leave the scene"), "one meaning for the word leave: the hunt; the cross puts the scene away");
  const main = readFileSync(new URL("library/addons/bug-hunt/main.js", root), "utf8");
  assert.ok(main.includes('if (e.key === "Escape" && state.mode === "hunt") { e.preventDefault(); askMode("work"); return; }'), "Esc inside the page asks for work");
  assert.ok(main.includes("bisa.window.close()"), "the cross puts the scene away");
  assert.ok(main.includes("H.stripTight(") && !main.includes("state.bounds.height < 520"), "the close layout follows what the strip measures");
  const manifest = JSON.parse(readFileSync(new URL("library/addons/bug-hunt/addon.json", root), "utf8"));
  assert.notEqual(manifest.window.closable, false, "and the scene may be closed by its own page");
});

test("the host keeps its answers true: a switch outlives the reads it raced, a reply never crosses a reload, a link is opened once", () => {
  const store = src("../addons/addonsStore.ts");
  // An optimistic switch or removal is newer than any read already on its way: that read must not land over it.
  const enable = store.slice(at(store, "export async function setAddonEnabled"), at(store, "export async function setAddonGrants"));
  assert.ok(at(enable, "reads.begin();") < at(enable, "withEnabled(state.addons, id, enabled)"), "the switch takes a ticket before it is drawn");
  assert.match(enable, /if \(wasHidden\) setAddonHidden\(id, true\);/, "a refused switch-on puts the window away again");
  const remove = store.slice(at(store, "export async function removeAddon"), at(store, "export async function importAddon"));
  assert.ok(at(remove, "reads.begin();") < at(remove, "state.addons.filter"), "and so does a removal");
  assert.ok(store.includes("staleWindowIds(") && !/forgetPref\(webStorage\(\), STORAGE_KEY_PREFIX \+ stale/.test(store), "a place is forgotten for an addon no longer listed; its store never is on a list alone");

  const bridge = src("../addons/addonBridge.ts");
  // A page that reloaded is another conversation: what the last one asked is not answered to it.
  assert.ok(bridge.includes("const talk = useRef(0);") && bridge.split("talk.current += 1;").length - 1 === 2, "a new frame and a new hello each start a conversation");
  assert.equal(bridge.split("if (talk.current !== asked) return;").length - 1, 3, "the fetch, the link and the clipboard answer only the conversation that asked");
  assert.match(bridge, /copyText\(effect\.text\)\s*\.then\(\(written\) =>/, "the clipboard's answer is the copy's own");

  const layer = src("../addons/AddonLayer.tsx");
  // React may call an updater twice; a link opened inside one opens twice.
  const settle = layer.slice(at(layer, "const settle = useCallback("), at(layer, "const shown = visibleAddons("));
  assert.ok(!/setAsk\(\(current\)/.test(settle) && settle.includes("openExternal(") && settle.includes("setAsk(null)"), "the link is opened outside the state updater");
  assert.ok(layer.includes("onRestart={") || layer.includes("restarted("), "an ask goes with the page that put it when that page starts again");

  const win = src("../addons/AddonWindow.tsx");
  assert.match(win, /if \(sameSize\(next, size\)\) return;/, "a resize to the size it has writes nothing and redraws nobody");
  assert.match(src("../views/_settings/AddonsPanel.tsx"), /await api\.setSetting[\s\S]{0,200}void refreshAddons\(\);/, "the machine's switch re-reads the list itself, bus or no bus");
});

test("a window comes forward, moves and resizes by keyboard, and passes the app's chords on", () => {
  const win = src("../addons/AddonWindow.tsx");
  // Forward by level, never by moving its node: a frame moved in the document loads again.
  assert.match(win, /zIndex: LAYER_FLOOR \+ level/, "the stack is a level over the layer's floor");
  assert.ok(win.includes("onPointerDownCapture={() => raiseAddon(addon.id)}"), "a press anywhere on the window raises it");
  assert.match(win, /if \(document\.activeElement === frame\.current\) raiseAddon\(addon\.id\);/, "and so does a press inside its page, which the app hears as its own blur");
  assert.ok(win.includes("onPress: () => raiseAddon(addon.id),") && src("../addons/addonBridge.ts").includes('if (parsed.kind === "press")'), "and a press its page says itself — the one signal when another window's page had the keyboard");
  assert.equal(win.split("onKeyDown={onMoveKey}").length - 1, 2, "the grip of a bare window and the grip in a bar both move by arrows");
  assert.ok(win.includes("onKeyDown={onResizeKey}"), "the corner resizes by arrows");
  assert.ok(win.includes('data-pane={w.transparent ? undefined : ""}'), "a window with a ground is a frosted pane on a glass family; a see-through one is not");
  assert.ok(win.includes('role="group"') && !win.includes('role="dialog"'), "a window is a group on the page, not a dialog that claims the page");
  const layer = src("../addons/AddonLayer.tsx");
  assert.match(layer, /level=\{stackOf\(order, windowIds, addon\.id\)\}/, "the layer hands each window its level");
  // The relay: the bridge passes a key to the door the shell provides, only from the frame that has the keyboard.
  const bridge = src("../addons/addonBridge.ts");
  assert.match(bridge, /if \(parsed\.kind === "key"\) \{\s*if \(session\.current\.ready && document\.activeElement === frame\) relayKey\(frame, parsed\.key\);/);
  const shortcuts = src("../shell/shortcuts.ts");
  assert.ok(shortcuts.includes("provideKeyRelay(") && shortcuts.includes("relays(command, key.key)"), "the shell judges the key by its own keymap and the bridge's short list");
  assert.ok(!src("../ui/keyRelay.ts").includes("../shell"), "the kit holds the door and knows nothing of the shell");
  // The palette offers the layer, each running window and the settings.
  const omnibox = src("../shell/Omnibox.tsx");
  assert.ok(omnibox.includes("addonCommands({ addons, hidden, layerShown, switchedOn })"), "the rows are the model's");
});

test("the terminal games: help pauses, a game at rest asks for no frames, a key map holds no modifier, and a focused button keeps its keys", () => {
  for (const game of ["tetrominal", "linefeed", "linerun"]) {
    const main = readFileSync(new URL(`library/addons/${game}/main.js`, root), "utf8");
    // Opening help mid-run used to leave the game running under the card, every game key swallowed: a crash within seconds.
    assert.match(main, /if \(name === "help"\) \{\s*if \(!help\.shown\(\) && phase === "play" && !game\.paused\) setGame\([A-Z]\.apply\(game, "pause"\)\);\s*help\.toggle\(\);/, `${game}: help pauses a run`);
    // Paused, over, or at the prompt: nothing moves, so no frame is asked for.
    assert.ok(main.includes('function syncClock() { if (phase === "play" && visible && !game.paused) clock.start(); else clock.stop(); }'), `${game}: the loop follows the game`);
    assert.match(main, /render\(\);\s*syncClock\(\);\s*\}/, `${game}: every change of the game settles the loop`);
    const keys = main.slice(at(main, "var KEYS = {"), main.indexOf("};", at(main, "var KEYS = {")));
    for (const modifier of ["Shift", "Control", "Alt", "Meta"]) assert.ok(!new RegExp(`\\b${modifier}\\b`).test(keys), `${game}: ${modifier} is half of another key — ? is Shift and /`);
    // The kit: Enter and Space on a focused control are that control's.
    assert.ok(main.includes('if ((e.key === "Enter" || e.key === " ") && onControl(e.target)) return;'), `${game}: a focused button keeps Enter and Space`);
  }
});

test("the media player: its shortcuts ask the rule, a player too small to show is tried again, and a source let go is let go", () => {
  const main = readFileSync(new URL("library/addons/media-player/main.js", root), "utf8");
  assert.ok(main.includes("if (P.ownsKey(controlOf(e.target), e.key)) return;"), "a focused control keeps its own keys");
  assert.ok(main.includes("pendingVideo = source.video;") && main.includes("if (embedded || pendingVideo) {"), "a window grown large enough gets the player it was owed");
  assert.match(main, /audio\.source\.disconnect\(\);/, "an element dropped leaves the audio graph");
  const page = readFileSync(new URL("library/addons/media-player/index.html", root), "utf8");
  for (const id of ["now", "total"]) assert.match(page, new RegExp(`<output id="${id}"[^>]*aria-live="off"`), `${id} ticks every second and is not announced`);
});

test("what ticks is never announced: a value that moves by itself is not a live region", () => {
  const page = (slug) => readFileSync(new URL(`library/addons/${slug}/index.html`, root), "utf8");
  // A live region is for what a person asked about or must be told — a turn, a result, a failure. A clock, a countdown,
  // a load read-out and a running count change on their own, and announced they talk over everything else.
  for (const slug of ["cpu", "gpu", "memory", "disk"]) assert.doesNotMatch(page(slug), /<main[^>]*aria-live/, `${slug}: the whole widget was a live region, said every five seconds`);
  assert.doesNotMatch(page("clock"), /id="reading"[^>]*aria-live/, "the clock's reading is there to be read, not said every second");
  assert.match(page("pomodoro"), /<output id="time"[^>]*aria-live="off"/, "the countdown is not said every second");
  assert.match(page("pomodoro"), /id="phase"[^>]*aria-live="polite"/, "the turn of a phase is");
  assert.doesNotMatch(page("bug-hunt"), /id="wait"[^>]*aria-live/, "the running count is redrawn on every blow");
  assert.match(page("stopwatch"), /<output id="time"[^>]*aria-live="off"/);
});

test("the small widgets say what is so: a lap needs a running watch, a square says its mark, a door that cannot open says why", () => {
  const main = (slug) => readFileSync(new URL(`library/addons/${slug}/main.js`, root), "utf8");
  assert.ok(main("stopwatch").includes("lap.disabled = !running;"), "a lap of a stopped watch was a lap of nothing");
  assert.match(main("stopwatch"), /\n  render\(\);\n  bisa\.ready\(\)/, "drawn before the app answers, so no control waits on it");
  assert.ok(main("tic-tac-toe").includes('b.setAttribute("aria-label", "Square " + (i + 1) + ", " + (cells[i] || "empty"));'), "a square's name carries its mark — the name alone hid X and O from a screen reader");
  assert.ok(main("needs-you").includes("Grant “open one of the app's screens” in Settings › Addons to go there."), "a door without its grant says what to grant");
  assert.match(main("unit-converter"), /\n  setKind\(pref\.kind, true\);\n\n  bisa\.ready\(\)/, "the menus are filled before the app answers");
  assert.ok(main("disk").includes("typeof d.workspace_bytes === \"number\""), "a size that did not come is left out, never said as NaN");
  assert.ok(main("gpu").includes('barfill.style.width = "0%";') && main("gpu").includes("history.length = 0;"), "a reader that went away takes its last bar and line with it");
  for (const slug of ["cpu", "gpu", "memory", "disk"]) assert.match(readFileSync(new URL(`library/addons/${slug}/style.css`, root), "utf8"), /\.value \{[^}]*text-overflow: ellipsis/, `${slug}: a long value is cut with its dots, never pushed out of the window`);
});

test("Bug hunt makes no sound nobody hears: the whine's oscillators end when it does, and a hidden scene's audio rests", () => {
  const main = readFileSync(new URL("library/addons/bug-hunt/main.js", root), "utf8");
  // The whine was started once and only turned down: two oscillators ran for as long as the page lived — at work, muted, hidden.
  assert.match(main, /ending\.osc\.stop\(/, "the whine's oscillators are stopped when it ends");
  assert.ok(main.includes("audio.whine = null;"), "and made again when the next mosquito comes");
  assert.match(main, /audio\.ctx\.suspend\(\)/, "a scene nobody sees rests its audio");
  assert.match(main, /audio\.ctx\.resume\(\)/, "and wakes it when seen again");
  assert.ok(!main.includes('state.working === 1 ? "running" : "running"'), "one word needs no choice");
});

test("the sticky note's toolbar is one row that fits the pad, and a paper's dot is not sized as a tool", () => {
  const css = readFileSync(new URL("library/addons/sticky-note/style.css", root), "utf8");
  // Six tools and six papers on one row at the pad's default 220: the last paper used to sit past the edge,
  // because a swatch — a button in the toolbar — took the tools' minimum width.
  assert.match(css, /\.tools \.swatch \{[^}]*min-width: 0; width: 12px; height: 12px;/);
  assert.match(css, /\.tools \{[^}]*overflow-x: auto;/, "narrower than that, the row scrolls sideways rather than take the note's lines");
  assert.doesNotMatch(css, /\.tools \{[^}]*flex-wrap: wrap/);
  const manifest = JSON.parse(readFileSync(new URL("library/addons/sticky-note/addon.json", root), "utf8"));
  // 6 tools × 18 + 5 gaps × 1, the spacer's 4, 6 papers × 12 + 5 gaps × 3 and its 4 of padding, the row's 12 of padding.
  assert.ok(6 * 18 + 5 + 4 + 6 * 12 + 15 + 4 + 12 <= manifest.window.width, "the row's own arithmetic fits the default width");
});

test("no built-in sets text a person must read under eleven pixels", () => {
  const dir = new URL("library/addons/", root);
  for (const slug of readdirSync(dir).filter((n) => !n.startsWith("."))) {
    const css = readFileSync(new URL(`${slug}/style.css`, dir), "utf8");
    const small = [...css.matchAll(/font-size:\s*(\d+(?:\.\d+)?)px/g)].map((m) => Number(m[1])).filter((px) => px < 11);
    assert.deepEqual(small, [], `${slug}: ${small.join(", ")}px`);
  }
});

test("the eighteen built-ins share one chrome, word for word, and say otherwise only after it", () => {
  const dir = new URL("library/addons/", root);
  const slugs = readdirSync(dir).filter((n) => !n.startsWith(".")).sort();
  const chromeOf = (css) => css.slice(at(css, "/* --- chrome --- */"), at(css, "/* --- end chrome --- */"));
  const first = chromeOf(readFileSync(new URL(`${slugs[0]}/style.css`, dir), "utf8"));
  for (const token of ["--ink: #1c1917", "--dim: #78716c", "--accent: #b45309", "--key:", "--line:", '[data-scheme="dark"]', "--ink: #f5f5f4", "font: 12px system-ui, sans-serif", "[hidden] { display: none !important; }", ":focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }"]) {
    assert.ok(first.includes(token), `the chrome holds ${token}`);
  }
  for (const slug of slugs) {
    const css = readFileSync(new URL(`${slug}/style.css`, dir), "utf8");
    assert.equal(chromeOf(css), first, `${slug}: its chrome is the others', word for word — change one, copy it to the rest`);
    const own = css.slice(at(css, "/* --- end chrome --- */"));
    // What the chrome says is said once: an addon restates a shared token only to say something else.
    assert.ok(!own.includes("[hidden] { display: none !important; }"), `${slug}: the fence is the chrome's`);
    assert.ok(!own.includes(":focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }"), `${slug}: the ring is the chrome's`);
    assert.ok(!/^:root(\[data-scheme="dark"\])? \{[^}]*--(accent: #(b45309|fbbf24)|ink: #(1c1917|f5f5f4))/m.test(own), `${slug}: the shared ink and accent are not said twice on the root`);
    assert.ok(!/html, body \{[^}]*margin: 0/.test(own), `${slug}: the page's reset is the chrome's`);
  }
});

test("every built-in paints its first frame in the machine's scheme, and a switch keeps one name", () => {
  const dir = new URL("library/addons/", root);
  for (const slug of readdirSync(dir).filter((n) => !n.startsWith("."))) {
    const main = readFileSync(new URL(`${slug}/main.js`, dir), "utf8");
    // Thirteen of them drew light until the app answered: a flash in a dark app.
    assert.ok(main.includes('matchMedia("(prefers-color-scheme: dark)")'), `${slug}: a scheme before the app says its own`);
  }
  // A switch says what it is, and `aria-pressed` says whether it is on: a name that flips with the state
  // reads as *Unmute, pressed*.
  const hunt = readFileSync(new URL("bug-hunt/main.js", dir), "utf8");
  assert.ok(!hunt.includes('"Unmute"') && !/setAttribute\("aria-label", marksWords\)/.test(hunt), "Bug hunt's mute and marks keep their names");
  const media = readFileSync(new URL("media-player/main.js", dir), "utf8");
  assert.ok(!/setAttribute\("aria-label", [^)]*"Unmute"/.test(media), "the media player's mute keeps its name");
});
