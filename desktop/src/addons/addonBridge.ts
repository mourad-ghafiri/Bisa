/**
 * The app's half of an addon's door (18 — Addons): one `message` listener
 * per frame, trusting **the frame's own window and nothing else** — the
 * origin is opaque, so every send targets `"*"` and every receipt is judged
 * by `event.source`, the way the page inspector's wire is
 * (`ui/artifact/usePageInspector.ts`). Every message is read by
 * `addonBridgeModel.mjs`, which answers with a reply and an effect; this
 * hook performs the effect — a notice, the clipboard, the node's broker,
 * a confirmed link, a route, a window move — and pushes the events the
 * addon listens to: the theme, whether it is visible, the
 * workspace's summary and, while subscribed, the machine's load.
 */

import { useCallback, useEffect, useRef, type RefObject } from "react";
import { api } from "../api";
import { locale } from "../i18n/l10n.mjs";
import { errorFields, log } from "../log";
import { navigate, type Route } from "../router";
import { deliverNotice } from "../shell/notifications";
import { readPref, webStorage, writePref } from "../shell/storedPrefModel.mjs";
import type { HostLoad } from "../shell/statsApi";
import type { Addon } from "../types";
import { copyText, useThemeNonce } from "../ui";
import { readInspectorTheme } from "../ui/artifact/inspectorTokens";
import { STORAGE_KEY_PREFIX, errorReply, eventMessage, fetchResult, handleCall, helloMessage, newSession, okReply, parseAddonMessage, refusedCode, storageBag } from "./addonBridgeModel.mjs";
import type { AddonSession, Effect, Reply, StorageBag } from "./addonBridgeModel.mjs";

/** What the workspace looks like to an addon that asked. */
export interface AddonSummary {
  waiting: number;
  review: number;
  working: number;
}

/** The machine's load as an addon hears it: the footer's sample, and the volume the platform's data lives on. */
export type AddonLoad = HostLoad & {
  disk: { used: number; total: number; mount: string; workspace_bytes: number } | null;
};

export interface AddonBridgeHandlers {
  /** The addon asked to be this size; the window clamps and decides. */
  onResize: (size: { width: number; height: number }) => void;
  /** The addon asked to close; the window puts itself away. */
  onClose: () => void;
  /** The addon named its own title for the bar. */
  onTitle: (title: string) => void;
  /** The addon asked to open a link: the layer asks the person and answers. */
  onOpenUrl: (url: string) => Promise<boolean>;
}

export function useAddonBridge(
  frameRef: RefObject<HTMLIFrameElement | null>,
  addon: Addon,
  {
    visible,
    summary,
    load,
    handlers,
  }: {
    /** Whether the window is on screen — hidden by a layer, put away, or the layer off. */
    visible: boolean;
    summary: AddonSummary;
    load: AddonLoad | null;
    handlers: AddonBridgeHandlers;
  },
): void {
  const session = useRef<AddonSession>(newSession(addon));
  const storage = useRef<StorageBag>(storageBag(readPref(webStorage(), STORAGE_KEY_PREFIX + addon.id, (s) => s, null)));
  const latest = useRef({ summary, handlers, load, visible });
  latest.current = { summary, handlers, load, visible };
  const nonce = useThemeNonce();

  // A new frame — the addon reinstalled, its grants changed — is a new
  // session: the iframe's key changes on the same three facts, the page
  // reloads and the hello comes again. A re-read that changed nothing keeps
  // the object; one that changed another record must not reset this one — a
  // session reset without a reload would refuse every later call `not_ready`.
  const grantsKey = JSON.stringify(addon.granted);
  useEffect(() => {
    session.current = newSession(addon);
    // Keyed on the three facts above, never the `addon` object a re-read remakes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [addon.id, addon.installed_at, grantsKey]);

  const post = useCallback(
    (message: unknown) => {
      const target = frameRef.current?.contentWindow;
      if (!target) return;
      target.postMessage(message, "*");
    },
    [frameRef],
  );

  /** Answer a call — and, when the answer is a refusal, say so in the log with its code: what a developer testing an addon reads. */
  const answer = useCallback(
    (reply: unknown, method: string | null) => {
      const code = refusedCode(reply);
      if (code) log.info("addons", "a call was refused", { addon: addon.id, method, code });
      post(reply);
    },
    [addon.id, post],
  );

  const perform = useCallback(
    (effect: Effect, id: string | null) => {
      const h = latest.current.handlers;
      switch (effect.type) {
        case "notify":
          void deliverNotice(effect.title, effect.body);
          return;
        case "clipboard":
          void copyText(effect.text).catch((e: unknown) => log.debug("addons", "the clipboard did not take the text", errorFields(e)));
          return;
        case "storage":
          writePref(webStorage(), STORAGE_KEY_PREFIX + addon.id, JSON.stringify(storage.current));
          return;
        case "fetch":
          void api
            .addonFetch(addon.id, effect.url, effect.accept)
            .then((r) => post(okReply(effect.id, fetchResult(r))))
            .catch((e: unknown) => answer(errorReply(effect.id, "refused", e instanceof Error ? e.message : undefined), "network.fetch"));
          return;
        case "open_url":
          void h
            .onOpenUrl(effect.url)
            .then((opened) => answer(opened ? okReply(effect.id, { opened: true }) : errorReply(effect.id, "refused"), "url.open"))
            .catch(() => answer(errorReply(effect.id, "refused"), "url.open"));
          return;
        case "navigate":
          navigate({ name: effect.route } as Route);
          return;
        case "resize":
          h.onResize({ width: effect.width, height: effect.height });
          return;
        case "close":
          h.onClose();
          return;
        case "title":
          h.onTitle(effect.title);
          return;
        case "subscribe":
          if (latest.current.load) post(eventMessage("system.load", latest.current.load));
          return;
        case "unsubscribe":
          return;
        default:
          if (id) answer(errorReply(id, "unavailable"), null);
      }
    },
    [addon.id, answer, post],
  );

  // The one listener: the frame's own window, or nothing.
  useEffect(() => {
    const onMessage = (e: MessageEvent) => {
      const frame = frameRef.current;
      if (!frame || e.source !== frame.contentWindow) return;
      const parsed = parseAddonMessage(e.data);
      if (!parsed) return;
      if (parsed.kind === "hello") {
        session.current = { ...session.current, ready: true };
        post(helloMessage(session.current, { locale: locale(), scheme: readInspectorTheme().scheme }));
        post(eventMessage("visibility", { visible: latest.current.visible }));
        return;
      }
      if (parsed.kind === "bad") {
        if (parsed.id) answer(errorReply(parsed.id, parsed.code), null);
        return;
      }
      const out = handleCall(session.current, parsed, {
        now: Date.now(),
        storage: storage.current,
        platform: { version: __APP_VERSION__, locale: locale() },
        scheme: readInspectorTheme().scheme,
        summary: latest.current.summary,
      });
      session.current = out.session;
      storage.current = out.storage;
      if (out.effect) perform(out.effect, parsed.id);
      if (out.reply) answer(out.reply as Reply, parsed.method);
    };
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, [frameRef, perform, post, answer]);

  // The theme moved: the addon that listens hears the scheme.
  useEffect(() => {
    if (!session.current.ready) return;
    post(eventMessage("theme", { scheme: readInspectorTheme().scheme }));
  }, [nonce, post]);

  // Shown or hidden: an addon may stop its clock while nobody sees it.
  useEffect(() => {
    if (!session.current.ready) return;
    post(eventMessage("visibility", { visible }));
  }, [visible, post]);

  // The workspace's counts moved.
  const summaryKey = `${summary.waiting}:${summary.review}:${summary.working}`;
  useEffect(() => {
    if (!session.current.ready || !session.current.granted.includes("workspace_summary")) return;
    post(eventMessage("workspace.summary", latest.current.summary));
  }, [summaryKey, post]);

  // The machine's load, at the footer's cadence, only while subscribed.
  useEffect(() => {
    if (!load || !session.current.ready || !session.current.subscribed) return;
    post(eventMessage("system.load", load));
  }, [load, post]);
}
