/**
 * The app's ways out (ide/03, ide/13), held by the shell and decided here.
 * **The window's red button** arrives as `bisa:close-requested`: while
 * *Closing the window keeps Bisa running* is on the app is put away
 * (`hide_window` — the shell decides how: on macOS the app hides as ⌘H
 * does, so ⌘Tab, the Dock and the icon all bring it back) — nothing asked,
 * nothing saved, the webview lives on in the menu bar (`useTray.ts`) — and
 * off, it is a quit. **⌘Q, the Dock's Quit, a logout and the menu bar's
 * *Quit Bisa*** arrive as `bisa:quit-requested` — on macOS the shell holds
 * AppKit's `terminate:` for the answer (`quit.rs`) — and **Ctrl+Q off
 * macOS** through the keymap's door (`shortcuts.QUIT_APP`). A quit is one
 * flow: ask first when `desktop.confirm_quit` is on, naming what is running
 * and what is unsaved; then save every dirty document; then keep what is
 * remembered — where the person was, how each screen stood
 * (`viewMemoryStore.settleMemories`) — then `quit_app`, which is the yes a
 * held `terminate:` was waiting on. A no — a cancelled quit, a document that
 * would not save, a flow that failed — is told to the shell as
 * `quit_declined`, so a logout waiting on it hears it too; and once the
 * listeners stand the shell is told so (`quit_ready`), since before that a
 * quit the OS asks for has nobody to ask through and stands. The question
 * comes **before** the save: a cancelled quit must not have written files.
 * A save that fails keeps the window, with an OS notice; a memory that
 * cannot be kept never does. The window put away writes its memories too:
 * a hidden app may be ended from the menu bar. A browser dev session keeps
 * `beforeunload`'s question; there is no other hook there.
 *
 * **Installed once, from `main.tsx`, above the root boundary** — not a hook
 * of `App`'s. A throw in the shell's own tree unmounts `App`; a hook there
 * took the listeners with it, and the shell, still holding every close for
 * the webview's answer, had nobody to hand it to: the red button did
 * nothing, ⌘Q hung, only a Force Quit ended Bisa. The listeners now outlive
 * the tree, and while the root has crashed the flow is bare
 * (`closeFlowModel`, `rootCrashModel`): nothing asked, nothing saved, the
 * way out taken. **Every request is acknowledged first** (`quit_heard`):
 * the shell gives the webview five seconds to say it heard before the exit
 * stands without it (`quit.rs` `ACK_DEADLINE`) — a webview that is gone
 * altogether must not hold the app either.
 */
import { errorFields, log } from "../log";
import { anyDirty, dirtyCount, flushAll } from "../views/_workbench/editorRegistry";
import { closeFlow, declines } from "./closeFlowModel.mjs";
import { deliverAppNotice } from "./notifications";
import { quitQuestion } from "./closeGuardModel.mjs";
import { confirmPrefs } from "./closeGuardSettings";
import { rootCrash } from "./rootCrashModel.mjs";
import { QUIT_APP, onDoor } from "./shortcuts";
import { askClose } from "./terminalCloseGuard";
import { harnessOf, isLive } from "./terminalsModel.mjs";
import { TRAY_EVENTS, closeVerb } from "./trayModel.mjs";
import { trayPrefs } from "./trayPrefs";
import { terminalSessions } from "./useTerminals";
import { flushMemories, settleMemories } from "./viewMemoryStore";
import { t } from "../i18n/l10n.mjs";

/** Which door the request came through — for the log line alone; the flow is one. `footer` is the sidebar's *Quit* beside a node that is away; `crash` the root crash card's. */
export type Door = "window" | "quit" | "keyboard" | "footer" | "crash";

/** The shell's words back about a held request (`quit.rs`): that the webview listens, that it heard, and a no. */
async function tellShell(command: "quit_ready" | "quit_heard" | "quit_declined"): Promise<void> {
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke(command);
}

/** The request was heard: the shell's deadline on it is off, and the flow has it. */
function heard(): void {
  tellShell("quit_heard").catch((e: unknown) => log.warn("close", "the shell did not hear that the request was heard; its deadline stands", errorFields(e)));
}

/** The OS asked and is told no — nothing held is nothing, and the shell knows which. */
function decline(): void {
  tellShell("quit_declined").catch((e: unknown) => log.warn("close", "the shell did not hear the no", errorFields(e)));
}

/** The one word this hook has when a save fails: an OS notice through the shell's one door, since the window was about to go. */
async function say(body: string): Promise<void> {
  try {
    await deliverAppNotice(t("shell-use-close-guard-bisa-still-open"), body);
  } catch (e) {
    // Nothing to say it with; the window staying open is the message — and the log has the words.
    log.warn("close", "a save failed on close and the notice could not be shown", { body, ...errorFields(e) });
  }
}

/** The flow's order is the model's (`closeFlowModel`); these are the window's hands. */
const flow = closeFlow({
  confirms: () => confirmPrefs().quit,
  ask: () => {
    const live = terminalSessions().filter(isLive);
    return askClose(quitQuestion({ shells: live.filter((s) => !harnessOf(s)).length, harnesses: live.filter((s) => harnessOf(s)).length, dirty: dirtyCount() }));
  },
  dirty: anyDirty,
  save: flushAll,
  unsaved: () => void say(t("shell-use-close-guard-document-did-save-stays-open-save")),
  keep: settleMemories,
  keepFailed: (e) => log.warn("close", "what was remembered could not be kept; the way out goes on", errorFields(e)),
  // The root crashed: no dialog to ask in, no editor to save.
  bare: () => rootCrash.crashed(),
});

/** The programmatic exit the shell lets through (`quit_app`). */
async function quit(): Promise<void> {
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("quit_app");
}

/**
 * Run the flow; a way out that throws is logged — the window staying open is
 * what the person sees — and, like a no, told to the shell (`declines`).
 */
function leave(door: Door): void {
  flow.run(quit).then(
    (outcome) => {
      log.info("close", "the way out ended", { door, outcome });
      if (declines(outcome)) decline();
    },
    (e: unknown) => {
      log.error("close", "the way out failed", { door, ...errorFields(e) });
      decline();
    },
  );
}

/** The app put away, still running; the shell's `hide_window` knows how on this OS. */
function hide(): void {
  flushMemories();
  void import("@tauri-apps/api/core")
    .then(({ invoke }) => invoke("hide_window"))
    .then(
      () => log.info("close", "the window hid; Bisa runs on in the menu bar"),
      (e: unknown) => log.error("close", "the window did not hide", errorFields(e)),
    );
}

/**
 * A quit asked for from inside the page — the sidebar's door beside a node
 * that is away, the root crash card's — through the one flow.
 */
export function requestQuit(door: Door): void {
  leave(door);
}

/**
 * Stand at the three doors for the life of the page. Returns the way to
 * leave them, for a test; the page itself never does.
 */
export function installCloseGuard(): () => void {
  if (!("__TAURI_INTERNALS__" in window)) return () => {};
  let off: (() => void)[] = [];
  let gone = false;
  void import("@tauri-apps/api/event").then(async (ev) => {
    const close = await ev.listen(TRAY_EVENTS.close, () => {
      heard();
      if (closeVerb(trayPrefs()) === "hide") hide();
      else leave("window");
    });
    const quitRequested = await ev.listen(TRAY_EVENTS.quit, () => {
      heard();
      leave("quit");
    });
    const keyboard = onDoor(QUIT_APP, () => leave("keyboard"));
    if (gone) {
      close();
      quitRequested();
      keyboard();
      return;
    }
    off = [close, quitRequested, keyboard];
    // Listening now: a quit the OS asks for can be held for the question.
    tellShell("quit_ready").catch((e: unknown) => log.warn("close", "the shell was not told the webview listens; a quit the OS asks for stands", errorFields(e)));
  });
  return () => {
    gone = true;
    for (const f of off) f();
  };
}
