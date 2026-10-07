// The language first: this import runs before any other module says a word
// (17 — Internationalisation), so a table built at import time is in it.
import { bootLocale } from "./i18n/boot";
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { errorFields, installGlobalLogHandlers, log } from "./log";
import { installEditMenu } from "./shell/editMenu";
import { installRouter, useAddress } from "./router";
import { installNodeBootStore } from "./shell/nodeBootStore";
import { RootCrashCard } from "./shell/RootCrashCard";
import { installCloseGuard } from "./shell/useCloseGuard";
import { followWindowForMemories } from "./shell/viewMemoryStore";
import { ErrorBoundary } from "./ui/ErrorBoundary";
import "./styles.css";

// What nothing else catches — an uncaught error, an unhandled rejection —
// is one line in the desktop's log, before the first render.
installGlobalLogHandlers();

// The catalog was installed when `./i18n/boot` ran, before any other import;
// the call is the guard's word for it and does nothing twice.
bootLocale();

// What the shell says of its node — booting, failed, restarted, ready —
// kept for every reader above and below the boundary (`nodeBootStore`): the
// sidebar's footer, Settings › Node and the root crash card say the same
// line, and a restart drops the cached API base so a port that moved is
// never dialled from memory.
installNodeBootStore();

// The native Edit menu's verbs, replayed as the chords the keymap knows (ide/15).
installEditMenu();

// Where the person was, before the first render: a launch opens on the place
// the app closed on, and every address from here on is resolved against the
// memory. What every screen keeps is written when the window is put away.
installRouter();
followWindowForMemories();

// The ways out — the window's red button, ⌘Q, the menu bar's Quit, Ctrl+Q
// — stand here, above the root boundary, for the life of the page: a throw
// in the tree below must never take the listeners the shell is waiting on
// (`useCloseGuard.ts`).
installCloseGuard();

/**
 * The shell's own boundary: the screen has one of its own (`App.tsx`), but
 * the chrome, the sidebar and the overlays render above it, and one throw
 * there used to blank the window with the error only in the console — and
 * leave it there: the card never reset, and nothing on it worked without
 * the node. Now the card is `RootCrashCard` — six doors, none of them
 * needing the node — and the boundary resets on the address, so moving to
 * another screen is a way out too.
 */
function Root() {
  const address = useAddress();
  return (
    <ErrorBoundary
      resetKey={address}
      onError={(error, info) => log.error("shell", "the shell crashed", { ...errorFields(error), address, component_stack: info.componentStack })}
      fallback={(error, reset) => <RootCrashCard error={error} reset={reset} />}
    >
      <App />
    </ErrorBoundary>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
