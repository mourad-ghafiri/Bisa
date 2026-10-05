/**
 * The platform's presence outside the window (guide/the-desktop.md §The
 * menu bar icon): one report — what the platform is doing, how many things
 * need you, in words — sent to the shell's menu bar icon whenever it
 * changes, and the same count on the Dock badge. Every decision is
 * `trayModel.mjs`'s; this file only carries — through the shell that
 * exists, silent in a browser.
 *
 * Two words come back from the icon's menu: `tray:go` — the needs line was
 * chosen, the window is already shown, go to the Inbox — and `tray:dock` —
 * *Show in Dock* was toggled, record it as the setting, which then applies
 * itself (`trayPrefs.ts`).
 */

import { useEffect, useMemo, useRef } from "react";
import { api, inDesktopShell } from "../api";
import type { ConnState } from "../bus";
import { connected } from "../busModel.mjs";
import { errorFields, log } from "../log";
import { navigate } from "../router";
import { TRAY_EVENTS, TRAY_KEYS, trayReport } from "./trayModel.mjs";
import { useSettledSessions } from "./useSettledSessions";
import type { TrayReport } from "./trayModel.mjs";
import { useWorkspace } from "./useWorkspaceData";

async function report(r: TrayReport): Promise<void> {
  if (!inDesktopShell()) return;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("tray_report", { report: r });
  } catch (e) {
    // The icon keeps its last word; the footer and the rail still say so.
    log.debug("tray", "the report did not reach the shell", errorFields(e));
  }
}

async function badge(count: number): Promise<void> {
  if (!inDesktopShell()) return;
  try {
    const w = await import("@tauri-apps/api/window");
    await w.getCurrentWindow().setBadgeCount(count > 0 ? count : undefined);
  } catch (e) {
    log.debug("tray", "the Dock badge could not be set", errorFields(e));
  }
}

/** Mount once, in the shell, with the one connection state `App` watches. */
export function useTray(conn: ConnState): void {
  const ws = useWorkspace();
  // As the tabs say it: a harness whose tab exited is not working.
  const sessions = useSettledSessions();
  // Before the node has ever answered, not open is *connecting*; after, it is *trouble*.
  const everOpen = useRef(false);
  if (connected(conn)) everOpen.current = true;
  const current = useMemo(
    () => trayReport({ conn, everOpen: everOpen.current, paused: ws.paused, inbox: ws.inbox, sessions, working: ws.working }),
    [conn, ws.paused, ws.inbox, sessions, ws.working],
  );
  // The same words again are no report: a refetched list is a new array, not news.
  const sent = useRef("");
  useEffect(() => {
    const key = JSON.stringify(current);
    if (key === sent.current) return;
    sent.current = key;
    void report(current);
  }, [current]);
  useEffect(() => {
    void badge(current.needs);
  }, [current.needs]);
  useEffect(() => {
    if (!inDesktopShell()) return;
    let off: (() => void)[] = [];
    let gone = false;
    void import("@tauri-apps/api/event").then(async (ev) => {
      const go = await ev.listen<{ to: string }>(TRAY_EVENTS.go, (e) => {
        if (e.payload.to === "inbox") navigate({ name: "inbox" });
      });
      const dock = await ev.listen<{ visible: boolean }>(TRAY_EVENTS.dock, (e) => {
        api.setSettings("machine", { [TRAY_KEYS.dockIcon]: e.payload.visible }).catch((err: unknown) => {
          log.warn("tray", "the Dock choice was not remembered", errorFields(err));
        });
      });
      if (gone) {
        go();
        dock();
      } else off = [go, dock];
    });
    return () => {
      gone = true;
      for (const f of off) f();
    };
  }, []);
}
