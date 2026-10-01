/**
 * A browser tab's bar (ide/18), wherever the tab shows — the IDE's centre
 * or the Details pane — and a browser's bar whole on every tab: back,
 * forward, reload (*Stop* while the page loads), the address field, the
 * wand, the camera, and outside the IDE the door to the tab's home there
 * and, on any page, the door to the machine's browser. What is free and
 * what is held, with its reason, is `browserChromeModel.mjs`'s word
 * (`chromeOf`): a blank tab holds everything but the address; the history
 * is the webview's own. The page under it is a native view
 * (`BrowserPanel.tsx`); the bar drives it through `browser/session.ts`.
 *
 * The bar answers the browser chords (`BROWSER_COMMAND`, ide/15): ⌘L the
 * address, ⌘[ ⌘] back and forward, ⌘R reload, ⌘T a tab beside, ⌘W close —
 * typed in the main window or relayed from inside the page — for its own
 * tab, and only from the host that draws it, since both hosts can hold a
 * bar for one tab and the centre wins.
 *
 * The camera is the person's screenshot — two buttons, *Copy a screenshot*
 * and *Save a screenshot as…*: the same snapshot an agent gets, copied to
 * the clipboard or saved where they choose (`browserShots.ts`). Buttons and
 * not a menu, since a menu is a surface the page yields to: it would blank
 * the very page about to be photographed.
 */

import { useEffect, useRef, useState, type ReactNode } from "react";
import { openExternal } from "../api";
import { errorFields, log } from "../log";
import { navigate } from "../router";
import { browserViewBack, browserViewForward, browserViewReload, browserViewStop, navigateBrowserView } from "../browser/session";
import type { ServedFolder } from "../types";
import { Button, ICON, TextInput, Tooltip, useToast } from "../ui";
import { servedWords } from "../views/_workbench/serversModel.mjs";
import { openBrowserPane } from "./browserDoors";
import { chromeOf, loadWords, wandWords } from "./browserChromeModel.mjs";
import type { BarVerb } from "./browserChromeModel.mjs";
import { placementOf } from "./browserPlacementModel.mjs";
import { copyShot, saveShot, takeShot } from "./browserShots";
import { shotWords } from "./browserShotModel.mjs";
import { BLANK_URL, PERSON, normalizeUrl, urlWords, workbenchHomeOf } from "./browsersModel.mjs";
import type { BrowserSession } from "./browsersModel.mjs";
import { layerSlot } from "./layerSlots";
import { BROWSER_COMMAND, onDoor } from "./shortcuts";
import type { BrowserCommandRequest } from "./shortcuts";
import { browserAsked, closeBrowserTab, focusBrowserTab, openBrowserIn } from "./useBrowsers";
import { t } from "../i18n/l10n.mjs";

/** The wand as the host holds it: whether the page can be annotated, whether picking is on, how many annotations wait. */
export interface BarWand {
  readonly enabled: boolean;
  readonly whyNot: string | null;
  readonly inspecting: boolean;
  readonly count: number;
  readonly onToggle: () => void;
}

/** A round icon button of the bar: free, or held with its reason on the tooltip. */
function BarButton({ verb, label, onClick, children }: { verb: BarVerb; label: string; onClick: () => void; children: ReactNode }) {
  return (
    <Tooltip label={verb.enabled ? label : `${label} — ${verb.why}`}>
      <span className="inline-flex">
        <Button size="icon" variant="ghost" className="h-7 w-7" aria-label={label} disabled={!verb.enabled} onClick={onClick}>
          {children}
        </Button>
      </span>
    </Tooltip>
  );
}

export function BrowserBar({ session, server = null, inIde, wand }: { session: BrowserSession; server?: ServedFolder | null; inIde: boolean; wand: BarWand }) {
  const toast = useToast();
  const key = session.key;
  const blank = session.url === BLANK_URL;
  const address = useRef<HTMLInputElement>(null);
  const [field, setField] = useState(blank ? "" : urlWords(session.url));
  const [editing, setEditing] = useState(false);
  const [shooting, setShooting] = useState(false);
  useEffect(() => {
    if (!editing) setField(blank ? "" : urlWords(session.url));
  }, [session.url, editing, blank]);

  // The tab's home in the IDE: where its strip is.
  const home = workbenchHomeOf(session);
  const chrome = chromeOf({ blank, loading: session.loading, canBack: session.canBack, canForward: session.canForward, inIde, atWorkbenchHome: home !== null, annotatable: wand.enabled, whyNot: wand.whyNot });

  const fail = (e: unknown) => toast.error(e instanceof Error ? e.message : String(e));
  const go = () => {
    const norm = normalizeUrl(field);
    if ("error" in norm) {
      toast.error(norm.error);
      return;
    }
    setEditing(false);
    address.current?.blur();
    browserAsked(key, norm.url);
    void navigateBrowserView(key, norm.url).catch(fail);
  };
  const back = () => void browserViewBack(key).catch((e: unknown) => log.warn("browser", "back did not take", { key, ...errorFields(e) }));
  const forward = () => void browserViewForward(key).catch((e: unknown) => log.warn("browser", "forward did not take", { key, ...errorFields(e) }));
  const load = () =>
    void (chrome.load.verb === "stop" ? browserViewStop(key) : browserViewReload(key)).catch((e: unknown) =>
      log.warn("browser", `${chrome.load.verb} did not take`, { key, ...errorFields(e) }),
    );
  const focusAddress = () => {
    const el = address.current;
    if (!el) return;
    el.focus();
    el.select();
  };
  /** The camera as it stands: held while a shot is being taken, else the chrome's word. */
  const camera: BarVerb = shooting ? { enabled: false, why: t("shell-browser-bar-screenshot-being-taken") } : chrome.camera;
  const shoot = async (deliver: "copy" | "save") => {
    setShooting(true);
    try {
      const shot = await takeShot(key);
      if (deliver === "copy") {
        await copyShot(shot);
        toast.ok(t("shell-browser-bar-copied-shot", { shot: shotWords(shot) }));
      } else {
        const path = await saveShot(shot);
        if (path) toast.ok(t("shell-browser-bar-saved", { path }));
      }
    } catch (e) {
      fail(e);
    } finally {
      setShooting(false);
    }
  };
  const openInIde = () => {
    if (!home) return;
    focusBrowserTab(key);
    navigate({ name: "workbench", scope: home.scope, id: home.id });
  };

  // The browser chords, for this tab, from the host that draws it.
  useEffect(() => {
    const mine = (request: BrowserCommandRequest | undefined) => {
      if (!request || request.key !== key) return;
      const drawn = placementOf(key, { center: layerSlot("center"), aux: layerSlot("aux"), mayShow: true });
      if (drawn?.host !== (inIde ? "center" : "aux")) return;
      switch (request.command) {
        case "focus_address":
          focusAddress();
          return;
        case "browser_back":
          if (chrome.back.enabled) back();
          return;
        case "browser_forward":
          if (chrome.forward.enabled) forward();
          return;
        case "browser_reload":
          if (chrome.load.enabled) load();
          return;
        case "new_browser_tab": {
          const opened = openBrowserIn({ home: session.home, by: PERSON });
          if (opened && !inIde) openBrowserPane(opened);
          return;
        }
        case "close_browser_tab":
          closeBrowserTab(key);
          return;
        default:
          return;
      }
    };
    // At the door (`onDoor`), never a listener added by hand: `fire` dispatches only to a counted one.
    return onDoor(BROWSER_COMMAND, (detail) => mine(detail as BrowserCommandRequest | undefined));
    // The verbs and `chrome` are remade every render; the door is re-hung on
    // the facts they read.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, inIde, chrome.back.enabled, chrome.forward.enabled, chrome.load.enabled, chrome.load.verb, session.home]);

  return (
    <div data-browser-bar className="flex h-9 shrink-0 items-center gap-1 border-b border-border px-1.5 text-2xs">
      <BarButton verb={chrome.back} label={t("shell-browser-bar-back")} onClick={back}>
        <ICON.back size={14} aria-hidden />
      </BarButton>
      <BarButton verb={chrome.forward} label={t("shell-browser-bar-forward")} onClick={forward}>
        <ICON.forward size={14} aria-hidden />
      </BarButton>
      <BarButton verb={chrome.load} label={loadWords(chrome.load.verb)} onClick={load}>
        {chrome.load.verb === "stop" ? <ICON.stop size={14} aria-hidden /> : <ICON.refresh size={14} aria-hidden />}
      </BarButton>
      <div className="relative min-w-0 flex-1">
        <ICON.page size={13} aria-hidden className="pointer-events-none absolute left-2 top-1/2 -translate-y-1/2 text-text-dim" />
        <TextInput
          ref={address}
          data-browser-address
          value={field}
          placeholder={t("shell-browser-bar-search-or-address")}
          aria-label={t("shell-browser-bar-address")}
          autoFocus={blank}
          spellCheck={false}
          autoComplete="off"
          className="h-7 w-full pl-7 font-mono text-2xs"
          onFocus={(e) => {
            setEditing(true);
            e.currentTarget.select();
          }}
          onBlur={() => setEditing(false)}
          onChange={(e) => setField(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              go();
            } else if (e.key === "Escape") {
              e.preventDefault();
              setField(blank ? "" : urlWords(session.url));
              setEditing(false);
              e.currentTarget.blur();
            }
          }}
        />
      </div>
      {server && !blank && (
        <span className="shrink-0 text-text-dim" title={t("shell-browser-bar-served-by-node-from", { from: servedWords(server) })}>{t("shell-browser-bar-served")}</span>
      )}
      <Tooltip label={wandWords(chrome.wand, wand.inspecting)}>
        <span className="inline-flex">
          <Button size="icon" className="relative h-7 w-7" variant={wand.inspecting ? "primary" : "ghost"} aria-pressed={wand.inspecting} aria-label={t("shell-browser-bar-annotate-page-agent")} disabled={!chrome.wand.enabled} onClick={wand.onToggle}>
            <ICON.annotate size={14} aria-hidden />
            {wand.count > 0 && (
              <span className="absolute -right-1 -top-1 inline-flex h-4 min-w-4 items-center justify-center rounded-full bg-accent px-1 text-3xs font-semibold text-accent-contrast" aria-label={t("shell-browser-bar-annotations", { wand: wand.count })}>
                {wand.count}
              </span>
            )}
          </Button>
        </span>
      </Tooltip>
      <BarButton verb={camera} label={t("shell-browser-bar-copy-screenshot-what-agent-s-browser")} onClick={() => void shoot("copy")}>
        <ICON.camera size={14} aria-hidden />
      </BarButton>
      <BarButton verb={camera} label={t("shell-browser-bar-save-screenshot")} onClick={() => void shoot("save")}>
        <ICON.save size={14} aria-hidden />
      </BarButton>
      {chrome.openInIde && (
        <BarButton verb={{ enabled: true, why: null }} label={t("shell-browser-bar-open-tab-project-ide-where-home")} onClick={openInIde}>
          <ICON.project size={14} aria-hidden />
        </BarButton>
      )}
      <BarButton verb={chrome.openOutside} label={t("shell-browser-bar-open-page-machine-s-browser")} onClick={() => void openExternal(session.url).catch(fail)}>
        <ICON.open size={14} aria-hidden />
      </BarButton>
    </div>
  );
}
