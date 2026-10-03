/**
 * The shell: top chrome, a sidebar that never unmounts, the routed screen,
 * and one auxiliary pane on the right.
 *
 * The shell before this one was a flat list of nav items with a full-page
 * screen behind each — you had to choose a *section* before you could choose
 * a *thing*, and unread only existed on the screen that owned it. Here the
 * workspace is always on screen and a screen is always a place inside it.
 * There are seven destinations, and they sit above the channels and the
 * direct messages rather than instead of them; no two of them are the same
 * door. A conversation with the agents (13) has no door of its own here: it
 * is reached where it is about, and `#/conversations/:id` only finds it.
 *
 * The screen is `lazy`, the shell is not, and neither is the workspace store:
 * that is what makes moving between two screens free rather than a spinner
 * and a rebuilt list.
 */

import { pushShellWords } from "./shell/shellWords";
import { readPref, webStorage, writePref } from "./shell/storedPrefModel.mjs";
import { LAYOUT_CHANGED, notifyLayoutChanged } from "./shell/layerSlots";
import { publishContentBox } from "./shell/contentBox";
import { useCloseGuard } from "./shell/useCloseGuard";
import { LinkProvider } from "./shell/linkHandler";
import { useNotifications } from "./shell/notifications";
import { useNotifySettings } from "./shell/notifySettings";
import { joinCodeFromUrls } from "./views/_settings/inviteCodeModel.mjs";
import { settingsSearch } from "./views/_settings/settingsLink.mjs";
import { Suspense, lazy, useCallback, useEffect, useRef, useState } from "react";
import { type ConnState, useEngineEvents, watchConnection } from "./bus";
import { FIRST_CONN } from "./busModel.mjs";
import { useReloadOnReconnect } from "./ui/useReloadOnReconnect";
import { navigate, useRoute, type Route } from "./router";
import { DragProvider, ErrorBoundary, OverlayBoundary, ResizeHandle, Spinner, ToastProvider, useDockClearance, useStoredSize } from "./ui";
import { AuxPane, useAux } from "./shell/AuxPane";
import { NoteOverlay } from "./notes/NoteOverlay";
import { DrawOverlay } from "./draw/DrawOverlay";
import { DrawPanel } from "./draw/DrawPanel";
import { PetCompanion } from "./pet/PetCompanion";
import { AddonLayer } from "./addons/AddonLayer";
import { ProjectGitDialog } from "./views/_work/ProjectGitDialog";
import { HookSecretsDialog } from "./views/_workflow/HookSecretsDialog";
import { WorkstreamScriptToasts } from "./views/_work/WorkstreamScriptToasts";
import { SecurityToasts } from "./views/_work/SecurityToasts";
import { CloseConfirmDialog } from "./shell/CloseConfirmDialog";
import { useCloseGuardSettings } from "./shell/closeGuardSettings";
import { useTrayPrefs } from "./shell/trayPrefs";
import { useTray } from "./shell/useTray";
import { Omnibox, type OmniboxMode } from "./shell/Omnibox";
import { TerminalPanel } from "./shell/TerminalPanel";
import { BrowserPanel } from "./shell/BrowserPanel";
import { Sidebar } from "./shell/Sidebar";
import { RAIL_WIDTH, SIDEBAR_MODE_KEY, readMode, toggledMode } from "./shell/sidebarModel.mjs";
import type { SidebarMode } from "./shell/sidebarModel.mjs";
import { TopChrome } from "./shell/TopChrome";
import { StatusBar } from "./shell/StatusBar";
import { applyTheme, syncAppearanceFromNode } from "./shell/theme";
import { useLogSettings } from "./shell/logSettings";
import { errorFields, log } from "./log";
import { inDesktopShell, revealLog } from "./api";
import { SetupGate } from "./shell/SetupGate";
import { syncCacheTuningFromNode } from "./shell/cacheTuning";
import { syncLocaleFromNode } from "./i18n/localeStore";
import { OPEN_PLACES, useGlobalShortcuts } from "./shell/shortcuts";
import { WorkspaceContext, useWorkspaceState } from "./shell/useWorkspaceData";
import { gonePaths, leftHosts, membershipsKnown } from "./shell/gonePlacesModel.mjs";
import { rootOfPath } from "./views/_workbench/idePlacesModel.mjs";
import { forgetRootMemory } from "./views/_workbench/workbenchStore";
import { isMember } from "./shell/hostedModel.mjs";
import { placesNow } from "./shell/placeMemoryStore";
import { adoptMemoryOwner, forgetPlaceMemory } from "./shell/viewMemoryStore";
import { t as tr } from "./i18n/l10n.mjs";

/**
 * The routed screens' modules, one table: what `lazy` mounts and what
 * `preloadScreens` fetches at idle after the first paint, so every chunk is
 * in memory before a person reaches for it and navigation after boot never
 * suspends. Monaco keeps its own dynamic import inside the editor.
 */
const SCREENS = {
  pulse: () => import("./views/Pulse"),
  inbox: () => import("./views/Inbox"),
  goals: () => import("./views/Goals"),
  goalDetail: () => import("./views/GoalDetail"),
  workflows: () => import("./views/Workflows"),
  workflowDesigner: () => import("./views/WorkflowDesigner"),
  workflowRun: () => import("./views/WorkflowRun"),
  workbench: () => import("./views/Workbench"),
  channels: () => import("./views/Channels"),
  messages: () => import("./views/Messages"),
  hosted: () => import("./views/Hosted"),
  conversation: () => import("./views/ConversationDoor"),
  agents: () => import("./views/Agents"),
  teams: () => import("./views/Teams"),
  settings: () => import("./views/Settings"),
};

const Pulse = lazy(SCREENS.pulse);
const Inbox = lazy(SCREENS.inbox);
const Goals = lazy(SCREENS.goals);
const GoalDetail = lazy(SCREENS.goalDetail);
const Workflows = lazy(SCREENS.workflows);
const WorkflowDesigner = lazy(SCREENS.workflowDesigner);
const WorkflowRun = lazy(SCREENS.workflowRun);
const ProjectsHome = lazy(() => SCREENS.workbench().then((m) => ({ default: m.ProjectsHome })));
const Workbench = lazy(SCREENS.workbench);
const Channels = lazy(SCREENS.channels);
const Messages = lazy(SCREENS.messages);
const Hosted = lazy(SCREENS.hosted);
const ConversationDoor = lazy(SCREENS.conversation);
const Agents = lazy(SCREENS.agents);
const Teams = lazy(SCREENS.teams);
const Settings = lazy(SCREENS.settings);

/** Fetch every screen's chunk; a chunk that fails to load is the route's own error when it is reached. */
function preloadScreens(): void {
  for (const load of Object.values(SCREENS)) void load().catch((e: unknown) => log.debug("app", "a screen's code did not preload", errorFields(e)));
}

/** After the first paint, when the window is idle — WebKit has no `requestIdleCallback`, so a short timer stands in. */
function whenIdle(cb: () => void): () => void {
  // Typed as optional on purpose: the DOM library says every window has it, WebKit does not.
  const idle = (window as { requestIdleCallback?: (cb: () => void) => number; cancelIdleCallback?: (handle: number) => void }).requestIdleCallback;
  if (idle) {
    const handle = idle.call(window, cb);
    return () => (window as { cancelIdleCallback?: (handle: number) => void }).cancelIdleCallback?.(handle);
  }
  const t = window.setTimeout(cb, 1_500);
  return () => window.clearTimeout(t);
}

const SIDEBAR_KEY = "bisa.sidebar.width";
const SIDEBAR_DEFAULT = 280;
const SIDEBAR_MIN = 220;
const SIDEBAR_MAX = 420;
/** Below this, the aux pane takes the whole content area instead of splitting it. */
const SINGLE_COLUMN_AT = 600;

function Screen({ route }: { route: Route }) {
  switch (route.name) {
    case "pulse":
      return <Pulse />;
    case "inbox":
      return <Inbox />;
    case "goals":
      return <Goals />;
    // Keyed by the route, as the designer is: another goal is another page,
    // read from its own memory rather than drawn over the last one's state.
    case "goal":
      return <GoalDetail key={route.id} id={route.id} />;
    case "workflows":
      return <Workflows />;
    // Keyed by the route: another workflow is another designer — the old one
    // unmounts and flushes its edits rather than being reused under a
    // different address with its session discarded.
    case "workflow":
      return <WorkflowDesigner key={route.id} id={route.id} />;
    case "run":
      return <WorkflowRun key={route.id} id={route.id} />;
    case "projects":
      return <ProjectsHome />;
    case "workbench":
      return <Workbench scope={route.scope} id={route.id} />;
    case "channels":
    case "channel":
      return <Channels id={route.name === "channel" ? route.id : undefined} />;
    case "messages":
    case "dm":
      return <Messages id={route.name === "dm" ? route.id : undefined} />;
    case "hosted_channel":
      return <Hosted key={`${route.host}/${route.id}`} host={route.host} id={route.id} kind="channel" />;
    case "hosted_dm":
      return <Hosted key={`${route.host}/${route.id}`} host={route.host} id={route.id} kind="dm" />;
    case "conversation":
      return <ConversationDoor id={route.id} />;
    case "agents":
    case "agent":
      return <Agents id={route.name === "agent" ? route.id : undefined} />;
    case "teams":
      return <Teams />;
    case "settings":
      return <Settings />;
  }
}


export default function App() {
  // Every screen's chunk, fetched once the first paint is behind us.
  useEffect(() => whenIdle(preloadScreens), []);
  const route = useRoute();
  const workspace = useWorkspaceState();
  const aux = useAux();

  const [sidebarWidth, setSidebarWidth] = useStoredSize(SIDEBAR_KEY, SIDEBAR_DEFAULT, { min: SIDEBAR_MIN, max: SIDEBAR_MAX });
  // Expanded, or collapsed to its rail of icons — never gone: the Inbox count stays in sight.
  const [sidebarMode, setSidebarMode] = useState<SidebarMode>(() => readPref(webStorage(), SIDEBAR_MODE_KEY, readMode, "expanded"));
  // The sidebar moves the workbench's centre without resizing the window.
  useEffect(() => notifyLayoutChanged(), [sidebarWidth, sidebarMode]);
  const [omnibox, setOmnibox] = useState<OmniboxMode | null>(null);
  // Before the bus has said anything the node is not known to be away (`busModel.FIRST_CONN`).
  const [conn, setConn] = useState<ConnState>(FIRST_CONN);
  // The room right of the sidebar, measured below: the one-column rule and
  // the Details pane's ceiling both read it. Zero until measured.
  const [contentWidth, setContentWidth] = useState(0);
  const singleColumn = contentWidth > 0 && contentWidth < SINGLE_COLUMN_AT;
  const content = useRef<HTMLDivElement>(null);
  // The room a floating dock over the content takes at the end of each screen's scroll (`ui/dockClearance.ts`).
  useDockClearance(content);

  const toggleSidebar = useCallback(() => {
    setSidebarMode((mode) => {
      const next = toggledMode(mode);
      writePref(webStorage(), SIDEBAR_MODE_KEY, next);
      return next;
    });
  }, []);

  useEffect(() => {
    applyTheme();
    void syncAppearanceFromNode();
    // The language is an appearance dial too (`appearance.language`).
    void syncLocaleFromNode();
    // The shell's own menus take their words from the catalog, once.
    void pushShellWords();
    // The desktop's own cache knobs follow the settings too.
    void syncCacheTuningFromNode();
  }, []);
  // The `logging.*` settings, for the webview's own lines and the shell's file.
  useLogSettings();
  // The three confirmation switches, followed for the close guards.
  useCloseGuardSettings();
  useNotifySettings();
  // The two menu bar switches — hide on close, the Dock icon — followed too.
  useTrayPrefs();
  // The six dials are `appearance.*` settings; a change from a second window,
  // the CLI or the raw settings panel reaches this one without a reload. The
  // `cache.desktop.*` knobs ride the same signal.
  useEngineEvents((e) => {
    if (e.payload.type === "settings_changed") {
      void syncAppearanceFromNode();
      void syncLocaleFromNode();
      void syncCacheTuningFromNode();
    }
    // What is gone is forgotten: a remembered place never leads to a dead
    // page (`gonePlacesModel`). What went while the app was closed raises no
    // fact; the screen that opens on it leaves on its own (`useGonePlace`).
    for (const path of gonePaths(e.payload)) {
      forgetPlaceMemory(path);
      // A root of the Project IDE keeps its view under its key, not its path.
      const root = rootOfPath(path);
      if (root) forgetRootMemory(root);
    }
  });
  // A node that was not there at boot answered none of the three: the dials,
  // the language and the cache knobs are read once it is.
  useReloadOnReconnect(() => {
    void syncAppearanceFromNode();
    void syncLocaleFromNode();
    void syncCacheTuningFromNode();
  });
  // The memory is one workspace's: another's is forgotten whole, once the
  // node has said whose this is.
  useEffect(() => {
    if (workspace.ready && workspace.me) adoptMemoryOwner(workspace.me);
  }, [workspace.ready, workspace.me]);
  // A hosted workspace left, or one the person was removed from, gives up its places.
  // Only on a list the node answered: a read that failed says nothing of who left.
  const hostsKnown = membershipsKnown(workspace);
  useEffect(() => {
    if (!hostsKnown) return;
    const hosts = workspace.hosted.filter((s) => isMember(s.host)).map((s) => s.host.host.pubkey);
    const paths = placesNow().queries.map(([path]) => path);
    for (const path of leftHosts(paths, hosts)) forgetPlaceMemory(path);
  }, [hostsKnown, workspace.hosted]);
  // The workbench header's switch button, which is the same palette as ⌘P.
  useEffect(() => {
    const open = () => setOmnibox("places");
    window.addEventListener(OPEN_PLACES, open);
    return () => window.removeEventListener(OPEN_PLACES, open);
  }, []);
  useEffect(() => watchConnection(setConn), []);
  // An invitation link (`bisa://join/…`) the OS opened the app with, or
  // handed it while running: Settings › People, the code filled in
  // (14-collaboration). Only the desktop shell registers the scheme.
  useEffect(() => {
    if (!inDesktopShell()) return;
    let off: (() => void) | null = null;
    let gone = false;
    const land = (urls: string[] | null) => {
      const code = joinCodeFromUrls(urls);
      if (code) navigate({ name: "settings" }, settingsSearch("people", { join: code }));
    };
    void import("@tauri-apps/plugin-deep-link").then(async (m) => {
      if (gone) return;
      land(await m.getCurrent().catch(() => null));
      const un = await m.onOpenUrl(land).catch(() => null);
      if (gone) un?.();
      else off = un;
    });
    return () => {
      gone = true;
      off?.();
    };
  }, []);

  useGlobalShortcuts({
    onOmnibox: (mode) => setOmnibox(mode),
    onEscape: () => {
      // Inside a terminal, Escape is a keystroke somebody's editor is waiting
      // for — closing a pane with it would make vim unusable in the drawer.
      if (document.activeElement?.closest("[data-terminal-panel]")) return;
      if (omnibox) setOmnibox(null);
      else if (aux.kind) aux.close();
    },
  });

  // Narrow content: the aux pane replaces the main pane rather than crushing
  // it. The same measure publishes the content column's box for a panel that
  // maximizes into it (19 — Drawings, `shell/contentBox.ts`) — re-read on a
  // window resize and a layout change too, since a sidebar toggle moves the
  // column without resizing the window.
  useEffect(() => {
    const el = content.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const publish = () => publishContentBox(el.getBoundingClientRect());
    const ro = new ResizeObserver(([entry]) => {
      setContentWidth(entry?.contentRect.width ?? 0);
      publish();
    });
    ro.observe(el);
    const onLayout = () => requestAnimationFrame(publish);
    window.addEventListener("resize", onLayout);
    window.addEventListener(LAYOUT_CHANGED, onLayout);
    el.addEventListener("transitionend", onLayout);
    return () => {
      ro.disconnect();
      window.removeEventListener("resize", onLayout);
      window.removeEventListener(LAYOUT_CHANGED, onLayout);
      el.removeEventListener("transitionend", onLayout);
    };
  }, []);

  // The shell's one notification duty — decided in `notificationsModel.mjs`.
  useNotifications();
  // The menu bar icon and the Dock badge: what the platform is doing, how many things need you.
  useTray(conn);
  // The red button hides the window or quits; ⌘Q and the menu bar's Quit ask, save, quit.
  useCloseGuard();

  const auxOpen = aux.kind !== null;
  // The route as one key: the screen's boundary resets on it, and so does
  // every host outside the screen — a closed dialog comes back on the next
  // screen, never on the one that closed it.
  const routeKey = `${route.name}:${"scope" in route ? route.scope : ""}:${"id" in route ? route.id : ""}`;
  const crashed = (what: string) => (error: Error, info: { componentStack?: string | null }) =>
    log.error("shell", `${what} crashed`, { ...errorFields(error), route: route.name, component_stack: info.componentStack });

  return (
    <ToastProvider>
      {/*
        Every host mounted outside the routed screen has a boundary of its
        own (`OverlayBoundary`): the screen's boundary cannot reach them, and
        the root one in `main.tsx` is the whole window. A throw in one of
        these costs that host, not the app.
      */}
      <OverlayBoundary name="script notices" resetKey={routeKey} onError={crashed(tr("app-app-workstream-script-toasts"))}>
        <WorkstreamScriptToasts />
      </OverlayBoundary>
      <OverlayBoundary name="security notices" resetKey={routeKey} onError={crashed(tr("app-app-security-toasts"))}>
        <SecurityToasts />
      </OverlayBoundary>
      <WorkspaceContext.Provider value={workspace}>
        {/* The one drag world (`ui/dnd`): a file leaves the explorer for the
            agent pane, a tab leaves one pane for another, a rail row moves —
            every source and target below shares this provider. */}
        <DragProvider>
        {/* The door every path and link opens through (`shell/linkHandler.tsx`, ide/17). */}
        <LinkProvider>
        <div className="flex h-full flex-col">
          <ErrorBoundary resetKey={routeKey} onError={crashed(tr("app-app-top-chrome"))} fallback={() => <ChromeFell what={tr("app-app-chrome-header")} />}>
            <TopChrome
              sidebarMode={sidebarMode}
              onToggleSidebar={toggleSidebar}
              onOmnibox={() => setOmnibox("all")}
            />
          </ErrorBoundary>
          <div className="flex min-h-0 flex-1">
            <div
              className="min-h-0 shrink-0 border-r border-border"
              style={{ width: sidebarMode === "collapsed" ? RAIL_WIDTH : sidebarWidth }}
            >
              <ErrorBoundary resetKey={routeKey} onError={crashed(tr("app-app-sidebar"))} fallback={() => <ChromeFell what={tr("app-app-chrome-sidebar")} />}>
                <Sidebar mode={sidebarMode} />
              </ErrorBoundary>
            </div>
            {sidebarMode === "expanded" && (
              <ResizeHandle
                side="right"
                size={sidebarWidth}
                min={SIDEBAR_MIN}
                max={SIDEBAR_MAX}
                defaultSize={SIDEBAR_DEFAULT}
                onSize={setSidebarWidth}
                label={tr("app-app-resize-sidebar")}
              />
            )}

            {/*
              The terminal layer is mounted here, outside the routed screen,
              `Suspense` and the `ErrorBoundary`, and *drawn* over the
              host it stands in (`shell/layerSlots.ts`). A shell outlives the
              surface that opened it (see `TerminalPanel.tsx`), so it cannot
              hang off a route: mounting it inside the workbench would end
              every running build the moment somebody opened the Inbox.
            */}
            <div ref={content} className="flex min-w-0 flex-1 flex-col">
              <div className="flex min-h-0 flex-1">
                {/* One column: the pane takes the width and the screen is
                    hidden, never unmounted — a screen that fills the pane
                    through `AuxPortal` (the goal's Details) must stay mounted
                    to fill it. */}
                {(
                  <main className="min-w-0 flex-1 overflow-hidden" hidden={singleColumn && auxOpen}>
                    <ErrorBoundary
                      resetKey={routeKey}
                      onError={(error, info) =>
                        log.error("view", "a screen crashed", {
                          ...errorFields(error),
                          route: route.name,
                          component_stack: info.componentStack,
                        })
                      }
                      onReveal={() => void revealLog().catch((e: unknown) => log.warn("shell", "the log could not be revealed", errorFields(e)))}
                    >
                      {/* A chunk still on its way: nothing for the beat the kit's
                          indicators wait, then one line in place — never an overlay. */}
                      <Suspense
                        fallback={
                          <div className="p-6">
                            <Spinner label={tr("app-app-opening")} />
                          </div>
                        }
                      >
                        <Screen route={route} />
                      </Suspense>
                    </ErrorBoundary>
                  </main>
                )}
                <ErrorBoundary resetKey={routeKey} onError={crashed(tr("app-app-details-pane"))} fallback={() => <ChromeFell what={tr("app-app-details-pane-2")} />}>
                  <AuxPane singleColumn={singleColumn} available={contentWidth} />
                </ErrorBoundary>
              </div>
              {/* Its own boundary: a throw during a burst of closes costs the panel, never the window. */}
              <ErrorBoundary onError={(error, info) => log.error("terminal", "the terminal panel crashed", { ...errorFields(error), component_stack: info.componentStack })}>
                <TerminalPanel />
              </ErrorBoundary>
              {/* The browser layer (ide/18): the tabs' native webviews, over the same slot. */}
              <ErrorBoundary resetKey={routeKey} onError={(error, info) => log.error("browser", "the browser panel crashed", { ...errorFields(error), component_stack: info.componentStack })}>
                <BrowserPanel />
              </ErrorBoundary>
              {/* The drawing bridge (19 — Drawings): the agents' requests performed, the presence read, the offscreen canvas. */}
              <ErrorBoundary resetKey={routeKey} onError={(error, info) => log.error("draw", "the draw panel crashed", { ...errorFields(error), component_stack: info.componentStack })}>
                <DrawPanel />
              </ErrorBoundary>
            </div>
          </div>
          <ErrorBoundary resetKey={routeKey} onError={crashed(tr("app-app-status-bar"))} fallback={() => <ChromeFell what={tr("app-app-chrome-footer")} />}>
            <StatusBar conn={conn} />
          </ErrorBoundary>
        </div>
        <OverlayBoundary name="palette" resetKey={`${routeKey}:${omnibox ?? ""}`} onError={crashed(tr("app-app-palette"))}>
          <Omnibox
            open={omnibox !== null}
            mode={omnibox ?? "all"}
            onClose={() => setOmnibox(null)}
          />
        </OverlayBoundary>
        {/*
          Outside `<Screen>`, `Suspense` and the error boundary, for the reason
          the terminal panel is: a note you are half-way through writing must
          outlive the screen you opened it on, and a screen that throws must
          not take unsaved text with it. It portals to the body at z-40, under
          every dialog and menu.
        */}
        <OverlayBoundary name="notes panel" resetKey={routeKey} onError={crashed(tr("app-app-notes-panel"))}>
          <NoteOverlay />
        </OverlayBoundary>
        {/*
          The Draw panel (19 — Drawings): the same placement and tier as the
          notes panel — a drawing you are half-way through must outlive the
          screen you opened it on, and maximized it stands over the content
          column, under every dialog.
        */}
        <OverlayBoundary name="draw panel" resetKey={routeKey} onError={crashed(tr("app-app-draw-panel"))}>
          <DrawOverlay />
        </OverlayBoundary>
        {/*
          Same placement, one tier lower: a companion that vanished when a
          screen threw would be missing at the one moment worth reporting on,
          and it must never draw over the notes panel or a dialog.
        */}
        <OverlayBoundary name="companion" resetKey={routeKey} onError={crashed(tr("app-app-companion"))}>
          <PetCompanion />
        </OverlayBoundary>
        {/*
          The addons (18 — Addons): the same tier as the pet, so a window never
          covers the notes panel or a dialog, and a screen that throws leaves
          every widget standing.
        */}
        <OverlayBoundary name="addons" resetKey={routeKey} onError={crashed(tr("app-app-addons"))}>
          <AddonLayer />
        </OverlayBoundary>
        {/* Who commits in a repository nobody is set to commit in — one dialog for every creation path. */}
        <OverlayBoundary name="who-commits dialog" resetKey={routeKey} onError={crashed(tr("app-app-who-commits-dialog"))}>
          <ProjectGitDialog />
        </OverlayBoundary>
        {/* A public hook's secret an adoption minted, shown once — after the card that decided it has left. */}
        <OverlayBoundary name="hook secrets" resetKey={routeKey} onError={crashed(tr("app-app-hook-secrets"))}>
          <HookSecretsDialog />
        </OverlayBoundary>
        {/* The one confirmation before something ends: a shell, a harness, the window, the app. */}
        <OverlayBoundary name="close confirmation" resetKey={routeKey} onError={crashed(tr("app-app-close-confirmation"))}>
          <CloseConfirmDialog />
        </OverlayBoundary>
        {/* What the platform needs before it can work (16 — The setup gate): a modal over every
            screen but Settings and Agents, a banner there, nothing once every check is ready. */}
        <OverlayBoundary name="setup gate" resetKey={routeKey} onError={crashed(tr("app-app-setup-gate"))}>
          <SetupGate screen={route.name} />
        </OverlayBoundary>
        </LinkProvider>
        </DragProvider>
      </WorkspaceContext.Provider>
    </ToastProvider>
  );
}

/** A piece of chrome that threw: one quiet line in its place, the rest of the window untouched. */
function ChromeFell({ what }: { what: string }) {
  return (
    <div className="flex h-full items-center justify-center p-3 text-2xs text-text-dim" role="status">{tr("app-app-hit-error-move-another-screen-bring", { what })}</div>
  );
}
