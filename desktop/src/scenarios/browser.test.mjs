/**
 * The embedded browser, as the desktop draws it (ide/18) — source-text
 * facts that hold the shape across the files: both hosts draw the one bar
 * and carry the browser chords' context; the page's words come through the
 * shell's door on every page and never IPC; the workspace's home carries
 * the Browser door; a URL in a message opens here first; a tab is born only
 * from an act and wears who opened it. Run with
 * `node --test --import ./src/i18n/preload.mjs src/scenarios/browser.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { paneToggle, screenHome as homeOfScreen } from "../shell/browserDoorsModel.mjs";
import { EMPTY_BROWSERS, PERSON, byAgent, byPage, openBrowser, parseBrowsers, seenSessions, serializeBrowsers, sessionOf } from "../shell/browsersModel.mjs";
import { statWords } from "../shell/browserStatModel.mjs";
import { urlCard } from "../shell/linkCardModel.mjs";

const src = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (rel) => readFileSync(join(src, rel), "utf8");

test("both hosts draw the one bar with the wand on it, over a body the browser chords are live in", () => {
  for (const host of ["views/_workbench/BrowserDoc.tsx", "shell/BrowserPane.tsx"]) {
    const text = read(host);
    assert.ok(text.includes("<BrowserBar "), `${host} draws the bar`);
    assert.ok(text.includes("data-browser-doc"), `${host} carries the browser context`);
    assert.ok(text.includes("useBrowserAnnotation(session)"), `${host} annotates through the one hook`);
    assert.ok(!text.includes("NoteBox"), `${host} draws no note box: the page draws its own`);
    assert.ok(text.includes("PaneAnnotationTray"), `${host} offers the screen's tray`);
  }
  // Annotating a page for an agent is the Project IDE's alone: beside any other screen the pane shows the page and draws no wand,
  // and a wand left on, or badges left drawn, are taken back from the page.
  const hook = read("views/_workbench/useBrowserAnnotation.ts");
  assert.ok(hook.includes('const offered = useRoute().name === "workbench";') && hook.includes("const annotatable = offered && read && canAnnotate(session, all);"), "annotating is offered on the Project IDE's screen alone");
  assert.ok(hook.includes('driveBrowserView(key, inspectMessage("off"))') && hook.includes("driveBrowserView(key, marksMessage([]))"), "off the IDE the page's wand and badges are taken back");
  assert.ok(read("shell/BrowserPane.tsx").includes("wand={a.offered ? {"), "the pane draws the wand only where annotating is offered");
  const bar = read("shell/BrowserBar.tsx");
  assert.ok(bar.includes("{wand && ("), "a bar with no wand draws none");
  assert.ok(bar.includes("data-browser-bar") && bar.includes("BROWSER_COMMAND"), "the bar answers the chords");
  for (const verb of ['label={t("shell-browser-bar-back")}', 'label={t("shell-browser-bar-forward")}', 'aria-label={t("shell-browser-bar-address")}', 'aria-label={t("shell-browser-bar-annotate-page-agent")}']) {
    assert.ok(bar.includes(verb), `the bar has ${verb}`);
  }
  assert.ok(!bar.includes("chrome.back &&") && !bar.includes("chrome.forward &&"), "back and forward are never absent, only held");
  const contexts = read("shell/keyContextsModel.mjs");
  assert.ok(contexts.includes('browser: "[data-browser-doc]"') && contexts.includes('out.push("browser")'), "the browser scope is a context of the handler");
  assert.ok(read("shell/keyContexts.ts").includes("scopesOf({"), "which asks the one model which scopes a target is within");
  assert.ok(read("shell/shortcuts.ts").includes("contextsOf(target, workbenchRoot() !== null)"), "and the handler reads its contexts from the one model");
});

test("a page's words come through the shell's door on every page, never IPC: no capability, no browser_message, no BLANK_PAGE", () => {
  assert.ok(!existsSync(join(src, "..", "src-tauri", "capabilities", "browser.json")), "the tabs' webviews stand in no capability");
  const shell = read("../src-tauri/src/browser.rs");
  assert.ok(!shell.includes("fn browser_message"), "the IPC command is gone");
  assert.ok(shell.includes("addScriptMessageHandler_name") && shell.includes('pub const DOOR: &str = "bisa"'), "the script message handler is the door");
  assert.ok(shell.includes("pub mod anchor") && shell.includes("Anchor::of(") && shell.includes("child.setFrame("), "the page is placed on the main webview's own frame and viewport, one setFrame on the main thread");
  assert.ok(shell.includes("anchor.placed_from(&frame_of(child))"), "and what was placed is read back as a fact, not as the input's inverse");
  assert.ok(shell.includes('"browser anchor"') && shell.includes("registry.note_anchor("), "the anchor is logged once per change");
  assert.ok(shell.includes("webview.hide().map_err(err)?;"), "hidden until placed");
  const main = read("../src-tauri/src/main.rs");
  assert.ok(!main.includes("browser_message"), "not registered either");
  const panel = read("shell/BrowserPanel.tsx");
  assert.ok(!panel.includes("BLANK_PAGE") && !panel.includes("localhost:0"), "a blank tab opens on the blank page");
  assert.ok(panel.includes("placedAsAsked") && panel.includes('"browser placed out of place"'), "a page drawn elsewhere is a logged fact");
  assert.ok(panel.includes("height: window.innerHeight") && read("browser/session.ts").includes("viewport: BrowserViewport"), "the page's own viewport rides every placement — the bar above the slot is never covered");
  const script = read("ui/artifact/pageInspector.mjs");
  assert.ok(!script.includes("__TAURI_INTERNALS__"), "the page's script asks nothing of IPC");
});

test("the Browser button is the Project IDE's alone, a URL in a message opens here first, and the palette takes an address", () => {
  // No screen outside the IDE wears a Browser button; beside any of them the
  // pane still opens with ⌘⇧L or the palette (the Details pane's door).
  for (const screen of ["views/Pulse.tsx", "views/Inbox.tsx", "views/Channels.tsx", "views/Messages.tsx", "views/GoalDetail.tsx", "views/_goal/GoalHeader.tsx", "views/WorkflowDesigner.tsx", "views/_studio/ConversationThread.tsx"]) {
    assert.ok(!read(screen).includes("BrowserDoor"), `${screen} wears no Browser button`);
  }
  assert.ok(read("views/Workbench.tsx").includes("<BrowserLauncher scope={scope} id={id}"), "the Project IDE keeps its Browser button");
  assert.ok(read("shell/AuxPane.tsx").includes("onDoor(OPEN_BROWSER, () => toggleBrowserPane({ kind, toggle }))"), "⌘⇧L and the palette open the pane beside any screen");
  // The card's verbs are the model's (`linkCardModel.urlCard`); the provider binds each to its act.
  const card = urlCard("https://example.com/docs", { embedded: true });
  assert.deepEqual(card.verbs.map((v) => [v.id, v.label, v.primary === true]), [["open_here", "Open in Bisa's browser", true], ["open_machine", "Open in the machine's browser", false], ["copy", "Copy the URL", false]], "a link's card opens the page here first; the machine's browser is the other door");
  const link = read("shell/linkHandler.tsx");
  assert.ok(/case "open_here":\s*return \(\) => void openUrlInBrowser\(verb\.url\);/.test(link), "the first verb opens the embedded browser");
  assert.ok(/case "open_machine":[\s\S]{0,80}openExternal\(verb\.url\)/.test(link), "the second the machine's");
  const palette = read("shell/Omnibox.tsx");
  assert.ok(palette.includes("urlRow(q)") && palette.includes("openUrlInBrowser(address.url)"), "a typed address is a row");
  // With no button on these screens, a tab ⌘⇧L opens beside a channel, a direct message, a goal or a workflow is still at home there: the door reads the route.
  for (const name of ["channel", "dm", "goal", "workflow"]) assert.deepEqual(homeOfScreen(null, null, { name, id: "X1" }), { scope: name, id: "X1" }, `beside a ${name}, the tab is at home in it`);
  assert.deepEqual(homeOfScreen({ kind: "conversation", id: "C1" }, null, { name: "goal", id: "X1" }), { scope: "conversation", id: "C1" }, "beside a conversation on screen, in the conversation");
});

test("the browser tools are one list everywhere: the engine's note, the skill, the reference and the MCP server name the same twenty-one", () => {
  const core = readFileSync(join(src, "..", "..", "crates", "bisa-core", "src", "browser.rs"), "utf8");
  const listed = [...core.matchAll(/^\s*"(browser_[a-z_]+)",$/gm)].map((m) => m[1]);
  assert.equal(listed.length, 21, "twenty-one tools in BROWSER_TOOLS");
  const skill = readFileSync(join(src, "..", "..", "library", "catalog", "skills", "embedded-browser.toml"), "utf8");
  const reference = readFileSync(join(src, "..", "..", "docs", "reference", "mcp-tools.md"), "utf8");
  const server = readFileSync(join(src, "..", "..", "crates", "bisa-mcp", "src", "server.rs"), "utf8");
  for (const tool of listed) {
    assert.ok(skill.includes(`\`${tool}\``), `the skill names ${tool}`);
    assert.ok(reference.includes(`\`${tool}\``), `the reference names ${tool}`);
    assert.ok(server.includes(`name = "${tool}"`), `the server registers ${tool}`);
  }
  assert.ok(skill.includes("browser_snapshot") && skill.includes("ref"), "the skill teaches acting by ref");
  assert.ok(skill.includes("dialogs"), "… and that dialogs are answered for the agent");
  const model = read("shell/browserBridgeModel.mjs");
  for (const action of ["snapshot", "type", "press", "select", "hover", "scroll", "wait", "forward", "reload", "console", "eval"]) {
    assert.ok(model.includes(`case "${action}"`), `the bridge plans ${action}`);
  }
  assert.ok(read("shell/browserBridge.ts").includes("navigatedResult("), "an act that moves the page answers the new page");
  assert.ok(read("ui/artifact/pageInspector.mjs").includes("installDialogs") && read("ui/artifact/pageInspector.mjs").includes("installConsole"), "the page's dialogs and console are the browser program's");
  assert.ok(read("views/_settings/BrowserAccessPanel.tsx").includes("SCRIPTS_KEY"), "Settings draws the scripts switch");
});

test("agents browse here first: every catalog agent carries the Embedded Browser skill, the strips show seen tabs only, and Settings draws the out-of-sight switch", () => {
  const agents = join(src, "..", "..", "library", "catalog", "agents");
  for (const file of readdirSync(agents).filter((f) => f.endsWith(".toml"))) {
    assert.ok(readFileSync(join(agents, file), "utf8").includes('"embedded-browser"'), `${file} carries the skill`);
  }
  for (const core of ["general-agent.toml", "workflow-agent.toml"]) {
    assert.ok(readFileSync(join(src, "..", "..", "library", "core", core), "utf8").includes("out of sight"), `${core} knows a tab can be kept out of sight`);
  }
  for (const strip of ["views/_workbench/CenterDocuments.tsx", "views/Workbench.tsx"]) {
    const text = read(strip);
    assert.ok(text.includes("seenRootedAt(") && !text.includes("browsersRootedAt("), `${strip} rides seen tabs only`);
  }
  assert.ok(read("shell/BrowserPane.tsx").includes("seenSessions(sessions)"), "the pane looks at seen tabs");
  assert.ok(read("views/_settings/BrowserAccessPanel.tsx").includes("HEADLESS_KEY"), "the switch is drawn by hand");
  assert.ok(read("shell/BrowserPanel.tsx").includes("BROWSER_PRESENCE_MS"), "an open desktop keeps the engine's presence fresh");
  assert.ok(!read("shell/browserBridge.ts").includes("browserPrefs().reveal"), "the reveal switch is gone: a headless tab is never revealed by an agent's act");
  const shell = read("../src-tauri/src/browser.rs");
  assert.ok(!shell.includes("offstage") && !shell.includes("headless"), "offstage is a placement like any other: no shell change for a headless tab");
});

test("the footer's Browser count tells the truth: it follows the browser's switch and the hosts' slots, names every tab's home, and every door gives a tab the home it was opened from", () => {
  const footer = read("shell/StatusBar.tsx");
  assert.ok(footer.includes("<BrowserStat />"), "the footer draws the one read-out");
  for (const gone of ["ICON.hidden", "hidden = 0", "seenSessions", "groupTabs", "WorkingDot"]) {
    assert.ok(!footer.includes(gone), `the footer draws no second glyph and no list of its own: ${gone}`);
  }
  const stat = read("shell/BrowserStat.tsx");
  for (const fact of ["useBrowserPrefs()", 'useLayerSlot("center")', 'useLayerSlot("aux")', "shownTab(", "canOpenBrowser()", "statWords(", "<ICON.page "]) {
    assert.ok(stat.includes(fact), `the read-out ${fact}`);
  }
  assert.ok(!stat.includes("ICON.hidden"), "one glyph: the eye names a dimension in the overlay, never the bar");
  assert.ok(stat.indexOf("useLayerSlot(\"aux\")") < stat.indexOf("if (!canOpenBrowser()) return null;"), "every hook before the gate");
  const model = read("shell/browserStatModel.mjs");
  for (const fact of ["groupTabs(", "whereWords(", "footerBrowserWords(", "seenSessions(", "visibilityWords(", "busyWords("]) {
    assert.ok(model.includes(fact), `the read-out's model composes ${fact}`);
  }
  const overlay = read("shell/BrowserOverlay.tsx");
  for (const fact of ["screenHome()", "if (key) openBrowserPane(key);", "closeBrowserTab(", 'settingsSearch("browser")', 'chooseDimension("browser"', 'useChosenDimension("browser")', "<SegmentedControl<Dimension>", "<Meter "]) {
    assert.ok(overlay.includes(fact), `the overlay ${fact}`);
  }
  assert.ok(!overlay.includes("homeWords"), "no raw ids: every home by name");
  assert.ok(!read("shell/browsersModel.mjs").includes("export function homeWords"), "the id-printing words are gone");
  assert.ok(read("App.tsx").includes("resetKey={routeKey} onError={(error, info) => log.error(\"browser\""), "the browser layer's boundary resets with the route");
  const panel = read("shell/BrowserPanel.tsx");
  assert.ok(panel.indexOf("forgetBrowserWork(key)") < panel.indexOf("if (!available) return;\n    for (const s of sessions) {"), "a tab that left is forgotten whatever the shell is");
  assert.ok(read("shell/browserShots.ts").includes("AbortSignal.timeout(UPLOAD_MS)"), "an upload ends, one way or the other");
  assert.ok(read("shell/useBrowsers.ts").includes("prefs.read && prefs.enabled"), "no browser before the settings are read");
});

test("a tab is born only from an act — a person's, an agent's request, a page's window — and wears who opened it; a pane that merely shows, a remembered address and a launch open none", () => {
  // The afternoon, stepped through the models. Every tab closed: the read-out says none.
  let state = EMPTY_BROWSERS;
  assert.equal(statWords({ sessions: state.sessions, busy: [], shown: null }).value, "0");
  // An addon navigates to a screen whose Browser pane the place memory brings back: the pane mounts, and no door was pressed.
  // The pane has no way to open a tab of its own: a target with no opener opens nothing.
  assert.equal(openBrowser(state, { home: { scope: "goal", id: "g1" } }), state, "a pane is nobody");
  assert.equal(statWords({ sessions: state.sessions, busy: [], shown: null }).value, "0", "the count stays where it was");
  // The person presses ⌘⇧L with no tab in sight: the door opens one — theirs — and shows it.
  assert.equal(paneToggle({ showing: false, seen: seenSessions(state.sessions).length }), "open");
  state = openBrowser(state, { by: PERSON, home: { scope: "goal", id: "g1" } });
  assert.equal(paneToggle({ showing: false, seen: seenSessions(state.sessions).length }), "show", "with one in sight the door only shows the pane");
  assert.equal(paneToggle({ showing: true, seen: 1 }), "hide");
  // An agent opens a page out of sight in an auto goal: counted, and said to be the agent's.
  state = openBrowser(state, { by: byAgent("reviewer"), home: { scope: "goal", id: "g1" }, url: "http://localhost:5173/", headless: true });
  const words = statWords({ sessions: state.sessions, busy: [], shown: null });
  assert.equal(words.value, "2");
  assert.equal(words.title, "2 browser tabs — 1 opened by agents, 1 out of sight");
  // Its page asks for a window: the child stands as the asker stands — out of sight — and is the page's.
  const asker = sessionOf(state, "b2");
  state = openBrowser(state, { by: byPage(asker.key), home: asker.home, url: "http://localhost:5173/popup", headless: asker.headless });
  assert.ok(sessionOf(state, "b3").headless, "a hidden page puts nothing on screen");
  assert.equal(state.active, "b1", "and takes no host from the tab the person looks at");
  // Quit and reopen: what a person could look at comes back; the agent's working pages do not.
  const back = parseBrowsers(JSON.parse(JSON.stringify(serializeBrowsers(state))));
  assert.deepEqual(back.sessions.map((s) => [s.key, s.by.kind]), [["b1", "person"]]);
  assert.equal(statWords({ sessions: back.sessions, busy: [], shown: null }).title, "1 browser tab");
  assert.equal(openBrowser(back, { by: PERSON }).sessions.at(-1).key, "b4", "no key is minted twice across the restart");

  // The wiring, by source. The pane opens nothing by itself and reveals nothing from an address.
  const pane = read("shell/BrowserPane.tsx");
  const effects = [...pane.matchAll(/useEffect\(\(\) => \{[\s\S]*?\n {2}\}, \[[^\]]*\]\);/g)].map((m) => m[0]);
  assert.ok(effects.length >= 1 && effects.every((e) => !e.includes("openBrowserIn(")), "no effect of the pane opens a tab");
  assert.ok(!pane.includes("revealBrowserTab(tab)") && pane.includes("focusBrowserTab(tab)"), "the address focuses a tab in sight; showing one kept out of sight is a click");
  assert.ok(pane.includes("<EmptyState") && pane.includes('t("shell-browser-pane-no-tab-here-yet")') && pane.includes("onClick={newTab}"), "with none in sight the pane says so and offers New tab");
  // Every birth names its opener: a person's door, an agent's request, a page's window.
  const births = [];
  for (const file of readdirSync(src, { recursive: true }).filter((f) => /\.(ts|tsx)$/.test(f) && !f.includes("node_modules"))) {
    const text = readFileSync(join(src, file), "utf8");
    for (const m of text.matchAll(/openBrowserIn\(([^;]*?)\);?\n/g)) if (!m[0].startsWith("openBrowserIn(target")) births.push([file, m[1]]);
  }
  assert.ok(births.length >= 8, `the doors are found (${births.length})`);
  for (const [file, args] of births) assert.ok(/\bby: (PERSON|byAgent\(|byPage\()/.test(args), `${file}: openBrowserIn(${args.slice(0, 60)}…) says who opens the tab`);
  const bridge = read("shell/browserBridge.ts");
  assert.ok(bridge.includes("if (!asker) return;") && bridge.includes("headless: asker.headless, by: byPage(key)"), "a window asked for a tab that is gone opens nothing; the child stands as the asker stands");
  assert.ok(bridge.includes("by: byAgent(pending.scope.agent)"), "an agent's tab names the agent the engine said");
  // The person's doors to the pane go through the one toggle.
  assert.ok(read("shell/AuxPane.tsx").includes("onDoor(OPEN_BROWSER, () => toggleBrowserPane({ kind, toggle }))"), "⌘⇧L and the palette");
  assert.ok(read("shell/BrowserDoor.tsx").includes("onClick={() => toggleBrowserPane(aux, home)}"), "a screen's Browser button");
  const doors = read("shell/browserDoors.ts");
  assert.ok(doors.includes('if (plan === "open") openBrowserPane(openBrowserIn({ home, by: PERSON }));'), "the door opens one when none is in sight — an act");
  // The one door logs the birth, and only what a person can look at is remembered.
  assert.ok(read("shell/useBrowsers.ts").includes('log.debug("browser", "a tab opened"'), "the birth is a line in the log");
  assert.ok(read("shell/browsersModel.mjs").includes("sessions: seenSessions(state.sessions).map("), "remembered: the tabs in sight");
});

test("the camera is two buttons and no screenshot is asked of a hidden page: the shot waits for the tab to show, the shell refuses a hidden view at once, and every kit surface counts itself", () => {
  // The bar: two buttons, not a menu — a menu is a surface the page yields to, and would blank the page about to be photographed.
  const bar = read("shell/BrowserBar.tsx");
  assert.ok(bar.includes('label={t("shell-browser-bar-copy-screenshot'), "copy is a button");
  assert.ok(bar.includes('label={t("shell-browser-bar-save-screenshot")}'), "save as is a button");
  assert.ok(!bar.includes("<Menu") && !bar.includes('label="Screenshot"'), "no menu on the camera");
  assert.ok(bar.includes('why: t("shell-browser-bar-screenshot-being-taken")'), "held while a shot is taken, with the reason");
  // One door for a shot: it waits until the tab shows, and refuses in words the bridge tells apart.
  const shots = read("shell/browserShots.ts");
  assert.ok(shots.indexOf("await untilShown(key)") < shots.indexOf("screenshotBrowserView("), "the wait comes before the ask");
  assert.ok(shots.includes("export class NotShownError extends Error"), "a refusal the caller can tell apart");
  assert.ok(shots.includes("copyImage(") && !shots.includes("navigator.clipboard"), "the picture goes through the kit's clipboard door, never the webview's own");
  const bridge = read("shell/browserBridge.ts");
  assert.ok(bridge.includes("e instanceof NotShownError") && bridge.includes("refused(NOT_SHOWN,"), "the bridge answers the agent's sentence for a tab that never showed");
  assert.ok(!bridge.includes("function untilShown") && !bridge.includes("function shownNow"), "the wait lives in one place");
  const shown = read("shell/browserShown.ts");
  assert.ok(shown.includes("subscribeLayerSlots(check)") && shown.includes("subscribeSurfaces(check)"), "the wait hears the slots and the surfaces alike — a menu closing is a change");
  assert.ok(shown.includes("if (shownNow(key)) return Promise.resolve(true);"), "a tab already showing is not made to wait");
  // The shell: a hidden view is refused before WebKit is asked anything.
  const shell = read("../src-tauri/src/browser.rs");
  assert.ok(shell.indexOf("web.isHiddenOrHasHiddenAncestor()") < shell.indexOf("WKSnapshotConfiguration::new(mtm)"), "the hidden check comes before the snapshot is configured");
  assert.ok(shell.includes("pub const SNAPSHOT_HIDDEN") && shell.includes("pub const SNAPSHOT_LATE"), "two refusals, named");
  // The picture's clipboard door is the shell's, like the text's.
  const main = read("../src-tauri/src/main.rs");
  assert.ok(main.includes("fn copy_image(") && main.includes("            copy_image,"), "copy_image is a command and is registered");
  assert.ok(main.includes("png::is_png(&png)"), "only a PNG is put on the clipboard");
  assert.ok(read("../src-tauri/Cargo.toml").includes('"image-png"'), "the shell decodes the PNG it is handed");
  assert.ok(read("api.ts").includes('invoke("copy_image"'), "the desktop's door to it");
  assert.ok(read("ui/clipboard.ts").includes("export async function copyImage("), "the kit's one door, for a picture too");
  // A placement the shell refuses is said, never swallowed.
  const panel = read("shell/BrowserPanel.tsx");
  assert.ok(panel.includes('"browser placement refused"'), "a refused placement is logged");
  const place = panel.slice(panel.indexOf("const place = useCallback("), panel.indexOf("const placeNow = useCallback("));
  assert.ok(place.includes("setBrowserViewBounds(") && !place.includes(".catch(() => undefined)"), "no silent catch on a placement");
  // Every kit surface counts itself, so the page yields to a right-click menu as it does to a dialog.
  for (const name of ["ui/ContextMenu.tsx", "ui/LinkCard.tsx", "ui/artifact/ArtifactStage.tsx"]) {
    assert.ok(/use(Open)?Surface\(/.test(read(name)), `${name} says when it opens`);
  }
});

test("a browser tab at home in the IDE follows its centre: the strip in Project Mode, the Details pane in Agent and Board Mode, carried across the switch", () => {
  const rule = read("shell/browserBridgeModel.mjs");
  assert.ok(rule.includes("export function revealPlan(session, current, centre)"), "the reveal rule reads what the centre shows");
  assert.ok(rule.includes('return centre === "documents" ? "ide" : "pane";'), "at home, the strip only while the centre shows documents");
  const doors = read("shell/browserDoors.ts");
  assert.ok(doors.includes("export function showBrowserTab(key: string): void") && doors.includes("revealPlan(session, root, workbenchCentreFor(root))"), "one door shows a tab where it belongs, reading the published centre");
  assert.ok(doors.includes("export function openBrowserAt(") && doors.includes("if (key) showBrowserTab(key);"), "a tab opened at a home is shown through the same door");
  assert.ok(!doors.includes("if (!workbenchRoot()) openBrowserPane(key);"), "no special case for the IDE: the rule is the centre's");
  assert.ok(doors.includes("onDoor(NEW_BROWSER_HERE,") && read("shell/shortcuts.ts").includes("fire(NEW_BROWSER_HERE, { home: root });"), "the keymap's new tab here goes through the door too");
  const bridge = read("shell/browserBridge.ts");
  assert.ok(bridge.includes("showBrowserTab(session.key);") && !bridge.includes("revealPlan("), "an agent's tab is shown through the one door");
  assert.ok(bridge.includes("return ideRoot();") && !bridge.includes('root.scope !== "machine"'), "the IDE's root is read once, in shortcuts");
  assert.ok(read("shell/BrowserLauncher.tsx").includes("openBrowserAt(home, url)"), "the Browser button too");
  const store = read("views/_workbench/workbenchCentreStore.ts");
  assert.ok(store.includes("export function publishWorkbenchCentre(") && store.includes("export function workbenchCentreFor("), "the centre is published, never looked up");
  assert.ok(!store.includes("useSyncExternalStore"), "and nothing subscribes: the doors read it at the moment they show a tab");
  const workbench = read("views/Workbench.tsx");
  assert.ok(workbench.includes("const centre = centreOf(mode, {") && workbench.includes("publishWorkbenchCentre({ scope, id }, centre)"), "the workbench derives the centre once and publishes it");
  assert.ok(workbench.includes("followCentre(centre, activeBrowserHere, paneBrowserHere)"), "a switch carries the tab");
  assert.ok(workbench.includes("if (was.key !== key || was.centre === centre) return;"), "only a switch of the same root moves anything");
  assert.ok(workbench.includes('if (carried?.show === "pane") openBrowserPane(carried.key);') && workbench.includes("aux.close();"), "to the pane leaving documents, back to the strip returning, the pane closing");
  assert.ok(!read("views/_workbench/BrowserDoc.tsx").includes("centreOf") && !read("shell/BrowserPane.tsx").includes("workbenchCentreFor"), "the hosts draw slots and know nothing of the mode: placement stays the slots' rule");
});

test("the bridge tells a gone tab from a silent page, retries only through the model's rule, and logs every refusal with the act", () => {
  const bridge = readFileSync(join(src, "shell/browserBridge.ts"), "utf8");
  assert.ok(bridge.includes('kind: "gone"; error: unknown') && bridge.includes('kind: "silent"; waited_ms: number'), "asking a page comes to an answer, silence or a gone tab");
  assert.ok(bridge.includes("resolve({ kind: \"gone\", error })"), "a refused drive is the tab gone, not silence");
  assert.ok(bridge.includes("refused(TAB_GONE,") && bridge.includes("refused(PAGE_SILENT,"), "two sentences");
  assert.equal((bridge.match(/askAgain\(/g) ?? []).length, 1, "the retry decision is the model's, taken once");
  const retry = bridge.indexOf("askAgain(plan, outcome, 1)");
  const navigating = bridge.indexOf("const watch = untilStarted(key, ANSWER_MS)");
  assert.ok(retry !== -1 && navigating !== -1 && retry < navigating, "the retry sits in the branch that cannot move the page, before the navigating one");
  assert.ok(bridge.includes('log.warn("browser", "the page could not be asked", { key, action,'), "a gone tab is a warning with the key and the act");
  assert.ok(bridge.includes('log.warn("browser", "a browser request was refused", { id: pending.id, action: pending.request.action'), "every refusal is one line with the id and the act");
  assert.ok(!bridge.includes("error: result.error, text") && !bridge.includes("note:"), "no page words in a log line");
  const panel = readFileSync(join(src, "shell/BrowserPanel.tsx"), "utf8");
  assert.ok(panel.includes("presenceLapsed(failures)") && panel.includes("agents are about to be told nobody is home"), "the second missed heartbeat is a warning");
});

test("the footer's bars are the kit's one stacked bar: Bisa's parts first, every band toned apart from the track, a legend for the resources", () => {
  const read = (rel) => readFileSync(new URL(`../${rel}`, import.meta.url), "utf8");
  const kit = read("ui/StackedBar.tsx");
  assert.ok(kit.includes('role="img"') && kit.includes("gap-px") && kit.includes("minWidth: b.percent > 0 ? 2 : 0"), "bands with hairlines, a floor for a sliver");
  const fills = kit.slice(kit.indexOf("const FILL"), kit.indexOf("const SWATCH"));
  assert.ok(!fills.includes("bg-surface-2"), "no band wears the track's colour — that is what free looks like");
  assert.ok(kit.includes('neutral: "bg-border"'), "the machine's band is the border tone, visibly not the track");
  for (const file of ["shell/ResourceOverlay.tsx", "shell/BrowserOverlay.tsx"]) {
    const src = read(file);
    assert.ok(src.includes("<StackedBar"), `${file} draws the kit's bar`);
    assert.ok(!src.includes("SEGMENT_FILL") && !src.includes("h-1.5 w-full overflow-hidden rounded-full"), `${file} paints no bar of its own`);
  }
  assert.ok(read("shell/ResourceOverlay.tsx").includes("legend={bar.legend}"), "the resource bar says Bisa, the machine and the free rest beneath");
  const model = read("shell/resourceModel.mjs");
  assert.ok(model.includes("export function usageBar(") && !model.includes("export function stackedBar("), "one bar model, read left to right");
});

test("the page's floating overlays are never under a tab: each says its box, the layer cuts a hole for it, and the page stays live around it", () => {
  const read = (rel) => readFileSync(new URL(`../${rel}`, import.meta.url), "utf8");
  // Every overlay that floats over the content says where it is painted.
  for (const [file, id] of [
    ["notes/NoteOverlay.tsx", '"notes-panel"'],
    ["draw/DrawOverlay.tsx", '"draw-panel"'],
    ["notes/NoteDock.tsx", '"notes-dock"'],
    ["draw/DrawDock.tsx", '"draw-dock"'],
    ["pet/PetCompanion.tsx", '"pet"'],
    ["addons/AddonWindow.tsx", "`addon:${addon.id}`"],
  ]) assert.ok(read(file).includes(`useBrowserClear(${id}`), `${file} says its box to the browser layer`);
  // A panel floating says it; maximized it is a surface, as before (scenarios/maximize.test.mjs).
  for (const file of ["notes/NoteOverlay.tsx", "draw/DrawOverlay.tsx"]) assert.ok(read(file).includes(", panel, open && !fixed);"), `${file}: only while floating`);
  // The one file that talks to the webviews sends them, once a frame, a failure logged.
  const panel = read("shell/BrowserPanel.tsx");
  assert.ok(panel.includes("setBrowserClears(clears, viewport)") && panel.includes("the browser layer could not cut around the overlays"), "sent, and a refusal is logged");
  assert.ok(panel.includes("requestAnimationFrame") && panel.includes("sameClears(last.clears, clears)"), "coalesced per frame, nothing sent twice");
  // The shell's layer: a mask for what shows, a hit-test for what is clicked — Apple's own primitives, no private API.
  const shell = readFileSync(new URL("../../src-tauri/src/browser.rs", import.meta.url), "utf8");
  for (const piece of ["#[name = \"BisaBrowserLayer\"]", "hitTest:", "updateLayer", "setDisableActions(true)", "addSubview_positioned_relativeTo", "kCAFillRuleEvenOdd"]) {
    assert.ok(shell.includes(piece), `browser.rs: ${piece}`);
  }
  assert.ok(!shell.includes("new_copy_by_"), "no CGPath boolean operation: they are macOS 13 and the app runs from 11");
});
