/**
 * The app's ways out (ide/03, ide/13), both held by the shell and decided
 * here. **The window's red button** arrives as `bisa:close-requested`: while
 * *Closing the window keeps Bisa running* is on the app is put away
 * (`hide_window` — the shell decides how: on macOS the app hides as ⌘H
 * does, so ⌘Tab, the Dock and the icon all bring it back) — nothing asked,
 * nothing saved, the webview lives on in the menu bar (`useTray.ts`) — and
 * off, it is a quit. **⌘Q and the menu bar's *Quit Bisa*** arrive as
 * `bisa:quit-requested`. A quit is one flow: ask first when
 * `desktop.confirm_quit` is on, naming what is running and what is unsaved;
 * then save every dirty document; then keep what is remembered — where the
 * person was, how each screen stood (`viewMemoryStore.settleMemories`) —
 * then `quit_app`. The question comes **before** the save: a cancelled quit
 * must not have written files. A save that fails keeps the window, with an
 * OS notice; a memory that cannot be kept never does. The window put away
 * writes its memories too: a hidden app may be ended from the menu bar. A
 * browser dev session keeps `beforeunload`'s question; there is no other
 * hook there.
 */
import { errorFields, log } from "../log";
import { useEffect } from "react";
import { anyDirty, dirtyCount, flushAll } from "../views/_workbench/editorRegistry";
import { closeFlow } from "./closeFlowModel.mjs";
import { deliverAppNotice } from "./notifications";
import { quitQuestion } from "./closeGuardModel.mjs";
import { confirmPrefs } from "./closeGuardSettings";
import { askClose } from "./terminalCloseGuard";
import { harnessOf, isLive } from "./terminalsModel.mjs";
import { TRAY_EVENTS, closeVerb } from "./trayModel.mjs";
import { trayPrefs } from "./trayPrefs";
import { terminalSessions } from "./useTerminals";
import { flushMemories, settleMemories } from "./viewMemoryStore";
import { t } from "../i18n/l10n.mjs";

/** Which door the request came through — for the log line alone; the flow is one. */
type Door = "window" | "quit";

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
});

/** The programmatic exit the shell lets through (`quit_app`). */
async function quit(): Promise<void> {
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("quit_app");
}

/** Run the flow; a way out that throws is logged — the window staying open is what the person sees. */
function leave(door: Door): void {
  flow.run(quit).then(
    (outcome) => log.info("close", "the way out ended", { door, outcome }),
    (e: unknown) => log.error("close", "the way out failed", { door, ...errorFields(e) }),
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

export function useCloseGuard(): void {
  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    let off: (() => void)[] = [];
    let gone = false;
    void import("@tauri-apps/api/event").then(async (ev) => {
      const close = await ev.listen(TRAY_EVENTS.close, () => {
        if (closeVerb(trayPrefs()) === "hide") hide();
        else leave("window");
      });
      const quitRequested = await ev.listen(TRAY_EVENTS.quit, () => leave("quit"));
      if (gone) {
        close();
        quitRequested();
      } else off = [close, quitRequested];
    });
    return () => {
      gone = true;
      for (const f of off) f();
    };
  }, []);
}
