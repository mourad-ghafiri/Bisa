/**
 * Settings › Capabilities › Browser, as facts (ide/18): the keys, the three
 * words `browser.agents` takes and what each means, where agents may go,
 * when an agent's tab is kept out of sight, whether an agent may run a
 * script in a page, and the status card's sentence about this machine. The
 * registry panel draws the rest of the group. Plain `.mjs`, so `node --test`
 * reads it.
 */

import { t } from "../../i18n/l10n.mjs";

export const ENABLED_KEY = "browser.enabled";
export const HOME_KEY = "browser.home";
export const REMEMBER_KEY = "browser.remember_tabs";
export const AGENTS_KEY = "browser.agents";
export const REACH_KEY = "browser.agents.reach";
export const HEADLESS_KEY = "browser.agents.headless";
export const SCRIPTS_KEY = "browser.agents.scripts";

/** Who may drive the browser through the tools, in the order the switch shows. */
export const POLICIES = Object.freeze(["everyone", "assigned", "nobody"]);
export const DEFAULT_POLICY = "everyone";
export const REACHES = Object.freeze(["anywhere", "local_only"]);
export const DEFAULT_REACH = "anywhere";
/** When an agent's tab is kept out of sight, in the order the switch shows. */
export const HEADLESS_MODES = Object.freeze(["unattended", "always", "never"]);
export const DEFAULT_HEADLESS = "unattended";
/** Whether an agent may evaluate a script in a page, in the order the switch shows. */
export const SCRIPTS_MODES = Object.freeze(["allow", "refuse"]);
export const DEFAULT_SCRIPTS = "allow";

/** The switch's segments. */
export function policySegments() {
  return [
    { id: "everyone", label: t("settings-browser-settings-everyone"), icon: "agent" },
    { id: "assigned", label: t("settings-browser-settings-assigned"), icon: "skill" },
    { id: "nobody", label: t("settings-browser-settings-nobody"), icon: "close" },
  ];
}

/** What a policy means, in a sentence under the switch. @param {string} policy */
export function policyWords(policy) {
  switch (policy) {
    case "assigned":
      return t("settings-browser-settings-only-agents-carry-embedded-browser-skill");
    case "nobody":
      return t("settings-browser-settings-browser-tools-refuse-every-agent-platform");
    default:
      return t("settings-browser-settings-any-agent-may-open-read-drive");
  }
}

/** The headless switch's segments. */
export function headlessSegments() {
  return [
    { id: "unattended", label: t("settings-browser-settings-unattended"), icon: "hidden" },
    { id: "always", label: t("settings-browser-settings-always"), icon: "hidden" },
    { id: "never", label: t("settings-browser-settings-never"), icon: "page" },
  ];
}

/** What a headless mode means, in a sentence under its switch. @param {string} mode */
export function headlessWords(mode) {
  switch (mode) {
    case "always":
      return t("settings-browser-settings-every-tab-agent-opens-kept-out");
    case "never":
      return t("settings-browser-settings-every-tab-agent-opens-shown-beside");
    default:
      return t("settings-browser-settings-goal-auto-mode-where-nobody-watching");
  }
}

/** The scripts switch's segments. */
export function scriptsSegments() {
  return [
    { id: "allow", label: t("settings-browser-settings-allow"), icon: "code" },
    { id: "refuse", label: t("settings-people-panel-refuse"), icon: "close" },
  ];
}

/** What a scripts mode means, in a sentence under its switch. @param {string} mode */
export function scriptsWords(mode) {
  return mode === "refuse"
    ? t("settings-browser-settings-browser-eval-refuses-names-setting-agents")
    : t("settings-browser-settings-agent-may-evaluate-javascript-expression-tab");
}

/** What a reach means. @param {string} reach */
export function reachWords(reach) {
  return reach === "local_only"
    ? t("settings-browser-settings-agents-may-open-only-pages-served")
    : t("settings-browser-settings-agents-may-open-any-http-https");
}

/**
 * The status card's sentence: whether this build has a browser, whether it
 * is on, how many tabs are open and how many of those are kept out of sight.
 * @param {{available: boolean, enabled: boolean, tabs: number, headless?: number}} facts
 * @returns {{tone: "ok" | "warn" | "quiet", label: string, sentence: string}}
 */
export function statusWords({ available, enabled, tabs, headless = 0 }) {
  if (!available) return { tone: "quiet", label: t("settings-browser-settings-here"), sentence: t("settings-browser-settings-embedded-browser-desktop-app-s-window") };
  if (!enabled) return { tone: "warn", label: "off", sentence: t("settings-browser-settings-turned-off-machine-tab-opens-agents") };
  const n = Number(tabs) || 0;
  const k = Number(headless) || 0;
  const unseen = k === 0 ? "" : k === 1 ? ` ${t("settings-browser-settings-one-kept-out-sight")}` : ` ${t("settings-browser-settings-kept-out-sight", { k })}`;
  const open = n === 0 ? t("settings-browser-settings-tab-open") : n === 1 ? t("settings-browser-settings-one-tab-open") : t("settings-browser-settings-tabs-open", { n });
  return { tone: "ok", label: "on", sentence: t("settings-browser-settings-l-opens-browser-pane-beside-any", { open, unseen }) };
}
