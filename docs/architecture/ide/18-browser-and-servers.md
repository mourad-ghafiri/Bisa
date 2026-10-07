# 18 — The embedded browser, and agents that browse where you can see

The platform has a browser of its own: a native webview under a browser's bar — back and forward
by the page's own history, reload, *Stop* while a page loads, the address field, the wand, the
camera, whole on every tab — on anything on http(s), its title the webview's own for every page, a
link that wants a window opening a tab beside. In the Project IDE it is a tab in the centre beside
the terminals, opened by **Browser** next to **Terminal** — the one button, whose menu also serves
the checkout: the node hosts a static server for the branch's checkout or any folder of it, while
the **Terminal** menu opens the project's **run command**. Everywhere else it is the **Browser pane**: the Details pane's `browser`
occupant, beside a goal, a workflow, a channel, a direct message, an artifact — ⌘⇧L, the
footer's *Browser* read-out, a **Browser** button on the screen — the Pulse's too. One store of tabs
serves both. Any page a tab shows can be **annotated** for an agent exactly as a rendered file can,
in the IDE and in the pane; every agent drives that same browser through tools, on any page, so the
person sees the tab where they are; a
**screenshot** of a tab reaches an agent as a file and the person as a copy; and the workspace
decides which agents may ask, and how far.

The rules that decide the shape are the IDE's own ([01](01-trust-boundary.md)): the webview never
names a program or a path — the node resolves the run command, hosts the servers and names a
screenshot's copy; nothing reaches an agent that is not a chip or a tool's answer
([09](09-agents-in-the-ide.md)); a page the node serves is on loopback only; a native layer never
paints over a dialog or a floating overlay; and a tool list is a menu, not a permission — who may ask is checked in the
engine.

---

## Serving a folder

`POST /workstreams/{wid}/servers {folder?}` (`crates/bisa-node/src/ide/serve.rs`) binds
`127.0.0.1:0` and serves the folder — the checkout itself when none is named — with
`tower_http::services::ServeDir`: `index.html` for a directory, the content type from the file,
a 404 for the rest, and **only the folder's own files**: one middleware
(`only_the_folders_own_files`) reads the path as the name it decodes to — `%2Egit` is `.git` — and
refuses any segment starting with `.`, an escape that is not one, and a separator an escape
smuggles in, so `.git` and `.env` are not there however the URL spells them; then it follows the
file's links (`stays_within`) and refuses one that ends outside the folder or on a hidden name,
whenever the link was made. The folder is resolved inside the checkout
(`bisa_store::resolve_within`) and **never a hidden one** (`Servers::folder_root`): served whole, a
hidden folder would hand out by their plain names the very files a URL is refused — `.git`'s
`config` as `/config` — so a folder is judged where it *is* once its links are followed, and one
whose place under the checkout has a part starting with a dot is a 400, `.git` and a link to it
alike. A second server for the same folder of the checkout is a 409, and
starts are taken one at a time, so two asks at the same moment — a double click, an agent's retry —
never bind two ports. At most thirty-two folders are served at once (`MAX_SERVERS`, checkouts' and
artifacts' together): each is a port and a task of the node's, and a start past the bound is
refused with the sentence that says to stop one. `GET …/servers` lists a checkout's, `DELETE …/servers/{id}` stops one of that
checkout's and is a 404 for any other server, and every start or stop puts `server_changed` on the
bus — naming the checkout, or no checkout for an artifact's page — so the Browser menu, the footer
and the browser bar read the list again. The servers live while the node does and are not records:
a restart forgets them, as it forgets a terminal; the desktop reads the list again when the node
comes back, and a remembered tab that showed a served folder then points at a port nobody listens
on until the folder is served again from the Browser menu. `GET …/servers/{id}/resolve?path=` answers the file of the checkout a URL path of that
server lands on (`site/docs/index.html` for `/docs/`), relative to the checkout — what makes an
annotation on a served page a **file** chip (§Annotating a served page).

A session serves too: `browser_serve` (`Op::BrowserServe`, `crates/bisa-engine/src/intake.rs`) puts
a folder of the session's checkout — the scope's checkout for a conversation about a workstream or a
project, the work item's workstream otherwise — on the same servers through the port the node lends
the engine at start (`bisa_engine::browser::FolderServer`, implemented by `Servers`;
`Engine::set_folder_server`), under the same word on who may use the browser, and answers the URL
to `browser_open` (`ServedPage { id, url, page, folder }`) with `server_changed` on the bus. A
session in no checkout is told so (`NO_CHECKOUT`), as is a session on an engine no node runs
(`NO_SERVER`) — the one-shot CLI hosts no server. This is how a goal that runs unattended reviews the
page it wrote: nobody is there to serve the checkout from the Browser menu.

## The run command

A fourth workstream script ([07 §Workstream scripts](07-workstreams.md#workstream-scripts)):
`workstreams.script.run`, *Run command*, set under About › Settings › Workstream scripts beside the
three lifecycle scripts and **approved on this machine like them** (the digest on
`workstreams.script.trusted`). The engine never runs it: `scripts::run_phase` refuses `Phase::Run`
by name, and `Phase::LIFECYCLE` is the three the engine does run. It is the IDE's **Terminal**
caret that opens it, in a terminal the person watches: on a checkout the caret's second item is
*Run `<command>`* when the project sets one (`runCommandModel.runCommandItem`), *Run `<command>` — not
approved here* until this machine approves it — and nothing when the project sets none; the card is where a command is set, and no menu offers to go there. The item and `⌘⇧R`
take one act (`shell/useRunCommand.ts`: `useRunCommand` reads
`GET /workstreams/{wid}/run-command` — `{command, trusted, digest, cwd}`, 404 when the project sets
none, re-read on `settings_changed`; `runTheCommand` opens the terminal when this machine has
approved the command, and otherwise says so (`approvalWords`) and opens the card that approves it,
`openPanelView("about", "settings")`). The shell's `terminal_open` with `run: true` asks that
route itself (`desktop/src-tauri/src/terminal.rs::run_launch`) — refusing an unapproved one with
where to approve it — and runs `sh -c <text>` through the login shell in the checkout, the way a
harness runs. The tab is labelled *run · <command>* (`terminalsModel.mjs`, `run: true` on the
session, remembered across a restart); the port it opens is found by the ports poll like any other
and attributed to the shell, so the Browser menu and the footer list it, one click from a tab.

## The Browser button

`shell/BrowserLauncher.tsx` over `views/_workbench/serversModel.mjs` — one split button beside
Terminal, on a workstream, a goal or a work item. The main click is `browserMenu(facts).main`: a
tab on the checkout's newest server, else its newest port, else blank with the field ready. The
caret is `browserMenu(facts).items`, a rule at each group's start, its words plain: *New tab*; on
a checkout *From folder…* — the one item that serves (§The folder picker): the root of the checkout
or one folder of it, served by the node on a port of this machine and opened;
every server up with *Open :4173* (where it serves is the hint — *the checkout · :4173*,
*docs/ · :4173*) and *Stop* — *Stop :4173* when several are up (`openLabel`, `stopLabel`); the
ports the rail attributes here with *Open :5173 · vite*; the tabs already open here; and *Annotate
the page for an agent…* when the active tab here shows a page an agent can edit (§Annotating a
served page). Nothing in the menu runs a command or opens a settings card: the run command is the
Terminal caret's (§The run command). A start in flight, or a checkout not on disk yet, holds the
serves and the stops — never an open. Every open lands in a browser tab rooted here. `⌘⇧R`
(`run_project`) opens the door `runCommandModel.runDoor` names — the newest server, else the run
command when the project sets one **and this machine approved it**, else *From folder…*, the picker
open on the folder served there last, so the chord and Enter serve it again; an
unapproved command is no door, the chord never opens a settings card — and the item that is the
door shows the chord (`command: "run_project"`), in whichever menu it stands: the Browser's *Open*
or *From folder…*, or the Terminal's *Run* (`browserMenu` is given the door's id as a fact and knows
nothing of the run command); off a checkout there is no door.
The servers and the ports are one read for every surface that names them (`shell/useServing.ts`:
`GET /servers`, this checkout's slice and every server, reloaded on `server_changed`). The Files
tree's folder menu offers *Serve this folder* for the same act, through the same door. The button
renders only in the desktop shell.

### The folder picker

What is served is always a folder, and the checkout's root is one: the node reads an absent `folder`
as the checkout itself, so the menu has no second item for it and nothing in it is named after a
branch. `shell/ServeFolderDialog.tsx` over `views/_workbench/serveFolderModel.mjs` — the rules in
the model, effects and paint in the component, the kit's `Dialog` and `TreeList` around them:

| | |
|---|---|
| **Root** | pinned first, under the project's name: *the whole checkout* — `ROOT_FOLDER`, the empty path |
| the tree | folders only, listed by the explorer's own state machine (`ui/fileTreeModel.mjs`: one `GET /tree` per folder opened, depth 1), so an ignored folder is there, dimmed — a `dist/` is what one serves. A folder the listing shows to hold no folder draws no chevron; a listing in flight or failed is one row in place of the children (`folderRows`) |
| the filter | the field on top keeps the caret. A few letters find a folder by its path — `quickOpenScore.rankPaths` over the folders the shared path index names (`foldersOf`) and the ones already listed (`listedFolders`), `FILTER_LIMIT` rows; *root* or the project's name finds Root; a path typed in full that matches nothing is offered last, *as typed*, so a folder no build has made yet can be named (`filterRows`). An absolute or a climbing path is never offered and the reason is said (`folderProblem`, the node's own rule said first) |
| what helps choose | *index.html* on a folder that has one (`indexFolders` — what the server answers a directory with), *serving :4173* on one already up (`serverFor`) |
| the keys | Up and Down walk the rows from the field (`stepCursor`), Left and Right stay the text's; in the tree they close and open a folder, with `TreeList`'s type-ahead, Home and End. Enter, a double-click or the button serves |
| the button | *Serve and open*, or *Open :4173* for a folder already served (`submitWords`); the line under the list says the choice in words (`choiceWords`) |
| remembered | the folder served last in each checkout, newest last and capped (`bisa.ide.serve.folder`, `rememberFolder`): the picker opens on it, its ancestors opened (`ancestorsOf`) |

**One door serves** — `shell/serveDoors.ts::serveAndOpen(wid, folder)`, the picker's and the Files
tree's alike: it asks the node what is up first and opens the server already there rather than
meeting the 409 a second serve of one folder is, else serves, says so, remembers the folder and opens
the server's `page` in a tab at home in the checkout.

## The browser tab

A tab is a session of the browser store (`shell/useBrowsers.ts` over `browsersModel.mjs`): a
key (`b<n>`), its **home** — the screen it was opened beside, `BROWSER_SCOPES`: a workbench root
(`workstream`, `goal`, `work_item`), a `workflow` on the designer, a `channel`, a direct message
(`dm`), a `conversation`, or `null` for the workspace's own — **who opened it** (`by`: a person, an
agent's request with the agent's id, or a page's window with the asking tab's key — `PERSON`,
`byAgent`, `byPage`; `openBrowser` opens nothing for a target that names no opener) — its URL, its
title, whether it loads. **The tabs in sight** are remembered under `bisa.browser.sessions.v3` while
`browser.remember_tabs` says so, so a restart brings back what a person was looking at; a tab kept
out of sight is an agent's working page for that run — it ends with the window and is never loaded
again at a launch nobody asked it of (`serializeBrowsers`). A tab at home in a workbench root rides that root's centre strip with the
terminals **while that root's centre shows documents** — `{kind: "browser", key}` in `mergedTabs`,
never stored in the workbench's tabs, never moved to another pane, its id `browser:b1` in `?doc=`
— and in the Details pane's Browser occupant while the centre is the conversation or the Board
(ide/09 §Agent Mode), where the strip is not drawn: the same place every other screen's browser
is. What the centre shows is one published fact (`views/_workbench/workbenchCentreStore.ts`,
`publishWorkbenchCentre` from `Workbench.tsx`, `workbenchCentreFor` read by the doors; the word is
`ideModeModel.centreOf`), and the one rule is `browserBridgeModel.revealPlan(session, current,
centre)`: `ide` at home with documents in the centre, `pane` otherwise, `null` for a tab kept out of
sight. A switch of the mode carries the tab (`browserDoorsModel.followCentre`, applied by the
Workbench on a switch of the same root, never on a mount or a root change): the strip's active tab
goes to the pane leaving documents; the pane's tab at home here comes back to the strip returning,
the pane's occupant closing. A pane closed by hand while the centre is the conversation stays
closed until the person opens it or returns to documents. The strip's body
(`views/_workbench/BrowserDoc.tsx`) is the shared bar over a `LayerSlot` with the wand added and
the annotation tray under it. Every tab, whatever its home, shows in the Browser pane (§The
Browser pane). The bar (`shell/BrowserBar.tsx`) is one component wherever the tab shows, and what
it shows is `shell/browserChromeModel.mjs`'s word (`chromeOf`): **a browser's bar, whole on every
tab** — back, forward, reload (**Stop** while the page loads), the address field (`normalizeUrl`:
a bare `localhost:5173` is `http://`; another scheme is refused; the whole address selected on
focus, Escape restores it), the wand (§Annotating a page), a *served* chip when the node serves the
page, the **camera** (§Screenshots), outside the IDE *Open in the Project IDE* for a tab at home in a
root, and *Open in the machine's browser* — each verb free or **held with its reason** on the
tooltip (`HELD`), never absent: a blank tab holds everything but the address (*Type an address
first*), back and forward follow the webview's own history (`canBack`, `canForward` on the session,
said with every load), the wand is held on a page nobody may edit. `browser.home` is the URL a new
tab opens on; blank opens the blank page, the field ready. A load the webview never finishes — a
port nobody listens on, since WebKit reports a load's start and its finish and never its failure —
settles after `LOAD_GRACE_MS` (`shell/useBrowsers.ts`) and says so once, so a tab never reads
*loading* for good.

**The browser chords** (ide/15, the `browser` scope, live while a browser tab's body has the focus
— `[data-browser-doc]` on both hosts' bodies): `⌘L` the address field, `⌘[` and `⌘]` back and
forward, `⌘R` reload, `⌘T` a tab beside at the same home, `⌘W` close the browser tab
(`close_browser_tab` — in the IDE and in the pane alike; the workbench's `close_tab` is another
command). Typed in the main window they reach the active tab's bar through the `BROWSER_COMMAND`
door (`shell/shortcuts.ts`), at which the bar stands with `onDoor`; pressed **inside the page**,
where no key reaches the main window, the page's script relays them as a `bisa:key` message and
the bridge fires the same door for that tab. The script is written with the keymap's browser
bindings as the tab opens (`relayChords` → `browserScript`), so a rebound chord works inside a page
opened after the rebinding, and a chord spelt with `Ctrl` is the main window's alone. A bar answers only for its own
tab and only from the host that draws it (`placementOf`), since both hosts can hold a bar for one
tab and the centre wins.

**The page is a native child webview**, not an iframe (`desktop/src-tauri/src/browser.rs`,
Tauri's `unstable` feature): `browser_open` builds a `WebviewBuilder` on the URL — `about:blank`
for a blank tab — with the kit's `browserScript(relay, theme)` as its initialization script, admits http(s)
navigations and the blank page only, adds it to the main window **hidden**, one pixel out of the
way, and installs the page's door (§What a page may reach); `browser_bounds` is **one atomic
placement anchored on the main webview** (§Where the page goes) that answers the box the shell
actually drew, read back (`Placed`); `browser_navigate`, `browser_back`,
`browser_forward`, `browser_reload`, `browser_stop` and `browser_close` are the bar's verbs, the
webview's own `goBack`, `goForward`, `reload` and `stopLoading`; `browser_drive` hands the page's
script a request; `browser:navigated` carries every page load's start and finish **with the
webview's history** (`canGoBack`, `canGoForward`, read on the main thread as the page moves) — and
a stop, since the webview reports no such thing, is said by `browser_stop` itself as a finish on
the page the tab is on; `browser:titled` is the page's title as the webview reads it
(`on_document_title_changed`), for every page; `browser:new-window` is a window the page asked
for — a link with a target, `window.open` — which the webview refuses (`on_new_window`, `Deny`) and
the layer's bridge turns into a tab of ours at the asking tab's home (`noteBrowserOpened`), when it
is an http(s) page at all **and the asking tab is still open** — a window asked for a tab that is
gone is nobody's and opens nothing — standing **as the asker stands**: in sight beside a page a
person looks at, kept out of sight beside one that is, so a hidden page puts nothing on screen
(`by: page`); `browser_screenshot` is WebKit's own snapshot of the webview
(§Screenshots). The layer (`shell/BrowserPanel.tsx`, mounted in `App.tsx` beside the terminal
layer, the one file that opens, moves and closes the webviews) opens a webview per session, places
it **the moment its open resolves** — a placement asked before the webview exists is refused and
would be lost — and on every change of the slots, lays each over the **host** that shows it and
hides the rest, and compares every box read back with the box it asked
(`browserPlacementModel.placedAsAsked`, a pixel of tolerance): a page drawn anywhere else — over
the bar, say — is logged once per tab as *browser placed out of place* with both boxes and the
viewport, so where the page went is a fact in the log and never a guess.

**Where the page goes.** The slot's box is in the main page's CSS viewport — `getBoundingClientRect`
— while a child webview is an `NSView` standing in the **browser layer** (`browser.rs` `mod layer`,
`BisaBrowserLayer`: a view the content view's size, unflipped, just above the main webview's own
view; a tab is moved into it once, as it opens, still hidden), in AppKit points from the
bottom-left. The two spaces agree only when the
page's `(0, 0)` is the content view's top-left corner and one CSS pixel is one point; whenever the
page's viewport sits lower than the main webview's frame (a content inset above the page), the
main frame is shorter than its parent, or the page is zoomed, a box handed straight to the child
lands too high by that difference — over the bar. So every placement carries the page's own
viewport (`window.innerWidth`, `window.innerHeight`) and the shell places the child **on the main
webview's frame** (`browser.rs` `mod anchor`): `Anchor::of(parent_height, main_frame, viewport)`
reads the zoom as the frame's width over the viewport's, the inset as what the frame is taller
than the zoomed viewport, and the page's top edge as the main frame's top plus that inset;
`frame_for(bounds)` is then the slot's box in points, one `setFrame` and `setHidden(false)` on the
main thread in one closure with a budget (`PLACEMENT_BUDGET`), and `placed_from` reads the frame
the child actually took back through the same anchor — a real read-back, not the input's inverse.
The anchor is logged once per change as *browser anchor* (the parent's height, the main frame,
the viewport, the zoom, the inset, the top), and a resize or a zoom change re-publishes the slot,
so the anchor follows on its own; the bar above the slot is visible on every tab because the page
is placed where the page's coordinates say.

**Two hosts, one rule.** `shell/layerSlots.ts` keeps one slot per host — `center`, the
workbench's body while a terminal or browser tab is active there, and `aux`, the Browser pane's
body — each published by the `LayerSlot` standing in it (`shell/LayerSlot.tsx`: its rect, which
layer, and which tab's key). `browserPlacementModel.placementOf(key, {center, aux, mayShow})` says
where a tab draws: the centre when it stands for that tab, else the pane when it does, else
nowhere; a tab standing in both draws in the centre, the bigger box, and the pane says so. The
terminal layer reads the centre slot as before.

**A native view paints above every DOM element**, so it yields: the kit counts its open surfaces
(`ui/openSurfaces.ts` over `surfacesModel.mjs` — `Dialog`, `ConfirmDialog`, `Popover`, `Menu`,
`ContextMenu`, the link card, the artifact stage and the palette each say when they open and leave;
a menu owns its open state through `useOpenSurface`, a card mounted only while open says
`useSurface(true)`), and the layer hides while any is open — a right-click menu from the explorer
or a rail row is drawn over the page, never under it. A tooltip is **not** a surface: hover-driven
and `pointer-events-none`, counting it would blank the page on every hover; a toast is read over
the page's edge and never hides it. The guard `ui/openSurfaces.test.mjs` reads the kit: every
component that portals at the `z-50` tier counts itself or is allowed there with its reason.
Nothing of the annotation flow has to paint over the page: the note box is the page's own
(§Annotating a page).

**The page's floating overlays are not surfaces: the layer cuts around them.** The Notes and Draw
panels (floating, not maximized), their docks, the pet and every addon window float over the
content where a tab may stand, and hiding the page for each would blank it most of the time. So
each says its painted box (`shell/browserClear.ts`, `useBrowserClear`: the element's
`getBoundingClientRect` and border radius, re-read on every render, on a resize and at the end of a
transition; a dock's count badge says its own), `browserClearModel.mjs` keeps the ones that meet
a slot holding a browser tab — kept while a surface hides the tab, so it shows again already cut,
never over an overlay for a frame — and `BrowserPanel.tsx` sends them once a frame, only when they
changed, with the page's viewport (`browser_clear`). A floating panel that opens while a tab
shows asks for the keyboard (`browser_focus_main`), since the page may hold it natively. The shell maps each box through the tab's own `Anchor` (`browser.rs`
`mod clear`, pure and unit-tested) into **disjoint** pieces — a lone overlay keeps its rounded
corners; overlapping ones are cut as their union, each corner no other overlay touches kept round
as a quarter disc — and the browser layer leaves them see-through:
its layer's **mask** (a `CAShapeLayer`, even-odd, set in `layout`/`updateLayer` with implicit
animations off — "fully transparent pixels block that content", Apple, `CALayer.mask`) shows the
main page there, and its **`hitTest:`** answers nothing for a point in a piece, so the window's own
hit-test goes on to the main webview and the overlay gets the click, the scroll and the keys ("you
might want to override it to have a view object hide mouse-down events from its subviews", Apple,
`NSView.hitTest(_:)`). The page keeps its size and stays live everywhere else; with no piece the
mask is dropped, so the common case costs nothing. Three limits stay: shipping WebKit still sends
mouse-moves to a tab under a hole (a page tooltip may peek out; WebKit's own topmost check, which
asks this very `hitTest:`, closes it), a file dropped on an overlay over a page goes to the page
(AppKit finds a drop target by a private lookup that ignores `hitTest:`), and where an overlay is
see-through — the pet, a transparent addon — the main page's own ground shows, not the tab's.
Off macOS the shell answers `false` and cuts nothing: an addon window still hides while it meets a
tab (`addonWindowModel.hiddenByLayer`), the rest stays as it was.

## What a page may reach

Nothing of the app. The tabs' webviews stand in **no capability**, so no page — this machine's or
the web's — has IPC. The one door out of a page is the shell's own: `browser_open` puts a
`WKScriptMessageHandler` named `bisa` on the tab's user content controller (`door::PageDoor`,
`objc2-web-kit`), and the page's script posts to `window.webkit.messageHandlers.bisa` — one JSON
text per message, which the shell parses into one object under `MAX_MESSAGE_BYTES` (anything else
is dropped whole) and re-emits to the main window as `browser:message`. The same door on every
page is what lets the inspector's picks, the driver's answers, a page's own moves and a chord
pressed inside the page come back from the web as from a dev server. What a page says is **data**:
the main window bounds every field (`parseInspectorMessage`) and takes a message as a pick to draw,
an answer to hand an agent, a title, or one of six chord ids — never as an instruction. A page's
own scripts can reach the door too, so a page can at most post a false pick, which becomes a chip
the person sees and can remove, or a false answer, which an agent reads as it would the page's
words — and every page's words pass the **content screen** before the agent reads them, framed as
data, held and asked about when they read as instructions
([11 § What an agent reads from outside](../11-security.md#what-an-agent-reads-from-outside)). No
CSP or capability of the main window changes.

## The Browser pane

The Details pane's `browser` occupant (`shell/BrowserPane.tsx`, drawn by the shell from the URL
like the artifact and transcript occupants — `?aux=browser&auxId=<key>`) beside any screen: a
strip of every open tab (its label, its home on hover, ✕, *+*), the shared bar for the active one
— the wand on it, as in the IDE — a `LayerSlot` on the `aux` host under it, and the tray under
the page (§Annotating a page). `auxId` focuses that tab **while it is in sight** — an address can
be a memory, and showing a tab kept out of sight is a click's to do. **The pane opens no tab of
its own**: with none in sight it says so — *No browser tab here yet* — and offers *New tab*; a
pane that merely mounted (an address the place memory brought back, an addon's `navigate`) once
made a blank tab nobody asked for, and the footer counted it. The person's doors keep the one
step: ⌘⇧L and the palette go through `browserDoors.toggleBrowserPane`
(`browserDoorsModel.paneToggle`: *hide* while the pane shows, *show* while a tab is in sight,
*open* one — the person's — when there is none). The pane's width is bounded by
the room, not by pixels (`auxPaneModel.auxBounds`): the floor is 300 px and the ceiling seven
tenths of the content column `App.tsx` measures for its one-column rule and hands the pane as
`available`; the stored width (`bisa.aux.width`) is kept as chosen and what is drawn is that
width held within the bounds (`shownWidth`), so a resize of the window never rewrites the choice. The doors are written once
(`shell/browserDoors.ts`): `showBrowserTab(key)` — where a tab belongs by `revealPlan`, the strip
or the pane; `openBrowserAt(home, url?)` — a tab opened at a home and shown through it (the
Browser button's, the keymap's `new_browser` through `NEW_BROWSER_HERE`, the agents' bridge);
`openBrowserPane(key?)`; `openUrlInBrowser(url)` — at home in the screen's place and shown where
that place's browser is; `openArtifactInBrowser(artifact)` (§Artifacts in the browser). They are reached from ⌘⇧L and the palette (`open_browser`, toggling the occupant
through the `OPEN_BROWSER` door the pane hears — a toggle that closes at once when the pane shows
the occupant on any tab, `auxPaneModel.toggledAux`), the footer's **Browser** read-out
(§The footer's count) — no screen outside the Project IDE carries a **Browser** button: the
IDE's (§The Browser button) is the only one, and beside any other screen ⌘⇧L opens a tab at home
in what the screen shows (`browserDoorsModel.screenHome`: the conversation on screen, else a goal,
a workflow, a channel or a direct message by the route, else the workspace),
the footer's ports: a
port a checkout's shell opened goes to the IDE, a port with no root to the pane; a URL in a message,
whose card's first verb is *Open in Bisa's browser* (`shell/linkHandler.tsx`, the machine's browser
the second); and the palette, where an address typed — a host with a dot or a port, a whole URL —
is one row that opens it (`shell/omniboxUrlModel.urlRow`).
Which tabs are busy is `shell/browserActivityStore.ts`'s word (`browserActivityModel.mjs`, a
count of requests in flight per tab): the bridge runs every act on a tab — an open, a read, a
back, a drive, a screenshot — inside `working(key, …)`, and a tab closed is forgotten.
None of these renders outside the desktop shell or while `browser.enabled` is off
(`canOpenBrowser`, `whyNotBrowser`).

## The footer's count

**A tab is born only from an act, and wears whose.** Every tab comes through one door
(`useBrowsers.openBrowserIn`, which logs the birth) and every caller says who opens it: a
person's door (`PERSON` — the Browser button, ⌘⇧L with no tab in sight, *New tab*, a link's
card, the palette's address, a port, a served folder), an agent's request (`byAgent`, the
engine's `BrowserScope.agent`), a page's window (`byPage`). A pane that shows, a remembered
address and a launch open none and reveal none — so the number below only ever moves because
somebody moved it, and says who.

The footer's **Browser** read-out (`shell/BrowserStat.tsx` over `browserStatModel.mjs`) is the
one place every tab is counted wherever the person is, and it tells the truth about them. It
follows the browser's switch (`useBrowserPrefs`) and both hosts' slots (`useLayerSlot`): hidden
outside the shell, while the browser is off, and **before the machine's settings have been read**
(`canOpenBrowser` requires `browserPrefs().read`, so the boot window shows no browser it cannot
vouch for); it is **one glyph and one number** — every open tab, in sight or kept out of sight —
pressed while any host draws a tab — the pane, or the IDE's centre
(`browserPlacementModel.shownTab`); the working dot while an agent browses; and its tooltip
`browserPlacesModel.footerBrowserWords` — *3 browser tabs — 2 opened by agents, 1 out of sight, an
agent browsing in 1; showing Pricing* (the agents' share named, so a number that surprises says
whose it is). A click opens its overlay (`BrowserOverlay.tsx`) in the shape the memory and
disk read-outs open (§The footer's resources in the desktop guide): the sentence as the title,
*New tab* and the door to Settings › Capabilities › Browser beside it, a bar (the kit's `StackedBar`) of the tabs in sight against the ones out of
sight (`visibilityBar`), and a control of three dimensions remembered the way a resource's is
(`resourceDimensionStore`, under `browser`; `chosenDimension`) — **Tabs**: every tab in the order
opened, its label and where it is at home **by name** (`whereWords`: *Goal · Ship the storefront*,
*Project IDE · Bisa › feat/a*, *Channel · #general*; a place the rows do not name yet reads by its
kind and the id's tail, never a bare id), *— an agent is browsing* after it, then **who opened it**
(`browsersModel.openerWords`: *opened by Reviewer* — the agent by name — or *opened by a page*;
nothing for the person's own), *on screen* on the
tab a host draws and *unseen* on one kept out of sight (dim, its visibility words as the row's
title), every row a door to the pane on its tab and a ✕ of its own beside the row, never inside it
(the one place a tab out of sight is closed without showing it); **Origins**: one row per origin —
Project IDE, Goal, Workflow, Channel, Message, Conversation, Workspace (`groupTabs`, `ORIGINS`),
empty ones left out — how many tabs, how many out of sight and how many an agent browses in, its
bar the share of the largest, its door the origin's first tab in sight; **Unseen**: the tabs kept
out of sight alone, one click showing each (`openBrowserPane` reveals first) — and the
out-of-sight policy as the footnote (`footnote`). *New tab* is at home **where the person is**
(`browserDoors.screenHome` over `browserDoorsModel.screenHome`: the conversation on screen the
surface published, else the IDE's root, else the workflow or screen the route shows, else the
workspace — the one rule a link's *Open in Bisa's browser* and the palette's address row follow
too), and a tab that could not open opens no pane. Every fact is `browserStatModel.mjs`'s
(`statWords`, `visibilityBar`, `overlayRows`, `emptyWords`, `footnote`); the read-out and the
overlay paint. The names are one index over the workspace the shell already holds
(`browserPlacesModel.browserPlaces` through `shell/browserPlaces.ts`: the goals by title, the
workstreams by project and branch through `footerSessionsModel.placeIndex`, the channels and
direct messages by name, the Inbox's conversations by title, the workflow rows read once and
again when one changes), and every other list of tabs — the IDE's Browser button, the pane's
strip — wears the same words.

## Screenshots

`browser_screenshot` (`desktop/src-tauri/src/browser.rs`) is `WKWebView.takeSnapshot` on the
tab's own webview — the page's render, scaled to `browser.screenshot.width` (320–4096, 1280 by
default), encoded as PNG by AppKit — never the screen, so no screen-recording grant is asked. The
call runs on the main thread and answers over a channel within `SNAPSHOT_BUDGET`; on another
platform it says so. The bytes come back to the main webview as raw IPC bytes, and
`browserShotModel.pngSize` reads the size from the header. A hidden webview renders nothing worth
keeping — and a snapshot asked of one *after screen updates* never comes, holding the page's own
paints while it pends — so **no snapshot is asked of a hidden page**, at either end: the shell
refuses a hidden view at once (`SNAPSHOT_HIDDEN`, checked before WebKit is asked anything; a late
one is `SNAPSHOT_LATE`), and the desktop's one door to a shot, `browserShots.takeShot`, **waits until
the tab shows** (`shell/browserShown.ts`: `shownNow` over the hosts' slots and the surfaces' count,
`untilShown` at once when the tab already shows, else on the slots' or the surfaces' next word —
a menu closing is a change — then two frames and a beat for the paint, `SHOW_MS` at most) and
refuses as `NotShownError` when it does not — the camera's toast says *the page is not showing*,
the bridge answers the agent `NOT_SHOWN`. The bridge reveals a seen tab first (§Agents browse
here); a headless one renders offstage and is snapped there. What is delivered
(`shell/browserShots.ts`):

- **for an agent**: uploaded to the workspace's attachments (`POST /attachments`, content-addressed,
  25 MiB cap) and answered as the `AttachmentRef` on the `BrowserResult` — never bytes through the
  bridge, whose route is a sentence-sized body — which the engine turns into the **path of a named
  copy** beside the store (`put_attachment_named`, `browser-<tab>-<ulid>.png`), the one path the
  tool prints: *screenshot: … (1280×800) — read the file to see the page*. A harness reads it with
  its own file tools; the desktop never names a path.
- **for the person**: the bar's camera is **two buttons**, never a menu — a menu is a surface the
  page yields to, and would blank the very page about to be photographed as the person chooses.
  *Copy a screenshot* puts the PNG on the clipboard through the shell (`copy_image`, the picture's
  `copy_text`: the webview's own clipboard takes a picture only inside a gesture, and the shot lands
  seconds after the click); *Save a screenshot as…* uploads it, has the node name the copy and
  copies from there where they choose, the route an artifact's *Save as* takes. Both are held with
  the reason while a shot is being taken.

## Who may ask

The `browser.*` settings ([13](13-settings.md)): `browser.enabled` (machine — the switch, off means
no tab opens and every launcher says so), `browser.home` (workspace or project), `browser.remember_tabs` (machine),
`browser.agents` (workspace or project: *everyone* — the default — · *assigned* · *nobody*),
`browser.agents.reach` (*anywhere* · *local_only*), `browser.agents.headless` (workspace or project:
*unattended* — the default — · *always* · *never*, §Headless tabs), `browser.agents.scripts`
(workspace or project: *allow* — the default — · *refuse*: whether `browser_eval` may evaluate a
script in a page; refused, the tool names the setting, `SCRIPTS_REFUSED`), `browser.screenshot.width`.
**The engine is the boundary** (`intake::browser`, `browser::Access::refusal`): before a request is
parked, the switch, then the policy against the asking agent — the session's, else the work item's,
else the General Agent's — then the reach against an `open`'s URL, then the scripts policy against
an `eval`; the settings are read for the checkout's project or the work item's (`browser::project_of`). *Everyone* is the default
because the guard refuses the machine's browser (§Agents browse here): the embedded browser is the
one browser an agent has, and every catalog agent carries the skill **Embedded Browser**
(`BROWSER_SKILL`, `embedded-browser`) as its guidance. *Assigned* narrows to the agents carrying
that skill; the General Agent and the Workflow Agent always may; *nobody* refuses them too. Each refusal is a sentence
naming what to attach or set (`OFF`, `NOBODY_MAY`, `NOT_ASSIGNED`, `LOCAL_ONLY`), so the agent tells
the person rather than working around it. The twenty-one tools stay on every session's menu — a tool
list is a menu, not a permission, the same argument `core_agent_only` makes. Settings ›
Capabilities › **Browser** (`BrowserAccessPanel.tsx` over `browserSettingsModel.mjs`, the rest of
the group as registry rows) draws the switch on a status card — with how many tabs are open and how
many are kept out of sight — the two policies as three-way switches with their sentences, and
scripts in a page as a two-way one.

## Headless tabs

A tab an agent opens can be **kept out of sight**: its native webview renders and answers every
tool — a read, a click, a screenshot — and nothing opens beside anyone. Whether it is, is the
**engine's decision**, made at the op and carried on the parked request and the bus frame
(`PendingBrowserRequest.headless`, `EnginePayload::BrowserRequest { headless }`):
`browser::headless_for(policy, mode, asked)` — the agent's word first (`browser_open`'s `headless`,
`BrowserRequest.headless`), else the policy, *unattended* reading the goal's mode
(`GoalMode::unattended`, an auto goal; a conversation with no goal has a person in it, so it is
shown). The desktop draws such a tab **offstage** (`browserPlacementModel.placementOf`: a host
that stands for the tab still wins; else `{host: "offstage"}`, a box a desktop viewport wide
wholly above and to the left of the window — in the window still, so WebKit keeps rendering — and
a dialog over the window changes nothing offstage); `isShown` is true there, so a screenshot is
taken where the tab renders (`browserBridge.untilShown`), never by revealing it. The store keeps
`headless` on the session (remembered across a restart), opens such a tab without making it active,
lists only the **seen** tabs on the IDE's strip (`seenRootedAt`) and as the pane's shown tab
(`seenSessions`), and one act shows it — `reveal` (`revealBrowserTab`): the person's click on the
tab in the pane's strip (dim, the hidden glyph), on its row in the footer's Browser overlay (under
*Unseen*, or worded *unseen* under *Tabs*), or in the IDE's
Browser menu, or the pane opened on it (`openBrowserPane` reveals first). **Nothing an agent does
reveals a headless tab** (`revealPlan` answers `null` for one): only the person, or the agent's own
`headless: false` on a later `browser_open` of that tab.

**Nobody home is said at once.** `BrowserRequests` keeps when a desktop was last heard from —
every `GET /browser/requests` and every answer stamps it — and the op refuses with `NOBODY_HOME`
before parking when none was heard within `DESKTOP_PRESENCE_TTL` (45 s); an open desktop reads the
list every `BROWSER_PRESENCE_MS` (20 s, `BrowserPanel.tsx`; the second missed read in a row is a
warning in the desktop's log, since the engine is about to think nobody home), so it is never
thought gone, and a request parked while one is home still waits `ANSWER_TIMEOUT` — and, unanswered,
is answered with a different sentence, `DESKTOP_SILENT`: the desktop *was* here, it is why the
request was parked, so its silence is not its absence, and the agent is told to try once more rather
than stop. An agent with no desktop to answer it reads `NOBODY_HOME` in a moment, not after a minute
per call.

## Artifacts in the browser

An html or svg artifact's `…` offers **Open in the browser** (`ArtifactView`'s `onOpenInBrowser`,
a host verb the pane, the stage and the IDE tab pass from `browserDoors`): `POST
/artifacts/{sha256}/serve {name}` has the node make the named copy and serve **its folder** on a
fresh loopback port through the same `Servers` as the checkouts' folders (`ServedOwner::Artifact`), so
the page opens with an origin of its own — a port, never the node's origin, which serves no agent
page ([12 §Security](../12-artifacts.md#security)). `page` is the file's URL, its name escaped; a
second ask answers the server already up; `GET /servers` lists every server of either kind and
`DELETE /servers/{id}` stops one. An artifact's server resolves to no file: its page is nobody's
to edit through an annotation.

## Annotating a page

The page's script is the inspector's core over the shell's door (`pageInspector.mjs`:
`INSPECTOR_CORE`, shared with the frame's `inspectorScript(theme)`; `browserScript(relay, theme)`
differs only in how words leave the page — the `bisa` message handler — in
`window.__bisaBrowser.perform`, the app's way in, and in the chords it relays). **The overlay wears
the app's theme**: `inspectorTheme.mjs` maps the role tokens read off `<html>` (`inspectorTokens.ts`:
colour, radius, shadow, font and motion as `getComputedStyle` gives them, the two type sizes measured
on a probe) to the `cssText` of every part — outline (an accent edge with no fill, so the element
under it stays readable), tag label, note box, crumbs, input, *Add*, badges (which, like the outline
and the tag, never catch the pointer) — with no colour, radius or font of its own (`pageInspector.test.mjs` builds the script from a
sentinel theme and finds nothing else in it). The dress is written into the tab's script as the tab
opens, so every document the tab loads starts dressed, and said again as `bisa:theme` by
`useBrowserAnnotation` after each load and whenever the theme, the accent, the scheme or the type
scale moves (`useInspectorTheme`, the `<html>` attribute observer every theme listener uses): a box
open re-paints where it stands, its words still typed. The page paints by `style.cssText`, which no
Content-Security-Policy — the frame's own or a web page's `style-src` — can block; hover and focus
are states the core switches between, not selectors. Whether a page can be annotated is `serversModel.annotatable`'s
one rule: **any page the tab shows** — the checkout the node serves, a dev server, the web — and
**never an artifact's page** (an agent's own, nobody's to edit: the tab's origin is matched against
`GET /servers`); a blank tab shows nothing to point at. The wand is on the bar in both hosts, held
with its reason where it cannot be pressed; *Annotate the page for an agent…* in the Browser
button's menu for the active tab here and in the tab's context menu (`tabMenuModel.browserTabMenu`)
turn it on too. The half both hosts share is `views/_workbench/useBrowserAnnotation.ts`: whether
the page can be annotated and why not, which page the elements are on, the draft, the wand and the
lost badges (`browserInspectorStore.ts`, the tab's own state), and the two words the page is
driven with — the mode and the badges with their notes — said **once a load has finished**, into
the document that is there and never the one leaving. **The page draws the note box itself**: the
inspector core opens it over the clicked element — the crumbs, the element's text, *What should
change here?*, *Add* or *Change* — exactly as it does in a rendered file's frame ([03](03-files-and-editing.md)
§Annotate), one implementation for both, so the wand behaves the same on a dev-server page and on
a `.html` file of the checkout; the note typed there comes back as `bisa:note` with the whole
pick, a box closed without one as `bisa:closed`, and the lost badges and Escape as before, all
through `browser:message` into the store — `listenBrowserNotes` hands each note to the hook,
which adds the annotation.

The draft is the session's, keyed by the **page** and the tab's home
(`annotationsKey(draftScopeOf(session), page)`): a page served from a file of the checkout resolves
to that file (`…/servers/{id}/resolve`) and its chips are `Annotation { page: File { path } }`
under the edit contract, the page following the agent's write as a rendered file does; any other
page is `Annotation { page: Url { url } }` — the agent reads *the annotated element of the page at
<url>* and the element as it was — and nothing on disk is followed. `bisa_core::PageRef` is the
wire's word for both (`{kind: file, path}` · `{kind: url, url}`), and `framing::context_block`
prints the path or the URL.

**Annotating is the Project IDE's alone.** `useBrowserAnnotation` reads the route: on the Project
IDE's screen (`workbench`) the centre's tab and the Details pane's have the wand, the note box and
the tray; beside any other screen the Browser pane shows the page and draws no wand (`BrowserBar`'s
`wand` is optional), and a wand left on — or badges left drawn — when the person leaves the IDE are
taken back from the page (`inspectMessage("off")`, `marksMessage([])`). The draft stays the
session's and the badges come back with it in the IDE.

**The note box** is drawn by the page's own inspector in the kit's popover language: a solid
surface (the theme's `surface` with its alpha taken off, `inspectorTheme.opaque` — nothing frosts
behind a box in a page), the element's three nearest parents as one line of quiet crumbs with the
element a neutral chip, a close drawn in CSS, the element's text, the field, and a foot with the
keys' hint and the one primary *Add* (*Change* for an element already annotated); its words are
catalog messages baked into the script (`pageInspector.inspectorWords`).

**Where the chips go** is `serversModel.annotationHome`'s word. A tab at home in a workstream has
the **checkout's tray** (`AnnotationTray`; in the pane `CheckoutAnnotationTray`, which reads the
checkout's project itself): **Send** to an agent as an edit into the checkout's conversation, or
**Attach** to the IDE's Agent pane. Any other tab — a goal's or a work item's, in the IDE's centre
or in the pane beside it — has the **screen's tray**
(`views/_workbench/PaneAnnotationTray.tsx`): **Attach to the message** puts every annotation as an
`annotation` chip in the tray of the conversation on screen, whose composer sends them with the
words the person writes, and *Clear*. The conversation on screen is **published, never looked up**
(`views/_studio/chatTargetStore.ts`: `Conversation` says its kind and id while mounted and withdraws
them as it goes, the way the Details pane hands its slot over), and every conversation surface
keeps its own chip tray under `chatKey(kind, id)` in the one context store (`agentPaneStore`,
`attachTo` — the door that attaches without opening the IDE's panel) unless its host brings one,
as the IDE's Agent pane does; with no conversation on screen the button is held and says what to
open. The rows both trays draw are `AnnotationRows.tsx`.

## Agents browse here

Agents have no browser of their own. The injected `bisa` MCP server holds **twenty-one tools on every
scope** ([reference](../../reference/mcp-tools.md); `bisa_core::browser::BROWSER_TOOLS` is the one
list the note, the skill, the reference and the server are checked against) — a person's browsing:

| Act | Tools |
|---|---|
| open and move | `browser_open` (a new tab, or a tab it holds), `browser_tabs`, `browser_back`, `browser_forward`, `browser_reload`, `browser_close` |
| serve | `browser_serve` — a folder of the session's checkout on a loopback port of this machine, the URL answered (§Serving a folder) |
| see | `browser_snapshot` — the page's outline: every heading, landmark, link, button, field, select, checkbox and menu item, one line each with a **ref** (`e12 button "Sign in"`, `e13 textbox "Email" value="…"`, `e14 link "Pricing" → /pricing`, `[disabled]`, `[checked]`); `browser_read` — the page or one element as text or HTML, an element led by its role, name and attributes; `browser_find` — words with their surroundings; `browser_screenshot` |
| act | `browser_click`, `browser_type` (a keystroke at a time — keydown · keypress · input · keyup per character, `clear`, `submit` presses Enter), `browser_fill` (a value in one go), `browser_press` (Enter, Escape, Tab, the arrows, a character, with modifiers; Enter in a form's field submits it), `browser_select` (by value or label), `browser_hover`, `browser_scroll` |
| settle | `browser_wait` — until the load, an element (`selector`), some words (`text`), an element's going (`gone`), or quiet (`idle`, half a second without a mutation); 10 s by default, 30 s at most (`MAX_WAIT_MS`, under the op's 60 s) |
| inspect | `browser_console` — what the page logged and the errors it raised since it loaded (a ring of a hundred lines); `browser_eval` — an expression evaluated in the page, a promise awaited, its value as JSON bounded to 16 KiB — refused where `browser.agents.scripts` says so (§Who may ask) |

Every element is named by a **ref** from the last snapshot or by a CSS selector — the same
`target` on every tool; the page keeps the refs (`pageInspector.mjs` `refs`) until it loads again,
and a ref that is gone says so (*ref e12 is gone — the page changed; browser_snapshot again*).
Every answer carries the **dialogs** the page raised meanwhile: the browser's program replaces
`alert`, `confirm` and `prompt` at init — an alert dismissed, a confirm accepted, a prompt given
its default — records each and reports them with the next answer, so nothing ever hangs and the
agent reads what the page said. An act that may move the page (`BrowserAction::may_navigate`:
open, click, fill, type, press, select, back, forward, reload) is raced against the navigation it
starts (`browserBridge.ts`: the page's answer against the webview's `started`, then a
`NAVIGATE_GRACE_MS` grace); when the page moves the bridge waits for the load and answers the new
page with `navigated: true` instead of *the page did not answer*; a tab still loading is waited for
before anything is asked of its page.

Every session reads the **one sentence** about them (`bisa_core::browser::BROWSER_NOTE`: a work
item's first prompt, a conversation's framing, the Workflow Agent's wake and the server's
instructions all carry it, so none drifts): the loop *snapshot → act by ref → wait → read
or screenshot*, the development loop — the run command or a served checkout, then `browser_open`
the port — a page read with `browser_read` and never `curl`, the guard's refusal, and the one
licence to stop: when a tool refuses you or says the browser is not available, say so and stop. An
unattended goal's session reads one more (`BROWSER_UNATTENDED_NOTE`,
`framing::browser_note(unattended)`): its tabs are out of sight; ask `headless: false` only when a
person should watch. The note and the catalog skill also say what each problem a tool answers asks
for — a silent read is retried once, an act never, a gone tab is `browser_tabs` then `browser_open`,
a silent desktop is one more try, a platform fault is reported once and the task carries on — and
that the person is never asked twice whether to retry. For every session the platform drives, the guard **refuses** the machine's browser and a headless one
(`machine_browser`, `Action::Deny` with a `hint`: `open`, `xdg-open`, `start` of a URL, `open -a` a
browser, `chrome`, `chromium`, `firefox`, `msedge`, `brave`, `puppeteer`, the drivers, anything
`--headless`) and the refusal names the tools — *refused by the guard rule “The machine's browser,
or a headless one” — the platform's embedded browser is yours: browser_open the page,
browser_snapshot its outline, …*; a project's own end-to-end suite (`playwright`, `cypress`, `wdio`,
`selenium`) is **asked** of the person first (`browser_test_runner`, `Action::Ask`), since it is the
person's suite and not the agent's browser. A harness a person opens in the IDE's terminal is none of
this: nothing of the platform's is injected into it, so it has no browser tools, and these rules —
`applies_to: platform`, with `harness_fetch` and `harness_web_search` — pass it over: it keeps the
machine's browser and its own prompt ([11 — Security](../11-security.md#a-claude-code-session-in-a-terminal)). The catalog skill *Embedded Browser* says how to browse
like a person and how to test a feature with the tools — every catalog agent carries it — and with
`browser.agents` at *assigned* carrying it is the assignment (§Who may ask).

**Every harness the engine drives receives the server.** Claude Code by `--mcp-config`, ACP — a
generic target, GitHub Copilot CLI, Grok Build, Gemini CLI — by the `mcpServers` field of `session/new`, Codex by `-c mcp_servers.<name>.command|args|env=…` overrides on `codex exec`,
OpenCode by the JSON its `OPENCODE_CONFIG_CONTENT` variable carries (`crates/bisa-adapters/src/mcp_inject.rs`:
`codex_overrides`, `opencode_config`); pi and a custom harness take the servers their descriptors
say. An agent browses whatever it runs on.

The bridge is a gate's shape (`crates/bisa-engine/src/browser.rs`): the intake op `browser` —
after the access check — parks the request in `BrowserRequests` — an in-memory entry with a
`watch` — puts `browser_request` on the bus with the request and the **home of the asking
session** (`BrowserScope { home, agent }`), and waits up to 60 s; the desktop hears the frame
(`BrowserPanel` → `browserBridge.ts`), reads what was parked before it opened (`GET
/browser/requests`), performs the request through the tabs and answers `POST
/browser/requests/{id}` with a `BrowserResult` — the tab, its URL and title, `navigated`, the text
cut at 16 KiB, a count, a wait's length, a scroll position, a script's value, the console's lines,
the dialogs, the tabs, a screenshot's reference and size, or a refusal — which the engine hands
back with the screenshot's named-copy path. **Where a tab is at home is the engine's decision**
(`browser::home_of`, word for word the desktop's `BROWSER_SCOPES`): a conversation about a
workstream or a project is in that checkout, about a goal beside the goal, about a workflow beside
the workflow, about the workspace or the node beside the conversation itself; a goal's thread
beside the goal; a channel's or a direct message's turn beside that channel or message (the scope
id resolves through `get_channel`, `ChannelKind::Direct` is a `dm`); a work item in the checkout it
was placed in, else beside the item; a goal the session was launched knowing counts only when it
exists — the MCP scope defaults `goal` to the scope id, and a conversation's id is not a goal; a
note's answer has none. `browserBridgeModel.rootFor` reads the home, else the root the IDE is on,
else the workspace — never refused for want of a root, since the pane shows every tab. The project
whose `browser.*` settings bind is the checkout's or the work item's (`browser::project_of`), so a
per-project policy binds a worker too. A read, a snapshot, a find, a click or a fill reaches **any
page** the tab shows, through the same door the person's picks come by, and nothing on a blank tab;
`browser.agents.reach` bounds where an agent may *open* (§Who may ask), not what it may read once
the person or another open put a page there; a screenshot is of any tab with a page. A tab an
agent opens is **shown** (`revealPlan`): focused in the IDE when the IDE is on the tab's home, else
the pane opens on it beside whatever the person is on — unless the engine said it is kept out of
sight (§Headless tabs), when it is drawn offstage and revealed by nothing an agent does; a
screenshot shows a seen tab first, since a hidden webview renders nothing, and snaps a headless one
where it renders. Nobody home is the sentence the agent reads, at once when no desktop has been
heard from: *the embedded browser is not available — the desktop app answers browser tools, and
none is open*; a desktop that was heard from and still did not answer within the sixty seconds says
so in other words (`DESKTOP_SILENT`, *try the same call once more*). Nothing here is durable. In the
desktop, asking a page comes to one of three things (`browserBridge.askPage`): its answer, its
silence, or the tab gone — the shell's `browser_drive` refused, a webview closed — and the last two
are two sentences (`browserBridgeModel.PAGE_SILENT`, `TAB_GONE`, the second naming `browser_tabs` and
`browser_open` as the way back); an act the plan says cannot move the page (`navigates: false` —
read, find, snapshot, console, eval, wait, hover, scroll) is asked **once more** after a silence, once
the load it may have been in has finished (`askAgain`), and an act that can move it never is: a
second click is a different act. Every hop logs with the request's id and action and never with the
page's words: the engine at debug when a request parks and when it is answered (`elapsed_ms`), at
warn when it times out, at info when the policy refuses it; the desktop at warn for every refusal it
answers. **The envelope itself never fails on a scope**: a conversation session hands its scope id out
as the candidate `goal` on every request, and the intake reads it as a candidate (`GoalCandidate`,
resolved through `goal_of_scope_id`), never as a ULID — a channel's slug names no goal, which is a
fact, not a malformed request; an envelope the engine truly cannot read is refused naming the op, as
a *platform fault* the MCP server hands back as an internal error, not a "Not done".

What stays unbuilt, said plainly: a file handed to a page's `<input type=file>`, cookies and
storage, a viewport size of the agent's choosing, a screenshot clipped to one element, and the
page's network. The tools do not pretend to them.

## Invariants

- The webview never names a program or a path: the Browser menu names a folder relative to the
  checkout and the node resolves it; the run command is the node's answer, opened from the Terminal
  menu, and the shell refuses an unapproved one; a browser tab is a URL and a key.
- The bar is whole on every tab: a verb the tab lacks is held with its reason, never absent; back
  and forward are the webview's own history; Stop stands in for Reload while the page loads; a load
  nobody finishes settles after the grace and says so.
- A page's title is the webview's word, for every page; a window a page asks for is a tab of ours
  at the same home, never a window, only for a page, only while the asking tab is open, and
  standing as the asker stands — out of sight beside a page kept out of sight.
- A tab is born only from an act — a person's door, an agent's request, a page's window — through
  the one door, and wears who opened it; the Browser pane opens none by mounting and reveals none
  from an address; only the tabs in sight are remembered across a restart.
- An artifact's page is never annotatable and a blank tab has nothing to point at; any other page
  is, in the IDE and in the pane, and the same three doors say so; the chips go to the checkout's
  tray for a tab at home in a workstream and to the conversation on screen otherwise.
- A served folder is loopback only, never a dotfile, never a hidden folder served whole, never
  above its folder; at most thirty-two at once; the servers are not records.
- The run phase is never run by the engine (`run_phase` refuses it; the lifecycle phases are three).
- No page has IPC; a page reaches the main window only through the shell's `bisa` message handler,
  the same on every origin, and what it says is data the main window bounds — a pick, an answer, a
  title, one of six chord ids — never an instruction.
- A native layer hides while any surface is open — a dialog, a popover, a menu, a right-click
  menu, the link card, the artifact stage, the palette; never for a tooltip or a toast, and never
  for a floating overlay (the Notes and Draw panels, their docks, the pet, an addon window), which
  the browser layer cuts around instead, so the page stays live under it — and shows
  only over the slot of its own tab, the centre before the pane; a webview opens hidden and shows
  on its first placement, one atomic call whose read-back is compared with the ask, a stray one
  logged and a refused one too.
- A tab at home in a workbench root is shown in the strip only while that root's centre shows
  documents; while the centre is the conversation or the Board it is shown in the Details pane, as
  every other screen's browser is, and a switch of the mode carries it across — the strip's active
  tab to the pane leaving documents, the pane's tab at home here back to the strip returning, the
  pane's occupant closing. Every door — an agent's act, the Browser button, a link, the keymap —
  shows a tab through the one rule (`revealPlan`) and never flips the mode; a document does.
- No screenshot is asked of a hidden webview: the desktop waits until the tab shows and the shell
  refuses a hidden view at once; the camera opens no surface over the page it photographs.
- Every list names a tab's home by the place's name and its origin, never by an id; every door
  gives a tab the home it was opened beside; the footer's read-out is one glyph and one number
  for every tab, follows the browser's switch and the hosts' slots, and never says a browser it
  cannot vouch for; its overlay is the resources' shape and every fact its model's.
- An annotation on a served page is a file chip when the file is known and a URL chip otherwise;
  nothing reaches the agent that is not a chip or a tool's answer.
- An agent's browser request is answered by a desktop, refused at once when none has been heard
  from (`NOBODY_HOME`), or times out in other words (`DESKTOP_SILENT`); the desktop tells a silent
  page from a gone tab, asks a page once more only for an act that cannot move it, and logs every
  refusal with the id and the act; the tool never touches a page itself; who may ask — and whether a
  script may run in a page — is the engine's check, not the tool list's.
- A conversation's scope id rides as a goal *candidate* on every request and the engine is the
  discriminator (`GoalCandidate`): no scope — a channel, a direct message, a conversation about
  anything — ever fails the envelope; an envelope the engine cannot read names the op and is a
  platform fault, never a refusal.
- Where a tab is at home is the engine's word, one of the desktop's own scopes: the checkout a
  conversation runs in, the goal, the channel or the direct message the turn speaks in, the work
  item's checkout, the conversation — never a goal that does not exist; the desktop obeys or falls
  to the root it is on.
- An element is named by a ref a snapshot handed out or by a selector; a ref outlives no load. An
  act that moves the page answers the new page and says so; a dialog the page raises is answered
  for the agent and reported, never left hanging. The console and the errors are the page's own,
  kept from its load. Every harness the engine drives receives the `bisa` server.
- A tab kept out of sight renders offstage and answers every tool, the screenshot too; whether it
  is kept out of sight is the engine's decision — the agent's word, else the policy, else the
  goal's mode — and nothing an agent does reveals it: only the person, or its own `headless: false`.
- The machine's browser and a headless one are refused with the tools named, and a project's own
  end-to-end suite is asked of the person — for the sessions the platform drives; a harness in a
  terminal keeps the machine's browser.
- A screenshot is the webview's own render, never the screen; it reaches an agent as a path the
  node named and the person as a copy; no bytes cross the bridge.
- An artifact opens in the browser on an origin of its own, never the node's.

## Tests

| Invariant | Test |
|---|---|
| a folder is served from a loopback port with `index.html` for a directory and a 404 for a dotfile; a URL path resolves to the file it lands on; the same folder twice and a folder outside the checkout are refused; a stopped server is gone; the run command is 404 unset, untrusted when set, trusted once approved; the bridge lists nothing and refuses an answer for nobody | `crates/bisa-node/tests/it/ide.rs` |
| through the binary, over loopback, every path sent as it is spelt: a served folder answers its own files and a directory by its index; a dotfile, a hidden folder, a way up and a link that leaves are refused however the URL spells them and no file's words are handed over; a served page resolves to its file; the same folder twice (naming where it is), a folder outside, a file, a hidden folder and a key the body does not know are refused; the whole checkout on a port of its own with `.git` never there; a server stopped through its own checkout's door alone, its port let go; every start and stop said on the bus; the servers gone with the node and not back with it | the journey `crates/bisa-cli/tests/it/e2e/a_folder_served_and_a_browser_asked.rs` |
| an agent's browser tool through the real MCP server: told at once that nobody is home, with nothing left parked; its own checkout's folder served by `browser_serve` and the page there; with a desktop home — the journey reads the list and answers — the request at home in the checkout the conversation is about, shown and not kept out of sight, an answer with a key nobody knows refused, the desktop's answer the tool's, answered once | the same journey, its second test |
| a hidden folder is never served whole wherever a link says it is; the folders served at once have a bound and a stop makes room | `crates/bisa-node/src/ide/serve.rs` unit tests |
| the run command is the fourth script — listed, approved with the others, answered on its own — and never a lifecycle run | `crates/bisa-engine/tests/it/projects.rs` |
| a browser op parks its request, the desktop's answer returns through it once, a request nobody answers says the desktop was silent — a parked request had one — in milliseconds through `wait_for`, and nobody home is said at once when no desktop has read the list; a channel's or a DM's request, built through the real `bisa_mcp::Scope`, parks at home beside the channel; the op refuses by the switch, the policy, the reach and the scripts policy, everyone passes by default, a core agent and a skilled agent pass under *assigned*, a project allows the script the workspace refuses for its own checkout; a tab is at home where the session speaks for every kind of scope — a channel, a direct message, a checkout's conversation, a workflow's beside the conversation and never at a goal that does not exist, a goal's conversation and thread; a result bounds its value and its lists, a request serialises by action, the acts that may move the page are named, a home serialises as the desktop's words; a tab is out of sight in an auto goal and shown in a guided one unless the agent asks, and *always* and *never* say so; a screenshot's upload becomes a named copy's path and a missing one a refusal | `crates/bisa-engine/tests/it/browser.rs`, `src/browser.rs` unit tests |
| the envelope as the real MCP client builds it never fails on a scope: every op that takes a goal candidate — `browser`, `decide`, `post_message`, `emit_signal` — answers a channel, a DM and a conversation about nothing in its own words, a conversation about a goal resolves to that goal through its candidate alone, an op that needs a goal still refuses a slug, and an envelope the engine cannot read is a platform fault naming the op | `crates/bisa-engine/tests/it/intake_scope.rs` |
| a channel-scoped browser call reaches the fake engine with the scope id as its candidate goal; a result's refusal and the envelope's wear the same *Not done*; a platform fault is an internal error and never *Not done* | `crates/bisa-mcp/tests/it/intake.rs` |
| a silent page is asked once more only for an act that cannot move the page and never a third time, a gone tab and a silent page are two sentences, the second missed presence read is the warning; the bridge tells them apart where they arise and logs every refusal with the id and the act | `desktop/src/shell/browserBridgeModel.test.mjs`, `desktop/src/scenarios/browser.test.mjs` |
| every session's browser sentence names every tool, the refusal and the one licence to stop, and an unattended session is told its tabs are out of sight | `crates/bisa-core/src/browser.rs` tests |
| every session holds the twenty-one browser tools and the reference names them; the words an agent reads name the screenshot's path, a move, a wait, a scroll, a value, the console and the dialogs | `crates/bisa-mcp/src/server.rs` tests, `tests/it/docs.rs` |
| the MCP servers reach Codex as config overrides and OpenCode as its inline configuration — a stdio server's command, args and env, an HTTP server's url, a TOML string escaped — and a spec with none adds nothing; both adapters claim `MCP_SERVERS` | `crates/bisa-adapters/src/mcp_inject.rs`, `codex.rs`, `opencode.rs` tests |
| an artifact is served on a port of its own once, by its escaped name, stopped by id; a served view knows its checkout only as a folder of one | `crates/bisa-node/tests/it/node.rs`, `src/ide/serve.rs` unit tests |
| the browser's keys hold their kinds, bounds and scopes | `crates/bisa-core/src/settings.rs` tests |
| the machine's browser and a headless one are refused with the tools named in the reason; a project's own end-to-end suite is asked; a folder or a file opened is not a browser; a rule's hint rides its refusal | `crates/bisa-security/src/builtin.rs`, `guard.rs` tests |
| the page reference round-trips as a file or a URL and frames as its path or its URL | `crates/bisa-core/src/message.rs`, `crates/bisa-engine/src/framing.rs` |
| http(s) pages and the blank page are admitted and nothing else; a page's message is one JSON object under the cap and anything else is nothing; a box is one logical rectangle never thinner than a pixel and reads back as it was; a window asked for is a tab for a page and nothing for the rest; each event has its own name; a tab is a labelled webview the registry forgets once; a screenshot's width stays within bounds | `desktop/src-tauri/src/browser.rs` tests |
| the bar's URL rule, a tab's life at home or the workspace's — the seven homes — its history as the webview says it, its words, where its drafts live, a headless tab opened out of sight and revealed by the person, every tab closed with the sequence kept; a target that names no opener opens nothing, the three openers and their words; what a restart brings back — the tabs in sight with who opened each, never one kept out of sight, never an older shape | `desktop/src/shell/browsersModel.test.mjs` |
| a tab is born only from an act: an addon's navigation to a screen that remembers the pane opens none; ⌘⇧L with none in sight opens the person's; an agent's is counted and named; a page's window stands as the asker stands and a closed tab's opens nothing; a restart brings back the person's alone — and by source: no effect of the pane opens a tab, the address focuses and never reveals, every `openBrowserIn` names its opener, both doors go through `toggleBrowserPane` | `desktop/src/scenarios/browser.test.mjs`, `desktop/src/shell/browserDoorsModel.test.mjs` (`paneToggle`) |
| every home's origin in the list's order; a place named from the workspace's rows — a goal, a workstream, a channel, a message, a conversation, a workflow — and the words with and without a name; the tabs grouped by origin; the footer's button in a sentence | `desktop/src/shell/browserPlacesModel.test.mjs` |
| the footer's read-out counts every tab in sight or not, its title the footer's sentence, the dot's words, pressed while a tab is on screen; the bar of the tabs in sight against the ones out of sight; the three dimensions' words and glyphs in the icon registry; Tabs, Origins and Unseen rows — the order, the home by name, the current, dim and busy rows, every tab's door and ✕, an origin's share of the largest and its door the first tab in sight; the empty words; the policy's footnote | `desktop/src/shell/browserStatModel.test.mjs` |
| where a tab opened beside the screen is at home: the conversation on screen, else the IDE's root, else the workflow or screen the route shows, else the workspace; a switch of the centre carries a tab at home here — the strip's to the pane leaving documents, the pane's back to the strip returning, nothing when nothing stands where it left | `desktop/src/shell/browserDoorsModel.test.mjs` |
| what the centre shows under a mode: the conversation and the Board only on a workstream with a project, documents otherwise | `desktop/src/views/_workbench/ideModeModel.test.mjs` |
| the tab a person looks at: the centre's, else the pane's, else none | `desktop/src/shell/browserPlacementModel.test.mjs` |
| a browser tab beside an Inbox row is at home in the goal, channel, message, conversation or workstream the row is | `desktop/src/views/_studio/inboxModel.test.mjs` |
| where a tab draws: the centre before the pane, offstage for a headless one — a dialog changing nothing — nowhere otherwise while a surface is open; a placement is as asked within a pixel and shown, else not | `desktop/src/shell/browserPlacementModel.test.mjs` |
| a screenshot's width, name and the size a PNG header says; the camera's refusal for a page that is not showing is the person's sentence, apart from the agent's, and a refused clipboard is said | `desktop/src/shell/browserShotModel.test.mjs` |
| every kit surface that portals at the `z-50` tier counts itself or is allowed with its reason — the tooltip, hover-driven and `pointer-events-none`; a toast never hides the page; the two menus own their open state and run the chosen item the one shared way | `desktop/src/ui/openSurfaces.test.mjs` |
| the policy's three words and sentences, scripts' two, the status card | `desktop/src/views/_settings/browserSettingsModel.test.mjs`, `settingsLink.test.mjs` |
| the Browser button's main click and menu — *New tab · From folder…*, nothing named after a branch, *Open :port* and *Stop* (the port named when several are up), the door and its chord on whichever item is it and on none when it is the Terminal's; ⌘⇧R's door — a server, the approved command, else the folder picker — and the Terminal caret's run item, none unset, held to its approval (`runCommandModel.test.mjs`), a goal's short list, what a start in flight holds — which server a page is on, that any page but an artifact's and a blank tab is annotatable, where the chips go, the toasts | `desktop/src/views/_workbench/serversModel.test.mjs` |
| the folder picker — the folders the paths name, Root first, files left out and a childless folder without a chevron, a failed listing in place, the filter's ranking, Root by its word, the path as typed offered only when it is valid and new, the field's arrows, a folder already served opening its server, the remembered folder capped and a garbage value reading as Root; and the whole walk from the caret to ⌘⇧R opening on the folder served last (`scenarios/serveFolder.test.mjs`) | `desktop/src/views/_workbench/serveFolderModel.test.mjs` |
| the bar is whole on every tab: a blank tab holds every verb but the address with its reason, a page frees them, back and forward follow the history, Stop while loading, the wand held with its reason, the IDE door the pane's alone | `desktop/src/shell/browserChromeModel.test.mjs` |
| what a request means for the tabs — a new tab at home where the engine said (any of the desktop's scopes), else the IDE's root, else the workspace, the engine's headless word riding into it; a read, a snapshot, a find, a click, a fill, typing, a key, a choice, a hover, a scroll, the console and a script on any page by ref or selector and none on a blank tab, the acts that may move the page marked, a wait for the load the bridge's own and bounded, the other waits the page's; back, forward and reload; a screenshot of any page, where a tab is shown — the strip at home while the centre shows documents, the pane otherwise — and that a headless one never is; the answers with every fact the page gave, a move said so | `desktop/src/shell/browserBridgeModel.test.mjs` |
| the policy's three words, everyone first; out of sight's three, unattended first; the status card counting the tabs kept out of sight | `desktop/src/views/_settings/browserSettingsModel.test.mjs` |
| the IDE's Browser menu marks a tab kept out of sight *unseen* with the hidden glyph, a pick showing it | `desktop/src/shell/browserDoorModel.test.mjs`, `views/_workbench/serversModel.test.mjs` |
| every catalog agent carries the Embedded Browser skill, the strips ride seen tabs only, Settings draws the out-of-sight and the scripts switches, an open desktop keeps the engine's presence fresh; the twenty-one tools are one list in the engine's note, the skill, the reference and the server, the bridge plans every act and a move answers the new page; the camera is two buttons and no screenshot is asked of a hidden page — the shot waits for the tab to show over the slots and the surfaces, the shell refuses a hidden view before WebKit is asked, the picture goes through the shell's clipboard, a refused placement is said, and every kit surface counts itself | `desktop/src/scenarios/browser.test.mjs` |
| the browser's program is the core over the shell's door on every page — no IPC, inert with no door — answering the driver and relaying the browser chords by ⌘ on a Mac and Ctrl elsewhere; a snapshot names every heading, landmark and control with a ref and the refs drive the other tools until the page loads again; typing is a keystroke at a time and Enter in a form's field submits it, a key is pressed on the target or the focused element, a fill is one act; an option is chosen by value or label, a hover moves the pointer, a scroll answers where the page stands; a wait answers at once when its condition holds and says what it waited for when it cannot; the console and the errors are kept and drained, a dialog is answered and reported once, a script's value comes back bounded; the parser bounds an answer's every field, a title and a chord | `desktop/src/ui/artifact/pageInspector.test.mjs` |
| the browser scope's six chords, narrowest like the editor's, the bracket keys pressed as themselves, ⌘W the browser tab's in a browser body, and the relay table read from the keymap — a rebinding riding into it | `desktop/src/shell/keymapModel.test.mjs` |
| an address typed into the palette is a row; a conversation's chips are kept under kind and id; the attach door names the message or says what to open | `desktop/src/shell/omniboxUrlModel.test.mjs`, `desktop/src/views/_studio/chatScopeModel.test.mjs` |
| both hosts draw the one bar over a body the chords are live in and draw no note box of their own; no capability, no IPC command, a blank tab on the blank page, the page anchored on the main webview's frame with a real read-back and the anchor logged, the viewport riding every placement, a stray placement logged; the Pulse's door, a link's first verb, the palette's row; a tab at home in the IDE follows its centre — the reveal rule reads the published centre, every door shows a tab through `showBrowserTab`, the Workbench derives the centre once and carries the tab on a switch of the same root, the hosts know nothing of the mode | `desktop/src/scenarios/browser.test.mjs` |
| the anchor is the identity when the page fills the content view, a top inset moves the page down by as much, a shorter lower main webview anchors the page to its own top, a zoom scales every edge, and a placement reads back as asked whatever the anchor; a tab's anchor is noted once per change; a hidden tab is refused a snapshot at once in its own words, apart from a late one's | `desktop/src-tauri/src/browser.rs` (`anchor::tests`, `tests`) |
| under either program — the frame's and the browser's — a pick opens the note box in the page with its crumbs, text and question, the caret in it; while it is open the pointer outlines nothing, a click outside is swallowed and a click inside is the box's own; Enter or Add says the note with its element and closes the box, Escape or Never mind closes it alone and says so, Escape with no box open leaves; a crumb re-picks keeping the words typed, an annotated element opens with its note and Change; the box sits above, below or beside the element and never on it, and follows a scroll; the tag rides outside the element and inside the right edge; the app's word closes it | `desktop/src/ui/artifact/pageInspector.test.mjs` |
| a browser tab id round-trips, rides the strip and is never stored; its menu, the annotation offered on a page an agent can edit; the run tab's identity; the surfaces count | `workbenchModel.test.mjs`, `tabMenuModel.test.mjs`, `terminalsModel.test.mjs`, `ui/surfacesModel.test.mjs` |
| annotations are about a page — a file's or a URL's — and the chips say which | `annotationModel.test.mjs`, `contextChips.test.mjs` |
