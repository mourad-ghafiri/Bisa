/**
 * Which moments deserve an OS notification, decided in one place — and
 * which of them this person wants (Settings › System › Notifications).
 *
 * Two sources feed it: session transitions (from the roster — a session that
 * starts *waiting on you*, *fails*, or is *done*), and the frames that have no
 * session behind them (a step's approval, a question, a proposed workflow, a
 * guided design that stalled), and the committer ask when nobody was at the
 * keyboard to see it — an agent's settlement refused, or a project a workflow
 * step made. A gate a waiting session already announced is not announced
 * twice: the session's `gate_id` is remembered. Two notices of the Inbox's
 * reach the OS as failures: a start event that could not start a run — a
 * library workflow's or a listening goal's — and a workstream script that
 * failed — each a failure nobody was looking at, said once per event or run.
 *
 * Every notice wears a **category** — what happened, in the platform's words:
 * `asks` (waiting on you), `failures`, `done` (finished work), `workflows`,
 * `addons` (a notice an addon sent), and `app` (the app's own word, a
 * document that did not save on the way out). The branches decide the
 * moment; one rule, `allowed`, applies the person's switches: the master
 * (`notifications.enabled`) over everything, then one switch per category —
 * every one on by default but `done`, since finished work is not an
 * interruption; `app` is under the master alone.
 */

import { titleOf } from "./sessionOriginModel.mjs";
import { boolOf } from "./settingsModel.mjs";
import { isNotifiable, waitWords } from "../ui/sessionState.mjs";
import { madeByAgentStep } from "../views/_work/projectOriginModel.mjs";
import { hostOf } from "../activityModel.mjs";
import { t } from "../i18n/l10n.mjs";

/** The registry keys, spelt once: the master, then a switch per category. */
export const NOTIFY_KEYS = Object.freeze({
  enabled: "notifications.enabled",
  asks: "notifications.asks",
  failures: "notifications.failures",
  done: "notifications.done",
  workflows: "notifications.workflows",
  addons: "notifications.addons",
});

/** The registry's defaults: everything on but finished work. */
export const NOTIFY_DEFAULTS = Object.freeze({ enabled: true, asks: true, failures: true, done: false, workflows: true, addons: true });

/** The categories a switch governs, in the panel's order. */
export const CATEGORIES = Object.freeze(["asks", "failures", "done", "workflows", "addons"]);

/** The person's switches, read from the resolved machine layer; an absent key is its default. */
export function readNotifyPrefs(resolved) {
  const out = {};
  for (const [name, key] of Object.entries(NOTIFY_KEYS)) out[name] = boolOf(resolved, key, NOTIFY_DEFAULTS[name]);
  return out;
}

/** Whether a `settings_changed` frame names one of the switches. */
export function namesNotifyKey(keys) {
  const wanted = new Set(Object.values(NOTIFY_KEYS));
  return Array.isArray(keys) && keys.some((k) => wanted.has(k));
}

/** A session state's category: waiting is an ask, failed a failure, done finished work. */
export function categoryOfState(word) {
  return word === "waiting" ? "asks" : word === "failed" ? "failures" : word === "done" ? "done" : null;
}

/**
 * Whether a notice of `category` reaches the OS under `prefs`: the master
 * first — off, nothing does, the app's own word included — then the
 * category's switch; `app` has none beyond the master.
 * @param {ReturnType<typeof readNotifyPrefs>} prefs
 * @param {"asks" | "failures" | "done" | "workflows" | "addons" | "app"} category
 */
export function allowed(prefs, category) {
  if (prefs.enabled === false) return false;
  if (category === "app") return true;
  return CATEGORIES.includes(category) ? prefs[category] !== false && !(category === "done" && prefs.done !== true) : false;
}

/** How many gate ids are remembered before the oldest are forgotten. */
export const MEMORY_CAP = 200;

export function emptyMemory() {
  return { gates: [] };
}

/** The one key a failure frame is remembered under, so it is announced once. */
function failureKey(payload) {
  if (payload.type === "listener_failed") return `listener:${payload.listener}:${payload.signal ?? ""}`;
  if (payload.type === "workstream_script_ran") return `script:${payload.workstream}:${payload.phase ?? ""}`;
  if (payload.type === "workstream_publish_failed") return `publish:${payload.workstream}:${payload.what ?? ""}`;
  return null;
}

/** A notice's body: who, and what they say — or who alone, when there is no more to say. */
function says(who, words) {
  return words ? t("shell-notifications-who-says", { who, words }) : String(who);
}

function remember(memory, gateId) {
  if (!gateId || memory.gates.includes(gateId)) return memory;
  return { gates: [...memory.gates, gateId].slice(-MEMORY_CAP) };
}

/**
 * A session moved: the notice, if any — with its category — and the memory
 * after it. The person's switches are not read here (`allowed` is the door's).
 * @param {object | null} prev the previous `SessionState`
 * @param {import("../types").SessionRow} row the row as it is now
 * @param {{gates: string[]}} memory
 */
export function onTransition(prev, row, memory) {
  const who = titleOf(row);
  const gateId = row.state?.state === "waiting" ? row.state.on?.gate_id ?? null : null;
  const next = remember(memory, gateId);
  if (!isNotifiable(prev, row.state)) return { notice: null, memory: next };
  const word = row.state.state;
  const category = categoryOfState(word);
  // The title says *waiting on you*: the body says who, and on what — in the
  // wait's own words (`waitWords`), never a label with its first words cut off.
  const notice =
    word === "waiting"
      ? { category, title: t("shell-notifications-bisa-waiting"), body: says(who, waitWords(row.state)) }
      : word === "failed"
        ? { category, title: t("shell-notifications-bisa-session-failed"), body: says(who, row.state.reason || t("ui-session-state-failed")) }
        : { category, title: t("shell-notifications-bisa-done"), body: t("shell-notifications-finished", { who }) };
  return { notice, memory: next };
}

/**
 * An engine frame arrived: the notice, if any — with its category — and the
 * memory after it. Only frames a session did not already announce produce one.
 * @param {any} payload an `EnginePayload`
 * @param {{gates: string[]}} memory
 */
export function onFrame(payload, memory) {
  const quiet = { notice: null, memory };
  if (!payload) return quiet;
  switch (payload.type) {
    case "gate_opened":
    case "question_asked": {
      if (memory.gates.includes(payload.gate_id)) return quiet;
      return {
        notice: {
          category: "asks",
          title: payload.type === "gate_opened" ? t("shell-notifications-bisa-waiting") : t("shell-notifications-bisa-question"),
          body: payload.question ?? payload.text ?? "",
        },
        memory: remember(memory, payload.gate_id),
      };
    }
    case "workflow_proposed":
      return { notice: { category: "workflows", title: t("shell-notifications-bisa-workflow-adopt"), body: t("shell-notifications-workflow-agent-proposed-workflow") }, memory };
    // A person creating a project sees the dialog on the spot; an agent's
    // refused settlement, or a project a step made, may happen with nobody
    // looking — so those two are the ones that reach the OS.
    case "committer_needed": {
      const unattended = payload.reason === "settlement_refused" || madeByAgentStep(payload.origin);
      if (!unattended) return quiet;
      return {
        notice: {
          category: "asks",
          title: t("shell-notifications-bisa-who-commits"),
          body:
            payload.reason === "settlement_refused"
              ? t("shell-notifications-agent-s-work-kept-uncommitted-until", { slug: payload.slug })
              : t("shell-notifications-created-workflow-step-nobody-set-commit", { slug: payload.slug }),
        },
        memory,
      };
    }
    // A failure nobody was looking at: a start event that could not start a
    // run, a workstream script that failed — a failure, like a session's.
    case "listener_failed":
    case "workstream_script_ran": {
      if (payload.type === "workstream_script_ran" && payload.ok !== false) return quiet;
      const key = failureKey(payload);
      if (key && memory.gates.includes(key)) return quiet;
      const notice =
        payload.type === "listener_failed"
          ? {
              category: "failures",
              title: t("shell-notifications-bisa-start-event-failed"),
              body: t("shell-notifications-start-event-could-not-start-run", { host: hostOf(payload.listener), error: String(payload.error ?? ""), flag: payload.error ? "yes" : "no" }),
            }
          : { category: "failures", title: t("shell-notifications-bisa-workstream-script-failed"), body: String(payload.output ?? "").split("\n")[0] || t("shell-notifications-workstream-script-failed") };
      return { notice, memory: remember(memory, key) };
    }
    // What the person approved and did not go out: they are not waiting on
    // it — they said yes and went on — so it is said to them.
    case "workstream_publish_failed": {
      const key = failureKey(payload);
      if (key && memory.gates.includes(key)) return quiet;
      const why = String(payload.reason ?? "").split("\n")[0];
      return {
        notice: { category: "failures", title: t("shell-notifications-bisa-publish-failed"), body: t("shell-notifications-could-not-publish", { what: String(payload.what ?? ""), why, flag: why ? "yes" : "no" }) },
        memory: remember(memory, key),
      };
    }
    case "guided":
      if (payload.status === "stalled" || payload.status === "failed") {
        return {
          notice: {
            category: "workflows",
            title: t("shell-notifications-bisa-design-needs"),
            body: payload.detail ?? t("shell-notifications-workflow-agent-could-design-workflow-retry"),
          },
          memory,
        };
      }
      return quiet;
    default:
      return quiet;
  }
}
