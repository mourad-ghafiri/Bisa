/**
 * The setup gate's rules (16 — The setup gate): which platform the person is
 * on and so which official install lines to show, when the gate is a modal,
 * a banner or nothing, how far along the checks are, the words and tone of
 * each check, the one call a fix becomes, what a read of the checks leaves
 * the gate knowing, and when it reads again. Facts in, words out; the gate
 * draws them. Plain `.mjs`, so `node --test` reads it.
 */

import { t } from "../i18n/l10n.mjs";
import { settingsPath } from "../views/_settings/settingsLink.mjs";

/** The screens a person needs while fixing things: they stay usable under a banner. */
const OPEN_SCREENS = Object.freeze(["settings", "agents", "agent"]);

/** How often the gate reads again while something is missing, or while it does not know. */
export const RECHECK_MS = 20_000;

/** What the gate knows before its first read: nothing, and no reason yet. */
export const UNREAD = Object.freeze({ readiness: null, error: null });

/**
 * What a read of the checks leaves the gate knowing. A read that landed is
 * what the gate knows, and clears the last failure; one that failed keeps
 * the last answer — a node that stopped answering says nothing about git —
 * and says why.
 * @template R
 * @param {{readiness: R | null, error: string | null}} was
 * @param {{ok: true, readiness: R} | {ok: false, error: string}} read
 * @returns {{readiness: R | null, error: string | null}}
 */
export function afterRead(was, read) {
  return read.ok ? { readiness: read.readiness, error: null } : { readiness: was.readiness, error: read.error };
}

/**
 * How long until the gate reads again on its own, or `null` when it need
 * not: while something is missing — a person is installing it in another
 * window — and while the gate **does not know**, because its read failed.
 * An unknown the gate never asked about again would stand for the life of
 * the window: the node not yet up when the desktop opened is the ordinary
 * case, and a machine with no harness would then never see the gate.
 * Ready, or not read yet, waits for nothing.
 * @param {{readiness: {ready?: boolean} | null, error: string | null}} state
 * @returns {number | null} milliseconds
 */
export function recheckMs(state) {
  if (state.readiness === null) return state.error === null ? null : RECHECK_MS;
  return state.readiness.ready === false ? RECHECK_MS : null;
}

/**
 * Whether a fact on the bus is worth reading the checks again: a setting
 * changed — a fix of the Decision-Making Agent and every hand-made change to
 * `decisions.*` say so.
 * @param {{type?: string} | null | undefined} payload an engine event's payload
 */
export function movesReadiness(payload) {
  return payload?.type === "settings_changed";
}

/** The check ids, in the order the node gives them. */
export const CHECK_IDS = Object.freeze(["git", "harness", "decision_making_agent", "general_agent", "workflow_agent"]);

/**
 * The platform the person is on, as the install hints name it. Unknown reads
 * as linux — the lines most likely to run in a shell somewhere.
 * @param {{ platform?: string, userAgent?: string } | null | undefined} nav
 * @returns {"mac_os" | "linux" | "windows"}
 */
export function platformOf(nav) {
  const words = `${nav?.platform ?? ""} ${nav?.userAgent ?? ""}`.toLowerCase();
  if (/mac|iphone|ipad|darwin/.test(words)) return "mac_os";
  if (/win/.test(words)) return "windows";
  return "linux";
}

/**
 * The install lines of a hint for one platform, in the page's order.
 * @param {{ commands?: {platform: string, command: string}[] } | null | undefined} hint
 * @param {string} platform
 * @returns {string[]}
 */
export function commandsFor(hint, platform) {
  return (hint?.commands ?? []).filter((c) => c.platform === platform).map((c) => c.command);
}

/**
 * How the gate shows: nothing while ready or unknown; a banner over the
 * screens a person fixes things on; the modal everywhere else.
 * @param {{ ready: boolean | null | undefined, screen: string | null | undefined }} facts
 * @returns {"none" | "banner" | "modal"}
 */
export function gateMode({ ready, screen }) {
  if (ready !== false) return "none";
  return OPEN_SCREENS.includes(screen ?? "") ? "banner" : "modal";
}

/**
 * *3 of 5 ready*.
 * @param {{ checks?: {state: string}[] } | null | undefined} readiness
 */
export function progressWords(readiness) {
  const checks = readiness?.checks ?? [];
  const ready = checks.filter((c) => c.state === "ready").length;
  return t("shell-setup-ready", { ready, checks: checks.length });
}

const ICONS = Object.freeze({ git: "branch", harness: "harness", decision_making_agent: "decisions", general_agent: "coreAgent", workflow_agent: "coreAgent" });

/**
 * A check's tone, glyph and state word for its card.
 * @param {{ id: string, state: string } | null | undefined} check
 */
export function checkWords(check) {
  const state = check?.state ?? "unready";
  return {
    icon: ICONS[check?.id] ?? "warn",
    tone: state === "ready" ? "ok" : state === "missing" ? "danger" : "warn",
    word: state === "ready" ? t("shell-setup-ready-word") : state === "missing" ? t("shell-setup-installed") : t("shell-setup-set-up"),
  };
}

/**
 * What a door opens: the route and its search, or nothing.
 * @param {{ door: string, tab?: string } | null | undefined} door
 * @returns {{ route: { name: string }, search?: { tab: string } } | null}
 */
export function doorTarget(door) {
  if (!door || door.door === "none") return null;
  if (door.door === "settings") return { route: { name: "settings" }, search: { tab: door.tab ?? "harnesses" } };
  return { route: { name: "agents" } };
}

/**
 * The words on a door's button.
 * @param {{ door: string, tab?: string } | null | undefined} door
 */
export function doorLabel(door) {
  if (!door || door.door === "none") return "";
  if (door.door === "agents") return t("shell-setup-open-agents");
  // The path is the rail's own, for the panel the door opens (`doorRoute`): the button and the place it lands cannot disagree.
  return t("shell-setup-open-settings-path", { path: settingsPath(door.tab ?? "harnesses") });
}

/** The banner's sentence over Settings and Agents: how far along, and the two ways on. @param {Parameters<typeof progressWords>[0]} readiness */
export function bannerWords(readiness) {
  return t("shell-setup-gate-banner", { progress: progressWords(readiness) });
}

const isRecord = (v) => !!v && typeof v === "object" && !Array.isArray(v);
const named = (v) => (typeof v === "string" && v.trim() !== "" ? v : null);

/**
 * The one call a fix becomes — or `null` for a fix that is no call: a kind
 * this build does not know, settings that set nothing, an agent's move that
 * names no agent, no harness or no plan. Nothing is made up for what the
 * node did not say: a harness of `""` is a body the node refuses, and a plan
 * invented empty would take the agent's models and leave it unready again.
 * @param {{ kind?: string, set?: object, agent?: string, harness?: string, models?: object } | null | undefined} fix
 * @returns {{ kind: "settings", set: object } | { kind: "agent", id: string, body: { harness: string, models: object } } | null}
 */
export function fixBody(fix) {
  if (fix?.kind === "settings") return isRecord(fix.set) && Object.keys(fix.set).length > 0 ? { kind: "settings", set: fix.set } : null;
  if (fix?.kind !== "agent_harness") return null;
  const id = named(fix.agent);
  const harness = named(fix.harness);
  if (id === null || harness === null || !isRecord(fix.models)) return null;
  return { kind: "agent", id, body: { harness, models: fix.models } };
}

/**
 * The fixes a check's card offers: the ones that are a call, each once by
 * its label — a button that could do nothing is not drawn.
 * @template {{label?: string}} F
 * @param {{fixes?: readonly F[]} | null | undefined} check
 * @returns {F[]}
 */
export function offeredFixes(check) {
  const seen = new Set();
  return (check?.fixes ?? []).filter((fix) => {
    const label = named(fix?.label);
    if (label === null || seen.has(label) || fixBody(fix) === null) return false;
    seen.add(label);
    return true;
  });
}
