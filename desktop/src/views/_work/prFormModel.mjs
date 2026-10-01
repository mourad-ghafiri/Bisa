/**
 * The pull-request form, rendered from what the code host can do (ide/08)
 * — and from nothing else.
 *
 * `controlsFor(caps)` is the whole rule: a control appears when the code host has
 * the capability behind it and is **absent** otherwise — never greyed out. A
 * code host with every capability off yields title and body alone, which is the
 * test that makes "add a code host without touching the IDE" a property rather
 * than a hope. Plain JavaScript so `node --test` runs it without a build.
 */

import { failedCheck } from "./prReviewModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** Controls every code host has: a pull request is a title and a body at minimum. */
const ALWAYS = Object.freeze(["title", "body"]);

/**
 * @param {import("../../types").CodeHostCapabilities | null | undefined} caps
 * @returns {string[]} control ids, in display order
 */
export function controlsFor(caps) {
  const out = [...ALWAYS];
  if (!caps) return out;
  if (caps.draft_prs) out.push("draft");
  if (caps.reviewers) out.push("reviewers");
  if (caps.labels) out.push("labels");
  return out;
}

/**
 * Which merge strategies to offer, in the code host's order. Empty means the
 * code host cannot merge from here and the merge control is absent.
 * @param {import("../../types").CodeHostCapabilities | null | undefined} caps
 */
export function mergeStrategies(caps) {
  return caps?.merge_strategies ? [...caps.merge_strategies] : [];
}

/**
 * The strategy a merge control starts on: the project's `git.merge_strategy`
 * when the code host offers it, else the first the code host lists (�� the
 * setting is the one place a preference lives; nothing here prefers squash).
 * @param {import("../../types").CodeHostCapabilities | null | undefined} caps
 * @param {string | null | undefined} [preferred] the resolved `git.merge_strategy`
 */
export function defaultStrategy(caps, preferred) {
  const s = mergeStrategies(caps);
  if (s.length === 0) return null;
  return preferred && s.includes(preferred) ? preferred : s[0];
}

/**
 * What the dialog's primary button says. A pull request is opened on pushed
 * work, and the node pushes a branch that is not on the remote yet under the
 * same gate — so the button says both when both will happen.
 * @param {string} state the workstream's recorded state word (`open`, `committed`, …)
 */
export function prActionLabel(state, noun = t("work-publish-outcome-banner-pull-request")) {
  return state === "pushed" || state === "pr_open" ? t("work-pr-form-open-2", { noun }) : t("work-pr-form-push-open", { noun });
}

/**
 * Split a comma- or space-separated list of handles, trimming and deduping.
 * @param {string} text
 */
export function splitHandles(text) {
  const seen = new Set();
  return text
    .split(/[\s,]+/)
    .map((h) => h.trim().replace(/^@/, ""))
    .filter((h) => h && !seen.has(h) && seen.add(h));
}

/**
 * The request body for `POST /workstreams/{wid}/pr`, holding only what the
 * code host accepts — a field the form never showed is never sent.
 * @param {import("../../types").CodeHostCapabilities | null | undefined} caps
 * @param {{title: string, body: string, draft: boolean, reviewers: string, labels: string}} form
 */
export function prRequest(caps, form) {
  const controls = new Set(controlsFor(caps));
  /** @type {{title: string, body?: string, draft?: boolean, reviewers?: string[], labels?: string[]}} */
  const req = { title: form.title.trim() };
  if (form.body.trim()) req.body = form.body.trim();
  if (controls.has("draft") && form.draft) req.draft = true;
  if (controls.has("reviewers")) {
    const r = splitHandles(form.reviewers);
    if (r.length) req.reviewers = r;
  }
  if (controls.has("labels")) {
    const l = splitHandles(form.labels);
    if (l.length) req.labels = l;
  }
  return req;
}

/**
 * One line summarising check runs: `3 passed · 1 failed · 2 running`, or
 * `no checks`. The tone is `danger` with a failure, `dim` while running,
 * `ok` when everything passed.
 * @param {import("../../types").CheckRun[]} checks
 */
export function checksSummary(checks) {
  if (!checks || checks.length === 0) return { text: t("work-pr-form-no-checks"), tone: "dim" };
  let passed = 0;
  let failed = 0;
  let running = 0;
  let other = 0;
  for (const c of checks) {
    if (c.status !== "completed") running++;
    else if (c.conclusion === "success" || c.conclusion === "neutral" || c.conclusion === "skipped") passed++;
    else if (failedCheck(c)) failed++;
    else other++;
  }
  const parts = [];
  if (passed) parts.push(t("work-pr-form-passed", { passed }));
  if (failed) parts.push(t("work-pr-form-failed", { failed }));
  if (running) parts.push(t("work-pr-form-running", { running }));
  if (other) parts.push(t("work-pr-form-other", { other }));
  return { text: parts.join(" · "), tone: failed ? "danger" : running ? "dim" : "ok" };
}
