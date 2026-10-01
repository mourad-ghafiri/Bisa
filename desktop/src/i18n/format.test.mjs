/**
 * Digits are Intl's in the locale's shape, words are the catalog's; the
 * span rules on a fixed clock. Run with
 * `node --import ./src/i18n/preload.mjs --test desktop/src/i18n/format.test.mjs`
 * (or `npm test`, which preloads the English catalog).
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { ago, bytes, dateTime, dayKey, dayLabel, duration, durationPrecise, elapsedSince, isoOf, number, percent, relative, shortDate } from "./format.mjs";
import { installEnglish } from "./testing.mjs";

installEnglish();

test("numbers and percentages take the locale's shape", () => {
  assert.equal(number(1234.5), "1,234.5");
  assert.equal(percent(0.34), "34%");
  assert.equal(percent(0.3456, 1), "34.6%");
});

test("bytes scale by 1024 with one decimal below ten, and a non-count is nothing", () => {
  assert.equal(bytes(0), "0 byte");
  assert.equal(bytes(512), "512 byte");
  assert.equal(bytes(1536), "1.5 kB");
  assert.equal(bytes(16 * 1024 ** 3), "16 GB");
  assert.equal(bytes(1.2 * 1024 ** 3), "1.2 GB");
  assert.equal(bytes(-1), "");
  assert.equal(bytes("x"), "");
  assert.equal(bytes(undefined), "");
});

test("a span is the fewest unit letters that say it, from the catalog", () => {
  assert.equal(duration(0), "0s");
  assert.equal(duration(12), "12s");
  assert.equal(duration(180), "3m");
  assert.equal(duration(4800), "1h 20m");
  assert.equal(duration(3600), "1h");
  assert.equal(duration(2 * 86_400 + 3 * 3600), "2d 3h");
  assert.equal(duration(-5), "0s");
  assert.equal(durationPrecise(134), "2m 14s");
  assert.equal(durationPrecise(120), "2m");
  assert.equal(durationPrecise(4800), "1h 20m", "seconds drop past an hour");
});

test("a moment relative to now reads both directions and becomes a date past a week", () => {
  const now = 1_700_000_000;
  assert.equal(relative(now - 10, now), "just now");
  assert.equal(relative(now + 10, now), "just now");
  assert.equal(relative(now - 300, now), "5m");
  assert.equal(relative(now + 6 * 3600, now), "in 6h");
  assert.equal(relative(now - 2 * 86_400, now), "2d");
  assert.equal(relative(now - 60, now), "1m");
  assert.equal(relative(now - 8 * 86_400, now), shortDate(now - 8 * 86_400));
  assert.match(shortDate(now), /^[A-Z][a-z]{2} \d{1,2}$/);
});

test("ago reads one direction and names a date past a week", () => {
  const now = 1_700_000_000;
  assert.equal(ago(now - 7200, now), "2h ago");
  assert.equal(ago(now + 7200, now), "just now", "a stamp ahead of this clock is skew");
  assert.equal(ago(now - 8 * 86_400, now), `on ${shortDate(now - 8 * 86_400)}`);
});

test("a day is Today, Yesterday, or a short date with the year only when it differs", () => {
  const now = new Date(2026, 8, 23, 12);
  const at = (d) => Math.floor(d.getTime() / 1000);
  assert.equal(dayLabel(at(now), now), "Today");
  assert.equal(dayLabel(at(new Date(2026, 8, 22, 9)), now), "Yesterday");
  const sameYear = dayLabel(at(new Date(2026, 2, 3)), now);
  assert.ok(/Mar 3/.test(sameYear) && !/2026/.test(sameYear), sameYear);
  assert.ok(/2025/.test(dayLabel(at(new Date(2025, 2, 3)), now)));
  assert.match(dateTime(at(now)), /2026/);
});

test("the day key is the local calendar day, so two messages an hour apart across midnight are two days", () => {
  const at = (d) => Math.floor(d.getTime() / 1000);
  assert.equal(dayKey(at(new Date(2026, 8, 16, 1, 0, 0))), dayKey(at(new Date(2026, 8, 16, 23, 0, 0))));
  assert.notEqual(dayKey(at(new Date(2026, 8, 16, 23, 30, 0))), dayKey(at(new Date(2026, 8, 17, 0, 30, 0))));
});

test("the seconds↔milliseconds boundary lives here: an ISO stamp only for a real moment, a live counter never negative", () => {
  assert.equal(isoOf(0), "1970-01-01T00:00:00.000Z");
  assert.equal(isoOf(1_700_000_000), "2023-11-14T22:13:20.000Z");
  assert.equal(isoOf(Number.NaN), null, "a time the node did not give is a dash, not a thrown RangeError");
  assert.equal(isoOf(undefined), null);
  assert.equal(isoOf(Number.POSITIVE_INFINITY), null);
  assert.equal(elapsedSince(100, 160_999), 60, "whole seconds");
  assert.equal(elapsedSince(200, 100_000), 0, "a start in the future is not a negative age");
  assert.equal(typeof dateTime(1_700_000_000), "string");
});
