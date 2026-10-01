// The language first: this import runs before any other module says a word
// (17 — Internationalisation), so a table built at import time is in it.
import { bootLocale } from "./i18n/boot";
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { forgetApiBase, inDesktopShell, revealLog } from "./api";
import { errorFields, installGlobalLogHandlers, log } from "./log";
import { installEditMenu } from "./shell/editMenu";
import { installRouter } from "./router";
import { followWindowForMemories } from "./shell/viewMemoryStore";
import { ErrorBoundary } from "./ui/ErrorBoundary";
import "./styles.css";

// What nothing else catches — an uncaught error, an unhandled rejection —
// is one line in the desktop's log, before the first render.
installGlobalLogHandlers();

// The catalog was installed when `./i18n/boot` ran, before any other import;
// the call is the guard's word for it and does nothing twice.
bootLocale();

// The sidecar restarted the node — by its watchdog after a crash, or because
// a person asked. The base is resolved again on the next call, so a port
// that moved is never dialled from memory; the lists are read again by the
// shell when the bus comes back (`useWorkspaceData`).
if (inDesktopShell()) {
  void import("@tauri-apps/api/event").then((ev) =>
    ev.listen<{ port: number; attempt: number; requested: boolean }>("node:restarted", (e) => {
      forgetApiBase();
      log.warn("shell", e.payload.requested ? "the node was restarted" : "the node restarted after exiting on its own", { port: e.payload.port, attempt: e.payload.attempt });
    }),
  );
}

// The native Edit menu's verbs, replayed as the chords the keymap knows (ide/15).
installEditMenu();

// Where the person was, before the first render: a launch opens on the place
// the app closed on, and every address from here on is resolved against the
// memory. What every screen keeps is written when the window is put away.
installRouter();
followWindowForMemories();

// The shell's own boundary: the screen has one of its own (`App.tsx`), but
// the chrome, the sidebar and the overlays render above it, and one throw
// there used to blank the window with the error only in the console.
ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <ErrorBoundary
      onError={(error, info) => log.error("shell", "the shell crashed", { ...errorFields(error), component_stack: info.componentStack })}
      onReveal={() => void revealLog().catch((e: unknown) => log.warn("shell", "the log could not be revealed", errorFields(e)))}
    >
      <App />
    </ErrorBoundary>
  </React.StrictMode>,
);
