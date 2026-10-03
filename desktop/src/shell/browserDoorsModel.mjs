/**
 * Where a tab opened "beside this screen" is at home (ide/18), as a fact:
 * the conversation the screen shows — a goal's thread, a channel, a direct
 * message, a conversation — published by the surface itself
 * (`chatTargetStore`), else the root the IDE is on, else the workflow the
 * route shows, else the workspace. One rule for the footer's *New tab*, a
 * link's *Open in Bisa's browser* and the palette's address row, so a tab
 * is never "the workspace's" while the person is plainly somewhere.
 * Plain `.mjs`, so `node --test` reads it.
 */

/**
 * @param {{kind: string, id: string} | null} target the conversation on screen, when one is published
 * @param {{scope: string, id: string} | null} root the workbench root the IDE is on
 * @param {{name: string, id?: string} | null} route the screen's route
 * @returns {{scope: string, id: string} | null}
 */
export function screenHome(target, root, route) {
  if (target && target.id) return { scope: target.kind, id: target.id };
  if (root && root.scope !== "machine") return { scope: root.scope, id: root.id };
  if (route?.name === "workflow" && route.id) return { scope: "workflow", id: route.id };
  if (route?.name === "goal" && route.id) return { scope: "goal", id: route.id };
  if (route?.name === "channel" && route.id) return { scope: "channel", id: route.id };
  if (route?.name === "dm" && route.id) return { scope: "dm", id: route.id };
  return null;
}

/**
 * What the person's door to the Browser pane does — ⌘⇧L and the palette,
 * beside any screen — as one word: **hide** the pane while it shows;
 * **show** it while a tab is in sight to look at; **open** a tab and show it
 * when there is none. Opening one is the person's act, made here at the
 * door: the pane itself, mounting — by a remembered address, by an addon's
 * navigation — opens nothing (ide/18 §The footer's count).
 * @param {{showing: boolean, seen: number}} facts `showing`: the pane shows the Browser occupant; `seen`: the tabs in sight
 * @returns {"hide" | "show" | "open"}
 */
export function paneToggle({ showing, seen }) {
  if (showing) return "hide";
  return (Number(seen) || 0) > 0 ? "show" : "open";
}

/**
 * What a change of the IDE's centre does to a browser tab at home in its
 * root (ide/18 §The browser tab): leaving documents carries the strip's
 * active tab to the Details pane — where the conversation's and the Board's
 * browser is — and returning carries the pane's tab at home here back to the
 * strip, the pane's occupant closing. Two sources, one per direction: the
 * person may change which tab the pane shows while the centre is the
 * conversation. Nothing to carry is `null`.
 * @param {"documents" | "conversation" | "board"} centre what the centre shows now
 * @param {string | null} centreTab the browser tab active in the strip and at home here
 * @param {string | null} paneTab the browser tab the Details pane shows, when it is at home here
 * @returns {{show: "pane", key: string} | {show: "centre", key: string} | null}
 */
export function followCentre(centre, centreTab, paneTab) {
  if (centre !== "documents") return centreTab ? { show: "pane", key: centreTab } : null;
  return paneTab ? { show: "centre", key: paneTab } : null;
}
