/**
 * A harness account's usage as words. The line reads its windows in the
 * adapter's order, each *label · bar · percent* — `5h ▰▰ 24% · resets in
 * 2 h 10 min  Weekly ▰▱ 41%  Fable ▱▱ 9%` — with the first (shortest)
 * window's reset inline and every other reset in its meter's tooltip; the
 * credit balance is set apart as an extra the compact footer line leaves
 * out; the tone is the tightest window's; a tooltip names the plan and the
 * source; and there is one honest sentence when there is no report. A failed
 * re-read keeps the last report for an hour (`settled`), as Claude Code's own
 * `/usage` does when its endpoint rate-limits it. `HarnessUsageLine.tsx`
 * draws — the footer's stat, its picker, Settings › Harnesses, the
 * Workstreams panel; the node reads; `node --test` checks.
 */

import { t } from "../i18n/l10n.mjs";
import { percent as formatPercent } from "../i18n/format.mjs";

/** The fold key a harness's usage line shares between every surface that folds it. */
export function usageKey(harness) {
  return `usage.${harness}`;
}

/** The tone a percentage earns: fine below 70, a caution below 90, danger from 90. */
export function usageTone(percent) {
  if (percent >= 90) return "danger";
  if (percent >= 70) return "warn";
  return "ok";
}

/** *resets in 2 h 10 min* · *resets in 3 d* · *resets soon* · null without a time. */
export function resetWords(resetsAt, now = Date.now() / 1000) {
  if (resetsAt == null) return null;
  const left = Math.floor(resetsAt - now);
  if (left <= 60) return t("shell-harness-usage-resets-soon");
  const d = Math.floor(left / 86_400);
  const h = Math.floor((left % 86_400) / 3_600);
  const m = Math.floor((left % 3_600) / 60);
  if (d >= 1) return h > 0 ? t("shell-harness-usage-resets-d-h", { d, h }) : t("shell-harness-usage-resets-d", { d });
  if (h >= 1) return m > 0 ? t("shell-harness-usage-resets-h-min", { h, m }) : t("shell-harness-usage-resets-h", { h });
  return t("shell-harness-usage-resets-min", { m });
}

/** How long a failed re-read keeps the last report: an hour, Claude Code's own rule. */
export const USAGE_KEEP_S = 3_600;

/** The window closest to its limit. */
function tightest(windows) {
  return windows.reduce((best, w) => (best === null || w.used_percent > best.used_percent ? w : best), null);
}

/**
 * @typedef {{id: string, label: string, percent: number, tone: "ok" | "warn" | "danger", scope: string | null, resets: string | null, tip: string}} Meter
 * @typedef {{meters: Meter[], extras: Meter[], inline: string | null, note: string | null, tone: "ok" | "warn" | "danger" | "quiet"}} UsageWords
 */

/**
 * One window as a meter: its rounded percentage, its tone, its own reset
 * words and its tooltip — *5h 24% · resets in 2 h 10 min*; a model's week
 * says so: *Fable this week 9% · resets in 3 d*.
 * @returns {Meter}
 */
function meterOf(w, now) {
  const percent = Math.round(w.used_percent);
  const resets = resetWords(w.resets_at ?? null, now);
  const scope = w.scope ?? null;
  const subject = scope && w.label === scope.split(" · ").at(-1) ? t("shell-harness-usage-week", { w: w.label }) : w.label;
  const at = t("shell-harness-usage-subject-percent", { subject, percent: formatPercent(percent / 100) });
  const tip = resets ? `${at} · ${resets}` : at;
  return { id: w.id, label: w.label, percent, tone: usageTone(w.used_percent), scope, resets, tip };
}

/**
 * What the line says for a node answer. A report is its windows as meters,
 * the credit balance apart, the first window's reset inline, the tightest
 * window's tone; anything else is one dim sentence.
 * @param {object | null | undefined} state the node's `UsageState`
 * @param {number} [now] unix seconds
 * @returns {UsageWords}
 */
export function usageWords(state, now = Date.now() / 1000) {
  if (!state) return { meters: [], extras: [], inline: null, note: t("shell-harness-usage-reading-usage"), tone: "quiet" };
  switch (state.state) {
    case "report": {
      const windows = state.report.windows ?? [];
      const meters = windows.map((w) => meterOf(w, now));
      const extras = (state.report.extras ?? []).map((w) => meterOf(w, now));
      const top = tightest(windows);
      return { meters, extras, inline: meters[0]?.resets ?? null, note: null, tone: top ? usageTone(top.used_percent) : "quiet" };
    }
    case "off":
      return { meters: [], extras: [], inline: null, note: t("shell-harness-usage-usage-reads-off-settings-harnesses"), tone: "quiet" };
    case "not_signed_in":
    case "unsupported":
      return { meters: [], extras: [], inline: null, note: state.reason, tone: "quiet" };
    default:
      return { meters: [], extras: [], inline: null, note: t("shell-harness-usage-usage-could-read", { reason: state.reason }), tone: "warn" };
  }
}

/**
 * The tooltip over a report: the plan, the login, the source, when it was
 * read — and, when the last re-read failed, that this report is the kept one
 * and why.
 * @param {object} report
 * @param {number} [now]
 * @param {string | null} [stale] the failed re-read's reason
 */
export function usageTitle(report, now = Date.now() / 1000, stale = null) {
  const parts = [];
  if (report.account?.plan) parts.push(t("shell-harness-usage-plan", { plan: report.account.plan }));
  if (report.account?.login) parts.push(report.account.login);
  parts.push(SOURCE_WORDS[report.source] ?? report.source);
  const ago = Math.max(0, Math.floor(now - report.read_at));
  parts.push(ago < 5 ? t("shell-harness-usage-read-just-now") : ago < 60 ? t("shell-harness-usage-read-s-ago", { ago }) : t("shell-harness-usage-read-min-ago", { min: Math.floor(ago / 60) }));
  if (stale) parts.push(t("shell-harness-usage-last-report-kept", { stale }));
  return parts.join(" · ");
}

const SOURCE_WORDS = Object.freeze({
  endpoint: t("shell-harness-usage-from-provider-s-usage-endpoint"),
  app_server: t("shell-harness-usage-from-harness-s-app-server"),
  cli: t("shell-harness-usage-from-harness-s-cli"),
});

/**
 * The store's one rule for an answer, whichever way a read fails: a failed
 * answer while the entry holds a report read within the hour keeps that
 * report and records why; any other answer — a report, *not signed in*,
 * *off*, *unsupported*, or a failure past the hour — replaces it.
 * @param {{ state: object | null, readAt: number }} previous
 * @param {object} answer the node's `UsageState`, or one made for an unreachable node
 * @param {number} now unix seconds
 * @returns {{ state: object, readAt: number, stale: string | null }}
 */
export function settled(previous, answer, now) {
  const kept = answer.state === "failed" && previous.state?.state === "report" && now - previous.readAt <= USAGE_KEEP_S;
  if (kept) return { state: previous.state, readAt: previous.readAt, stale: answer.reason };
  return { state: answer, readAt: now, stale: null };
}
