/**
 * A harness account's usage as a line's words. Run with
 * `node --test desktop/src/shell/harnessUsageModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { USAGE_KEEP_S, USAGE_RETRY_MS, resetWords, retryDelay, settled, usageKey, usageTitle, usageTone, usageWords } from "./harnessUsageModel.mjs";

const NOW = 1_000_000;
const window = (id, label, used, over = {}) => ({ id, label, used_percent: used, ...over });
const report = (windows, over = {}) => ({ state: "report", report: { harness: "claude-code", read_at: NOW - 40, source: "endpoint", windows, extras: [], ...over } });

test("the fold key is per harness and shared; a percentage earns its tone", () => {
  assert.equal(usageKey("claude-code"), "usage.claude-code");
  assert.deepEqual([0, 69.9, 70, 89.9, 90, 120].map(usageTone), ["ok", "ok", "warn", "warn", "danger", "danger"]);
});

test("a reset reads as time left, coarsely", () => {
  assert.equal(resetWords(null), null);
  assert.equal(resetWords(NOW + 30, NOW), "resets soon");
  assert.equal(resetWords(NOW + 25 * 60, NOW), "resets in 25 min");
  assert.equal(resetWords(NOW + 2 * 3_600 + 10 * 60, NOW), "resets in 2 h 10 min");
  assert.equal(resetWords(NOW + 3 * 3_600, NOW), "resets in 3 h");
  assert.equal(resetWords(NOW + 3 * 86_400 + 5 * 3_600, NOW), "resets in 3 d 5 h");
  assert.equal(resetWords(NOW + 3 * 86_400, NOW), "resets in 3 d");
});

test("a report is one meter per window in the adapter's order, each with its reset and tip, the credits apart, the first reset inline, the tightest tone", () => {
  const words = usageWords(
    report(
      [
        window("five_hour", "5h", 23.5, { resets_at: NOW + 7_800 }),
        window("seven_day", "Weekly", 91.4, { resets_at: NOW + 3 * 86_400 }),
        window("weekly:fable", "Fable", 8.5, { scope: "Fable", resets_at: NOW + 3 * 86_400 }),
      ],
      { extras: [window("extra_usage", "Credits", 3)] },
    ),
    NOW,
  );
  assert.deepEqual(
    words.meters.map((m) => [m.label, m.percent, m.tone, m.scope, m.resets, m.tip]),
    [
      ["5h", 24, "ok", null, "resets in 2 h 10 min", "5h 24% · resets in 2 h 10 min"],
      ["Weekly", 91, "danger", null, "resets in 3 d", "Weekly 91% · resets in 3 d"],
      ["Fable", 9, "ok", "Fable", "resets in 3 d", "Fable this week 9% · resets in 3 d"],
    ],
  );
  assert.deepEqual(
    words.extras.map((m) => [m.label, m.percent, m.tone, m.scope, m.resets, m.tip]),
    [["Credits", 3, "ok", null, null, "Credits 3%"]],
  );
  assert.equal(words.inline, "resets in 2 h 10 min", "the first window's reset, though the week is the tightest");
  assert.equal(words.tone, "danger");
  assert.equal(words.note, null);
  const scoped = usageWords(report([window("weekly:fable", "Fable", 8.5, { scope: "anthropic · Fable" })]), NOW);
  assert.equal(scoped.meters[0].tip, "Fable this week 9%", "a provider-scoped model week is still a week");
  const noReset = usageWords(report([window("five_hour", "5h", 10)]), NOW);
  assert.equal(noReset.inline, null);
  assert.equal(noReset.meters[0].tip, "5h 10%");
  assert.equal(noReset.tone, "ok");
});

test("everything but a report is one sentence", () => {
  assert.deepEqual(usageWords(null), { meters: [], extras: [], inline: null, note: "reading usage…", tone: "quiet" });
  assert.equal(usageWords({ state: "off" }).note, "usage reads are off — Settings › Capabilities › Harnesses");
  assert.equal(usageWords({ state: "unsupported", reason: "pi reports no usage limits — its provider does." }).note, "pi reports no usage limits — its provider does.");
  assert.equal(usageWords({ state: "not_signed_in", reason: "run `claude` and sign in" }).note, "run `claude` and sign in");
  const failed = usageWords({ state: "failed", reason: "the usage endpoint answered 503" });
  assert.equal(failed.note, "usage could not be read — the usage endpoint answered 503");
  assert.equal(failed.tone, "warn");
});

test("the tooltip names the plan, the login, the source, when it was read, and a kept report's reason", () => {
  const r = report([], { account: { plan: "max", login: "dev@example.com" } }).report;
  assert.equal(usageTitle(r, NOW), "plan max · dev@example.com · from the provider's usage endpoint · read 40 s ago");
  assert.equal(usageTitle({ ...r, account: null, source: "cli", read_at: NOW - 2 }, NOW), "from the harness's CLI · read just now");
  assert.equal(usageTitle({ ...r, account: null, source: "app_server", read_at: NOW - 400 }, NOW), "from the harness's app server · read 6 min ago");
  assert.equal(
    usageTitle({ ...r, account: null, read_at: NOW - 400 }, NOW, "the usage endpoint is rate limiting reads"),
    "from the provider's usage endpoint · read 6 min ago · the last report is kept — the usage endpoint is rate limiting reads",
  );
});

test("a failed re-read keeps the last report an hour, then shows; any other answer replaces it", () => {
  const kept = report([window("five_hour", "5h", 10)]);
  const failed = { state: "failed", reason: "the usage endpoint answered 503" };
  const half = settled({ state: kept, readAt: NOW - 1_800 }, failed, NOW);
  assert.deepEqual(half, { state: kept, readAt: NOW - 1_800, stale: "the usage endpoint answered 503" });
  const edge = settled({ state: kept, readAt: NOW - USAGE_KEEP_S }, failed, NOW);
  assert.equal(edge.stale, failed.reason, "the hour itself still keeps");
  const old = settled({ state: kept, readAt: NOW - USAGE_KEEP_S - 1 }, failed, NOW);
  assert.deepEqual(old, { state: failed, readAt: NOW, stale: null }, "past the hour the failure shows");
  assert.deepEqual(settled({ state: null, readAt: 0 }, failed, NOW), { state: failed, readAt: NOW, stale: null }, "nothing to keep");
  const fresh = report([window("five_hour", "5h", 12)]);
  assert.deepEqual(settled({ state: kept, readAt: NOW - 100 }, fresh, NOW), { state: fresh, readAt: NOW, stale: null }, "a report clears the reason");
  const signedOut = { state: "not_signed_in", reason: "run `claude` and sign in" };
  assert.deepEqual(settled({ state: kept, readAt: NOW - 100 }, signedOut, NOW), { state: signedOut, readAt: NOW, stale: null }, "only a failure is kept over");
});

test("a read that failed with nothing kept is asked again at 15 s, 30 s, 60 s, then left to the poll; an answer, or a kept report, ends the retries", () => {
  assert.deepEqual([...USAGE_RETRY_MS], [15_000, 30_000, 60_000]);
  const failed = { state: { state: "failed", reason: "the node could not be reached" }, stale: null };
  assert.deepEqual([0, 1, 2, 3].map((n) => retryDelay(failed, n)), [15_000, 30_000, 60_000, null]);
  assert.equal(retryDelay({ state: report([window("five_hour", "5h", 10)]), stale: "the usage endpoint answered 503" }, 0), null, "a kept report is an answer for the hour");
  assert.equal(retryDelay({ state: report([window("five_hour", "5h", 10)]), stale: null }, 0), null);
  assert.equal(retryDelay({ state: { state: "not_signed_in", reason: "run `claude`" }, stale: null }, 0), null, "not signed in is an answer");
  assert.equal(retryDelay({ state: { state: "off" }, stale: null }, 0), null);
  assert.equal(retryDelay({ state: null, stale: null }, 0), null, "nothing read yet is the first read's");
});

test("the store asks the source again once a refresh pressed mid-read lands, rather than dropping the press, reads again when the bus comes back, and retries a failure on the model's backoff", async () => {
  const { readFileSync } = await import("node:fs");
  const store = readFileSync(new URL("./harnessUsageStore.ts", import.meta.url), "utf8");
  assert.ok(store.includes("if (refresh) again.add(id);"), "a refresh asked while a read is out is remembered");
  assert.ok(store.includes("if (again.delete(id)) void read(id, true);"), "and asked once the read lands");
  assert.ok(store.includes("reloadOnReconnect(watchConnection, "), "a page that opened while the node was down reads again when it comes up");
  assert.ok(store.includes("retryDelay(entries[id] ?? EMPTY, attempts.get(id) ?? 0)"), "a failure is retried as the model says");
});
