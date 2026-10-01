/**
 * The words and the small facts the addon surfaces share — the footer's
 * read-out and popover, the Settings panel, the windows' bars — so the three
 * never disagree (18 — Addons). Plain `.mjs`, so `node --test` reads it.
 *
 * A manifest's own words — its name, its description — are the developer's
 * content and never translated; every sentence around them is the catalog's.
 */

import { t } from "../i18n/l10n.mjs";
import { PERMISSIONS, networkHosts, permissionWord } from "./addonBridgeModel.mjs";

/** The read-out's tooltip and the popover's title line: how many windows show, of every addon installed. */
export function titleWords(showing, total) {
  if (total === 0) return t("addons-addons-model-no-addons");
  return t("addons-addons-model-showing-of", { showing, total });
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

/** What state an addon is in: running, off, or its files not on this machine. */
export function stateWords(addon) {
  if (addon.active) return t("addons-addons-model-state-running");
  if (!addon.files_present) return t("addons-addons-model-state-files-missing");
  return t("addons-addons-model-state-off");
}

/** A popover row's state: a running addon put away says so; the rest as `stateWords`. */
export function rowStateWords(row) {
  if (row.running && !row.shown) return t("addons-addons-model-state-put-away");
  return stateWords(row.addon);
}

/**
 * One permission, in plain words a person can weigh before granting it —
 * `network` names its hosts.
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

/** The review before an install: a title and one line per declared permission, or the line that says it asks for nothing. */
export function reviewWords(manifest) {
  const permissions = sortedPermissions(manifest.permissions ?? []);
  return {
    title: t("addons-addons-model-review-title", { name: manifest.name }),
    lead: permissions.length === 0 ? t("addons-addons-model-review-asks-nothing") : t("addons-addons-model-review-asks-for", { n: permissions.length }),
    lines: permissions.map((p) => permissionWords(p)),
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
 * The list as it will read once the node has taken the switch — the
 * optimistic picture: the record's `enabled` flipped and `active` recomputed
 * as the node computes it (enabled with its files here). An id the list does
 * not hold leaves it as it is.
 */
export function withEnabled(addons, id, enabled) {
  if (!addons.some((a) => a.id === id)) return addons;
  return addons.map((a) => (a.id === id ? { ...a, enabled, active: enabled && a.files_present } : a));
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
 * already holds, and nothing re-renders or resets for it.
 */
export function sameAddon(a, b) {
  return a.id === b.id && a.installed_at === b.installed_at && a.enabled === b.enabled && a.active === b.active && a.files_present === b.files_present && JSON.stringify(a.granted) === JSON.stringify(b.granted);
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
    frame: w.frame === "none" ? "none" : "bar",
    default_dock: w.default_dock ?? "bottom_right",
  };
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
