/**
 * The shell's one notification duty: OS notifications for what needs a
 * person. Every decision is `notificationsModel.mjs`'s — which moments, and
 * which of them this person wants (`allowed` over the switches
 * `notifySettings.ts` follows); this file only delivers — through the plugin
 * that exists, silent in a browser. The count outside the window — the Dock
 * badge, the menu bar icon — is `useTray.ts`'s. Every notice, the app's own
 * and an addon's included, passes through `deliver`, so the permission is
 * asked once and the person's switches are honoured everywhere.
 */
import { errorFields, log } from "../log";
import { useEffect, useRef } from "react";
import { subscribe as busSubscribe } from "../bus";
import { allowed, emptyMemory, onFrame, onTransition } from "./notificationsModel.mjs";
import type { Notice, NotifyMemory } from "./notificationsModel.mjs";
import { notifyPrefs } from "./notifySettings";
import { onSessionTransition } from "./sessionsStore";

const inShell = () => "__TAURI_INTERNALS__" in window;

async function deliver(notice: Notice): Promise<void> {
  if (!inShell()) return;
  if (!allowed(notifyPrefs(), notice.category)) {
    log.debug("notifications", "a notification was kept back by the person's switches", { category: notice.category });
    return;
  }
  try {
    const n = await import("@tauri-apps/plugin-notification");
    let granted = await n.isPermissionGranted();
    if (!granted) granted = (await n.requestPermission()) === "granted";
    if (granted) n.sendNotification({ title: notice.title, body: notice.body });
    else log.debug("notifications", "the notification permission is not granted");
  } catch (e) {
    // Notifications are a courtesy; never let them break the app.
    log.debug("notifications", "a notification could not be sent", errorFields(e));
  }
}

/**
 * A notice an addon asked for (18 — Addons): the bridge has already judged
 * the grant, cut the words and paced the calls; this only delivers, under
 * the person's *Addons* switch.
 */
export function deliverNotice(title: string, body: string): Promise<void> {
  return deliver({ category: "addons", title, body });
}

/** The app's own word — a document that did not save on the way out — under the master switch alone. */
export function deliverAppNotice(title: string, body: string): Promise<void> {
  return deliver({ category: "app", title, body });
}

/** Mount once, in the shell. */
export function useNotifications(): void {
  const memory = useRef<NotifyMemory>(emptyMemory());
  useEffect(() => {
    const offTransitions = onSessionTransition((prev, row) => {
      const r = onTransition(prev, row, memory.current);
      memory.current = r.memory;
      if (r.notice) void deliver(r.notice);
    });
    const offBus = busSubscribe({ stream: "engine" }, (frame) => {
      if (frame.stream !== "engine") return;
      const r = onFrame(frame.payload.payload, memory.current);
      memory.current = r.memory;
      if (r.notice) void deliver(r.notice);
    });
    return () => {
      offTransitions();
      offBus();
    };
  }, []);
}
