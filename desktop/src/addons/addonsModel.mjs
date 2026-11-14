/**
 * The words and the small facts the addon surfaces share — the footer's
 * read-out and popover, the Settings panel, the windows' bars — so the three
 * never disagree (18 — Addons). Plain `.mjs`, so `node --test` reads it.
 *
 * A manifest's own words — its name, its description — are the developer's
 * content and never translated; every sentence around them is the catalog's.
 */

import { t } from "../i18n/l10n.mjs";
import { PERMISSIONS, frameOf, mediaGrant, networkHosts, permissionWord } from "./addonBridgeModel.mjs";

/** The read-out's tooltip and the popover's title line: how many windows show, of every addon installed. */
export function titleWords(showing, total, read = { loaded: true, failed: false }) {
  // *None* is a fact somebody read: before the first answer, and after a read that failed with
  // nothing yet known, the read-out says where the reading stands instead.
  if (total === 0 && !read.loaded) return read.failed ? t("addons-addons-model-not-read") : t("addons-addons-model-reading");
  if (total === 0) return t("addons-addons-model-no-addons");
  return t("addons-addons-model-showing-of", { showing, total });
}

/**
 * The addons whose window's place is still remembered though they are no
 * longer installed: the ids behind the stored keys of `prefix` that the list
 * does not hold. A place is furniture and is forgotten with its addon; an
 * addon's own **store** is never forgotten on a list alone — a record the
 * node could not read is left out of the list, and its notes must outlive that.
 * @param {readonly string[]} keys every key this machine's storage holds
 * @param {string} prefix
 * @param {readonly {id: string}[]} addons
 */
export function staleWindowIds(keys, prefix, addons) {
  const installed = new Set(addons.map((a) => a.id));
  return keys.filter((k) => k.startsWith(prefix) && k.length > prefix.length).map((k) => k.slice(prefix.length)).filter((id) => !installed.has(id));
}

/** The layer switch's label and its hint. */
export function switchWords(layerShown) {
  return {
    label: t("addons-addons-model-show-addons"),
    hint: layerShown ? t("addons-addons-model-hint-on") : t("addons-addons-model-hint-off"),
  };
}

/** Where an addon came from, in a word. */
export function originWords(origin) {
  if (origin && typeof origin === "object" && "catalog" in origin) return t("addons-addons-model-origin-catalog");
  return t("addons-addons-model-origin-imported");
}

/**
 * What state an addon is in: running, starting (its switch on, the node not
 * yet listing it running — the moment between a switch and the node's word),
 * off, or its files not on this machine.
 */
export function stateWords(addon) {
  if (addon.active) return t("addons-addons-model-state-running");
  if (!addon.files_present) return t("addons-addons-model-state-files-missing");
  if (addon.enabled) return t("addons-addons-model-state-starting");
  return t("addons-addons-model-state-off");
}

/** A popover row's state: a running addon put away says so; the rest as `stateWords`. */
export function rowStateWords(row) {
  if (row.running && !row.shown) return t("addons-addons-model-state-put-away");
  return stateWords(row.addon);
}

/**
 * One permission, in plain words a person can weigh before granting it —
 * `network` names its hosts; `media` its sites (`*` said as any site) and
 * whether YouTube's player comes beside the window.
 * @param {unknown} permission the manifest's spelling
 */
export function permissionWords(permission) {
  const word = permissionWord(permission);
  switch (word) {
    case "platform_info":
      return t("addons-addons-model-permission-platform-info");
    case "theme":
      return t("addons-addons-model-permission-theme");
    case "system_load":
      return t("addons-addons-model-permission-system-load");
    case "workspace_summary":
      return t("addons-addons-model-permission-workspace-summary");
    case "notify":
      return t("addons-addons-model-permission-notify");
    case "clipboard_write":
      return t("addons-addons-model-permission-clipboard-write");
    case "storage":
      return t("addons-addons-model-permission-storage");
    case "network":
      return t("addons-addons-model-permission-network", { hosts: networkHosts([permission]).join(", ") });
    case "open_url":
      return t("addons-addons-model-permission-open-url");
    case "navigate":
      return t("addons-addons-model-permission-navigate");
    case "media": {
      const grant = mediaGrant([permission]);
      const hosts = grant?.hosts ?? [];
      const sites = hosts.includes("*") ? t("addons-addons-model-permission-media-any-site") : hosts.join(", ");
      if (!grant?.youtube) return t("addons-addons-model-permission-media", { sites });
      return hosts.length === 0 ? t("addons-addons-model-permission-media-youtube-only") : t("addons-addons-model-permission-media-youtube", { sites });
    }
    default:
      return t("addons-addons-model-permission-unknown");
  }
}

/** The order a review lists permissions in: the platform's, then the reaching ones last. */
export function sortedPermissions(permissions) {
  return [...permissions].sort((a, b) => PERMISSIONS.indexOf(permissionWord(a)) - PERMISSIONS.indexOf(permissionWord(b)));
}

/** Whether a declared permission is among the grants — by its word. */
export function isGranted(granted, permission) {
  const word = permissionWord(permission);
  return granted.some((g) => permissionWord(g) === word);
}

/**
 * The grants after one switch: the declared permission added exactly as
 * declared, or taken out. Never anything the manifest did not declare.
 * @param {unknown[]} granted
 * @param {unknown[]} declared
 * @param {unknown} permission one of `declared`
 * @param {boolean} on
 */
export function grantToggle(granted, declared, permission, on) {
  const word = permissionWord(permission);
  const declaredOne = declared.find((d) => permissionWord(d) === word);
  const without = granted.filter((g) => permissionWord(g) !== word);
  if (!on || declaredOne === undefined) return without;
  return [...without, declaredOne];
}

/**
 * The review before an install: a title, one line per declared permission
 * or the line that says it asks for nothing, and — for a scene — its
 * **reach**: it draws over the whole app, which no permission word says.
 */
export function reviewWords(manifest) {
  const permissions = sortedPermissions(manifest.permissions ?? []);
  const scene = frameOf(manifest.window ?? {}) === "scene";
  return {
    title: t("addons-addons-model-review-title", { name: manifest.name }),
    lead: permissions.length === 0 ? t("addons-addons-model-review-asks-nothing") : t("addons-addons-model-review-asks-for", { n: permissions.length }),
    lines: permissions.map((p) => permissionWords(p)),
    reach: scene ? t("addons-addons-model-review-scene") : null,
    version: t("addons-addons-model-version-licence", { version: manifest.version, license: manifest.license }),
  };
}

/** A folder's problems as lines: the field, then the sentence. */
export function problemLines(problems, tx) {
  return problems.map((p) => (p.field ? `${p.field}: ${tx(p.text)}` : tx(p.text)));
}

/** Installed first, then running before off, then by name. */
export function sortAddons(addons) {
  return [...addons].sort((a, b) => Number(b.active) - Number(a.active) || a.manifest.name.localeCompare(b.manifest.name));
}

/** The catalog's offers not yet installed here, by name. */
export function offerRows(offers) {
  return offers.filter((o) => !o.installed).sort((a, b) => a.manifest.name.localeCompare(b.manifest.name));
}

/**
 * The windows a layer draws — and the count the footer tints by: enabled,
 * files here, not put away, while the layer shows and the machine's switch is
 * on. One rule, read by the layer, the read-out and the popover.
 */
export function visibleAddons(addons, hidden, layerShown, switchedOn) {
  if (!layerShown || !switchedOn) return [];
  return addons.filter((a) => a.active && !hidden.includes(a.id));
}

/**
 * The popover's rows: every installed addon, running first then by name,
 * each with whether it runs, whether its window shows, and whether its switch
 * may be turned on (its files are here).
 */
export function overlayRows(addons, hidden, layerShown, switchedOn) {
  const showing = new Set(visibleAddons(addons, hidden, layerShown, switchedOn).map((a) => a.id));
  return sortAddons(addons).map((addon) => ({
    addon,
    running: addon.active,
    shown: showing.has(addon.id),
    putAway: hidden.includes(addon.id),
    canEnable: addon.files_present,
  }));
}

/**
 * The list as it reads while the switch is out — the optimistic picture:
 * the record's `enabled` flipped; `active` cleared by a switch off (the
 * window leaves at once — the node refuses its files either way) and left
 * as the node last said it by a switch on, so the window mounts only once
 * the node lists the addon active and will serve its files: a frame that
 * mounted first got a bare 404 and, its key unmoved, never loaded again.
 * An id the list does not hold leaves it as it is.
 */
export function withEnabled(addons, id, enabled) {
  if (!addons.some((a) => a.id === id)) return addons;
  return addons.map((a) => (a.id === id ? { ...a, enabled, active: enabled ? a.active : false } : a));
}

/**
 * The list with the object held before a switch back in its id's place — a
 * refusal's undo, on the list as it stands now, so a re-read that landed
 * meanwhile for another addon is not undone with it. An id the list no
 * longer holds leaves it as it is.
 */
export function restoreAddon(addons, prior) {
  return addons.map((a) => (a.id === prior.id ? prior : a));
}

/** The put-away list with the ids no installed addon has taken out. */
export function pruneHidden(hidden, addons) {
  const ids = new Set(addons.map((a) => a.id));
  const kept = hidden.filter((h) => ids.has(h));
  return kept.length === hidden.length ? hidden : kept;
}

/**
 * Whether two readings of one addon say the same: the record's facts, not
 * the object — so a re-read that changed nothing keeps the object a window
 * already holds, and nothing re-renders or resets for it. The manifest's
 * version is a fact: a built-in refreshed at open keeps its `installed_at`,
 * and its window must load the new files under the new manifest.
 */
export function sameAddon(a, b) {
  return a.id === b.id && a.installed_at === b.installed_at && a.manifest.version === b.manifest.version && a.enabled === b.enabled && a.active === b.active && a.files_present === b.files_present && JSON.stringify(a.granted) === JSON.stringify(b.granted);
}

/**
 * The frame's key: the page reloads when it moves — the addon reinstalled,
 * its grants changed (said in the hello), a built-in refreshed to another
 * version, a reload asked from the window.
 * @param {{id: string, installed_at: number, manifest: {version: string}, granted: unknown[]}} addon
 * @param {number} reloads how many reloads the window asked for
 */
export function frameKeyOf(addon, reloads = 0) {
  return `${addon.id}:${addon.installed_at}:${addon.manifest.version}:${JSON.stringify(addon.granted)}:${reloads}`;
}

/** How long a page has to say hello before the window says it has not started. */
export const START_DEADLINE_MS = 8000;

/** The window's note past the deadline — the kit's failure note, with its Retry. The host cannot tell a broken page from a slow one, so the note says *not yet*. @param {string} name */
export function notStartedWords(name) {
  return t("addons-addons-model-not-started", { name });
}

/** A scene past the deadline, said once in a toast: it has no window to carry a note. @param {string} name */
export function sceneNotStartedWords(name) {
  return t("addons-addons-model-scene-not-started", { name });
}

/** The toast for a window that threw: that addon alone was put away; the eye brings it back afresh. @param {string} name */
export function stoppedWords(name) {
  return t("addons-addons-model-stopped", { name });
}

/**
 * What a screen reader hears of a scene's mode: who has the pointer while it
 * hunts, and how to take it back — and nothing at work. Said from a line that
 * is always there, so the change itself is what is announced.
 * @param {string} name @param {string} mode
 */
export function huntStatus(name, mode) {
  return mode === "hunt" ? t("addons-addons-model-hunt", { name }) : "";
}

/**
 * A window's slot for its first opening — its place among every installed
 * addon by id, which does not move when another window shows or hides.
 */
export function slotOf(addons, id) {
  const ids = addons.map((a) => a.id).sort();
  return Math.max(0, ids.indexOf(id));
}

/** The manifest's window as it stands: the wire may leave the object out when every field is its default. */
export function manifestWindow(manifest) {
  const w = manifest.window ?? {};
  return {
    width: Number(w.width) || 240,
    height: Number(w.height) || 160,
    min_width: w.min_width ?? null,
    min_height: w.min_height ?? null,
    max_width: w.max_width ?? null,
    max_height: w.max_height ?? null,
    resizable: w.resizable !== false,
    closable: w.closable !== false,
    transparent: w.transparent === true,
    frame: frameOf(w),
    default_dock: w.default_dock ?? "bottom_right",
  };
}

/** Whether an addon is a scene — the whole desktop under the chrome rather than a window (18 — Addons). @param {{manifest: {window?: unknown}}} addon */
export function isScene(addon) {
  return frameOf(addon.manifest?.window) === "scene";
}

/** The permissions a manifest declares — none when the wire left the list out. */
export function declaredOf(manifest) {
  return Array.isArray(manifest.permissions) ? manifest.permissions : [];
}

/** The page the window shows. */
export function entryOf(manifest) {
  return typeof manifest.entry === "string" && manifest.entry !== "" ? manifest.entry : "index.html";
}

/** The window's bar title: what the addon named itself, else its name. */
export function barTitle(addon, title) {
  return title && title.trim() !== "" ? title : addon.manifest.name;
}

/** How many levels stand between the layer's floor and the panels above it. */
const STACK_LEVELS = 10;

/**
 * The order windows were brought forward in, after `id` was: `id` last — in
 * front. Already in front, the same list comes back and nothing is redrawn.
 * @param {readonly string[]} order @param {string} id
 */
export function raised(order, id) {
  if (order.length > 0 && order[order.length - 1] === id) return order;
  return [...order.filter((o) => o !== id), id];
}

/**
 * How far above the layer's floor a window stands: a window never brought
 * forward is under every one that was, in the list's own order; the last one
 * brought forward is on top. A window is raised by its **level**, never by
 * moving its node — a frame moved in the document loads again. Ten levels
 * stand under the panels: past ten windows, the ones longest untouched share
 * the floor.
 * @param {readonly string[]} order the order windows were brought forward in
 * @param {readonly string[]} shown the windows showing, in the list's order
 * @param {string} id
 */
export function stackOf(order, shown, id) {
  const never = shown.filter((s) => !order.includes(s));
  const stack = [...never, ...order.filter((o) => shown.includes(o))];
  const at = stack.indexOf(id);
  if (at === -1) return 0;
  return Math.max(0, at - Math.max(0, stack.length - STACK_LEVELS));
}

/**
 * The palette's rows for the addons: the layer's switch, each running
 * window's *Put away* or *Show*, and the door to their settings. With the
 * layer hidden a window's own row would do nothing a person could see, and
 * with the machine's switch off nothing draws at all — the settings are where
 * both are changed.
 * @param {{addons: readonly {id: string, active: boolean, manifest: {name: string}}[], hidden: readonly string[], layerShown: boolean, switchedOn: boolean}} facts
 * @returns {{id: string, label: string, addon?: string}[]}
 */
export function addonCommands({ addons, hidden, layerShown, switchedOn }) {
  const rows = [];
  if (switchedOn && addons.length > 0) {
    rows.push({ id: "layer", label: layerShown ? t("addons-addons-model-hide-addons") : t("addons-addons-model-show-addons") });
    if (layerShown) {
      for (const a of addons) {
        if (!a.active) continue;
        // content, never translated: the addon's own name.
        const name = a.manifest.name;
        rows.push(hidden.includes(a.id) ? { id: `show:${a.id}`, label: t("shell-addons-overlay-show-window", { name }), addon: a.id } : { id: `away:${a.id}`, label: t("shell-addons-overlay-put-away", { name }), addon: a.id });
      }
    }
  }
  rows.push({ id: "settings", label: t("addons-addons-model-settings") });
  return rows;
}
