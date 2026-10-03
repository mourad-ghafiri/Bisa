/**
 * The doors to the embedded browser from anywhere (ide/18): the Details
 * pane's Browser occupant on a tab, a tab shown where it belongs, a URL
 * opened where the person is, an artifact's page opened with an origin of
 * its own. The IDE's Browser button, the footer, the palette, the
 * keymap and the agents' bridge come through here, so what "show the
 * browser" means is written once: a tab at home in the IDE's root is shown
 * in the strip while that root's centre shows documents, and in the pane
 * otherwise — the conversation's and the Board's browser is the pane's, as
 * every other screen's is (`browserBridgeModel.revealPlan`).
 */

import { useEffect } from "react";
import { api } from "../api";
import { currentRoute, setSearch } from "../router";
import { sayFailure, toaster } from "../ui";
import type { ArtifactRef } from "../types";
import { chatTargetNow } from "../views/_studio/chatTargetStore";
import { workbenchCentreFor } from "../views/_workbench/workbenchCentreStore";
import { revealPlan } from "./browserBridgeModel.mjs";
import { paneToggle, screenHome as homeOfScreen } from "./browserDoorsModel.mjs";
import { PERSON, seenSessions, sessionOf } from "./browsersModel.mjs";
import type { BrowserHome } from "./browsersModel.mjs";
import { NEW_BROWSER_HERE, ideRoot, onDoor, workbenchRoot } from "./shortcuts";
import { browserState, focusBrowserTab, openBrowserIn, revealBrowserTab, whyNotBrowser } from "./useBrowsers";
import { t } from "../i18n/l10n.mjs";

/**
 * Where a tab opened beside this screen is at home: the conversation on
 * screen, else the IDE's root, else the workflow or screen the route shows,
 * else the workspace (`browserDoorsModel.screenHome`).
 */
export function screenHome(): BrowserHome | null {
  const route = currentRoute();
  return homeOfScreen(chatTargetNow(), workbenchRoot(), { name: route.name, id: "id" in route ? route.id : undefined });
}

/** The Details pane's Browser occupant, on this tab — shown, if it was kept out of sight — or on whatever tab is active. The pane opens no tab of its own. */
export function openBrowserPane(key: string | null = null): void {
  if (key) revealBrowserTab(key);
  setSearch({ aux: "browser", auxId: key });
}

/**
 * The person's door to the Browser pane — ⌘⇧L and the palette, beside any
 * screen (`paneToggle`): hide it while it shows, show it while a tab
 * is in sight, and with none **open one** — at home in `home`, the person's
 * own — and show it. The one place a tab is opened for the pane: by an act.
 */
export function toggleBrowserPane(aux: { kind: string | null; toggle: (kind: "browser") => void }, home: BrowserHome | null = screenHome()): void {
  const plan = paneToggle({ showing: aux.kind === "browser", seen: seenSessions(browserState().sessions).length });
  if (plan === "open") openBrowserPane(openBrowserIn({ home, by: PERSON }));
  else aux.toggle("browser");
}

/**
 * Show a tab where it belongs (`revealPlan`): the IDE's strip when the IDE
 * is on the tab's home and its centre shows documents — the tab comes to
 * the front there — else the Details pane's Browser occupant, beside
 * whatever the person is on. A tab kept out of sight is left as it is.
 */
export function showBrowserTab(key: string): void {
  const session = sessionOf(browserState(), key);
  if (!session) return;
  const root = ideRoot();
  const plan = revealPlan(session, root, workbenchCentreFor(root));
  if (plan === "ide") focusBrowserTab(key);
  else if (plan === "pane") openBrowserPane(key);
}

/**
 * Open a tab at home in `home` — on `url`, or the home page — and show it
 * where it belongs. Answers the tab's key, or `null` when no tab could open.
 * A person's door — the keymap, a link's card, the palette, a launcher, a
 * served folder — so the tab is the person's.
 */
export function openBrowserAt(home: BrowserHome | null, url: string | null = null): string | null {
  const key = openBrowserIn(url ? { home, url, by: PERSON } : { home, by: PERSON });
  if (key) showBrowserTab(key);
  return key;
}

/**
 * The keymap's *New browser tab here* (`new_browser`): a blank tab at home
 * in the IDE's root, shown where that root's browser is. The keymap fires
 * the door; whoever mounts this — the browser layer — answers it.
 */
export function useNewBrowserHereDoor(): void {
  useEffect(() => onDoor(NEW_BROWSER_HERE, (detail) => void openBrowserAt((detail as { home: BrowserHome }).home)), []);
}

/**
 * Open a URL where the person is — at home in the screen's place
 * (`screenHome`), shown where that place's browser is: the IDE's strip
 * while the IDE is on a root whose centre shows documents, else the pane
 * beside the screen. Answers the tab's key, or `null` with a toast saying
 * why no tab could open.
 */
export function openUrlInBrowser(url: string | null): string | null {
  const key = openBrowserAt(screenHome(), url);
  if (!key) toaster.error(whyNotBrowser() ?? t("shell-browser-doors-browser-tab-could-open"));
  return key;
}

/** An artifact's page, served on a port of its own, opened in a tab where the person is. */
export async function openArtifactInBrowser(artifact: ArtifactRef): Promise<void> {
  try {
    const served = await api.serveArtifact(artifact.sha256, artifact.name);
    openUrlInBrowser(served.page);
  } catch (e) {
    toaster.error(sayFailure("browser", t("shell-browser-doors-could-not-open-page"), e));
  }
}
