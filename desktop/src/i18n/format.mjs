/**
 * Numbers, dates and spans as the desktop says them, in the language
 * installed (17 — Internationalisation). Every digit is `Intl`'s in the
 * locale's shape; every word — *just now*, *Today*, the unit letters of a
 * span, *ago*, *in* — is a message of `locales/<lang>/desktop/format.ftl`.
 * Plain `.mjs`, so `node --test` reads it; a function takes `now` so a rule
 * is tested on a fixed clock.
 */

import { locale, t } from "./l10n.mjs";

const MINUTE = 60;
const HOUR = 3600;
const DAY = 86_400;
const WEEK = 7 * DAY;

/**
 * A number in the locale's shape — `1,234.5` in English.
 * @param {number} n
 * @param {Intl.NumberFormatOptions} [options]
 */
export function number(n, options) {
  return new Intl.NumberFormat(locale(), options).format(n);
}

/**
 * A ratio as a percentage: `0.34` → `34%`.
 * @param {number} ratio
 * @param {number} [digits] fraction digits, none by default
 */
export function percent(ratio, digits = 0) {
  return new Intl.NumberFormat(locale(), { style: "percent", maximumFractionDigits: digits, minimumFractionDigits: digits }).format(ratio);
}

const BYTE_UNITS = Object.freeze(["byte", "kilobyte", "megabyte", "gigabyte", "terabyte"]);

/**
 * A byte count scaled by 1024 to the unit that reads best, one decimal below
 * ten and none above — `1.5 kB`, `16 GB`; not a count is the empty string,
 * so a folder never claims to be zero bytes.
 * @param {unknown} bytes
 */
export function bytes(bytes) {
  if (typeof bytes !== "number" || !Number.isFinite(bytes) || bytes < 0) return "";
  let n = bytes;
  let u = 0;
  while (n >= 1024 && u < BYTE_UNITS.length - 1) {
    n /= 1024;
    u++;
  }
  const digits = u === 0 || n >= 10 ? 0 : 1;
  return new Intl.NumberFormat(locale(), { style: "unit", unit: BYTE_UNITS[u], unitDisplay: "short", maximumFractionDigits: digits, minimumFractionDigits: digits }).format(n);
}

/**
 * A moment in full — the tooltip behind a relative time.
 * @param {number} unixSeconds
 * @param {Intl.DateTimeFormatOptions} [options]
 */
export function dateTime(unixSeconds, options = { dateStyle: "medium", timeStyle: "short" }) {
  return new Intl.DateTimeFormat(locale(), options).format(new Date(unixSeconds * 1000));
}

/**
 * A short date — `Mar 3` — the answer once a moment is more than a week away.
 * @param {number} unixSeconds
 */
export function shortDate(unixSeconds) {
  return new Intl.DateTimeFormat(locale(), { month: "short", day: "numeric" }).format(new Date(unixSeconds * 1000));
}

/** The local calendar day a moment falls on — the key two messages share when they are the same day. @param {number} unixSeconds */
export function dayKey(unixSeconds) {
  return new Date(unixSeconds * 1000).toDateString();
}

/** The ISO stamp a `<time>` carries, or `null` for a moment the node did not give — `toISOString` is the one date call that throws. @param {unknown} unixSeconds */
export function isoOf(unixSeconds) {
  return typeof unixSeconds === "number" && Number.isFinite(unixSeconds) ? new Date(unixSeconds * 1000).toISOString() : null;
}

/** Whole seconds since a start, never negative — what a live counter shows. @param {number} sinceUnixSeconds @param {number} nowMs */
export function elapsedSince(sinceUnixSeconds, nowMs) {
  return Math.max(0, Math.floor(nowMs / 1000) - sinceUnixSeconds);
}

/**
 * How a divider names a day: *Today* and *Yesterday* by name — the two days
 * a conversation is usually read on are the two a date is least useful for —
 * and a short date otherwise, with the year only when it is not this one.
 * @param {number} unixSeconds
 * @param {Date} [now]
 */
export function dayLabel(unixSeconds, now = new Date()) {
  const d = new Date(unixSeconds * 1000);
  const key = dayKey(unixSeconds);
  if (key === now.toDateString()) return t("format-today");
  if (key === new Date(now.getTime() - DAY * 1000).toDateString()) return t("format-yesterday");
  return new Intl.DateTimeFormat(locale(), {
    weekday: "short",
    month: "short",
    day: "numeric",
    year: d.getFullYear() === now.getFullYear() ? undefined : "numeric",
  }).format(d);
}

const span = {
  seconds: (n) => t("format-span-seconds", { n }),
  minutes: (n) => t("format-span-minutes", { n }),
  hours: (n) => t("format-span-hours", { n }),
  days: (n) => t("format-span-days", { n }),
};

/**
 * How long something has been going, in the fewest characters that say it:
 * `12s`, `3m`, `1h 20m`, `2d 3h` — for a state still happening.
 * @param {number} seconds a non-negative span; anything else reads as `0s`
 */
export function duration(seconds) {
  const s = Number.isFinite(seconds) && seconds > 0 ? Math.floor(seconds) : 0;
  if (s < MINUTE) return span.seconds(s);
  const m = Math.floor(s / MINUTE);
  if (m < 60) return span.minutes(m);
  const h = Math.floor(m / 60);
  const rm = m % 60;
  if (h < 24) return rm === 0 ? span.hours(h) : `${span.hours(h)} ${span.minutes(rm)}`;
  const d = Math.floor(h / 24);
  const rh = h % 24;
  return rh === 0 ? span.days(d) : `${span.days(d)} ${span.hours(rh)}`;
}

/**
 * How long something took, keeping seconds where they matter: `45s`,
 * `2m 14s`, `1h 20m`, `2d 3h` — seconds are dropped once the span reaches an hour.
 * @param {number} seconds a non-negative span; anything else reads as `0s`
 */
export function durationPrecise(seconds) {
  const s = Number.isFinite(seconds) && seconds > 0 ? Math.floor(seconds) : 0;
  if (s < MINUTE) return span.seconds(s);
  const m = Math.floor(s / MINUTE);
  const rs = s % MINUTE;
  if (m < 60) return rs === 0 ? span.minutes(m) : `${span.minutes(m)} ${span.seconds(rs)}`;
  return duration(s);
}

/** The span word for a distance in seconds, under a week. @param {number} d */
function spanOf(d) {
  if (d < 90) return span.minutes(1);
  if (d < HOUR) return span.minutes(Math.round(d / MINUTE));
  if (d < DAY) return span.hours(Math.round(d / HOUR));
  return span.days(Math.round(d / DAY));
}

/**
 * How far away an instant is, in the fewest characters that say it — both
 * directions: *5m* for something that happened, *in 6h* for something that
 * will; inside 45 seconds either way is *just now*, so clock skew reads as a
 * moment ago rather than a countdown; past a week it is a date.
 * @param {number} unixSeconds
 * @param {number} [now] unix seconds
 */
export function relative(unixSeconds, now = Date.now() / 1000) {
  const delta = now - unixSeconds;
  const d = Math.abs(delta);
  if (d < 45) return t("format-just-now");
  if (d >= WEEK) return shortDate(unixSeconds);
  return t("format-relative", { direction: delta >= 0 ? "past" : "future", span: spanOf(d) });
}

/**
 * When something happened, as the tail of a sentence: *2h ago*, *just now*,
 * *on Mar 3* — one direction only: a stamp ahead of this clock is skew.
 * @param {number} unixSeconds
 * @param {number} [now] unix seconds
 */
export function ago(unixSeconds, now = Date.now() / 1000) {
  const delta = now - unixSeconds;
  if (delta < 45) return t("format-just-now");
  if (delta >= WEEK) return t("format-on-date", { date: shortDate(unixSeconds) });
  return t("format-ago", { span: spanOf(delta) });
}
