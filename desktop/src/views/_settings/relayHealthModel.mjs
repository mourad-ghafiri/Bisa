/**
 * A relay's health, as words and a tone (14-collaboration): what the
 * Relays & sync panel's row shows for each relay the node reads, from the
 * pool's own numbers (`GET /sync`). Nothing here fetches.
 */

import { t } from "../../i18n/l10n.mjs";

/** The dot's tone and the word for a relay's status — the pool's status folded to what a person acts on, said in the catalog's words. */
export function relayTone(relay) {
  const as = (tone, state) => ({ tone, word: t("settings-relay-health-state", { state }) });
  if (!relay) return as("quiet", "unknown");
  if (relay.connected) return as("ok", "connected");
  switch (relay.status) {
    case "off":
      return as("quiet", "off");
    case "connecting":
    case "pending":
    case "initialized":
      return as("warn", "connecting");
    case "sleeping":
      return as("quiet", "idle");
    case "banned":
      return as("danger", "refused");
    case "terminated":
    case "shutdown":
      return as("danger", "closed");
    default:
      return as("danger", "disconnected");
  }
}

/** The row's second line: latency, success rate, bytes — what the relay proved. */
export function relayWords(relay, now = Date.now() / 1000) {
  if (!relay) return "";
  const parts = [];
  if (relay.latency_ms != null) parts.push(t("settings-relay-health-ms", { latency_ms: relay.latency_ms }));
  if (relay.attempts > 0) parts.push(t("settings-relay-health-attempt-attempts", { success_rate: Math.round(relay.success_rate * 100), attempts: relay.attempts }));
  if (relay.connected && relay.connected_at) {
    const secs = Math.max(0, Math.floor(now - relay.connected_at));
    parts.push(t("settings-relay-health-up-since", { since: sinceWords(secs) }));
  }
  const bytes = (relay.bytes_sent ?? 0) + (relay.bytes_received ?? 0);
  if (bytes > 0) parts.push(t("settings-relay-health-moved", { bytes: sizeWords(bytes) }));
  return parts.join(" · ");
}

/**
 * Why a relay is not connected, as the node says it (`RelayHealth.problem` —
 * `bisa_collab::relay_problem`): the row's third line, shown whole rather
 * than truncated, since it is the one line a person acts on. Empty while the
 * relay is connected, off, or not tried yet.
 */
export function relayProblem(relay) {
  if (!relay || relay.connected) return "";
  return typeof relay.problem === "string" ? relay.problem.trim() : "";
}

function sinceWords(s) {
  if (s < 60) return `${s} s`;
  const min = Math.floor(s / 60);
  if (min < 60) return t("settings-relay-health-min", { min });
  const h = Math.floor(min / 60);
  if (h < 24) return t("settings-relay-health-hours", { h });
  return t("settings-relay-health-days", { d: Math.floor(h / 24) });
}

function sizeWords(n) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

/** What a check answered, in words and a tone. */
export function checkWords(check) {
  if (!check) return null;
  if (check.ok) return { tone: "ok", text: check.latency_ms != null ? t("settings-relay-health-reachable-answered-ms", { latency_ms: check.latency_ms }) : t("settings-relay-health-reachable-3") };
  return { tone: "danger", text: check.error ? t("settings-relay-health-reachable", { error: check.error }) : t("settings-relay-health-reachable-2") };
}

/**
 * The check's answer the *Add a relay* field may draw: the one about the
 * address it holds now. An answer that lands after the field moved on — a
 * check is a dial, and takes its time — is about an address nobody is
 * looking at, and *reachable* under another would be a lie.
 * @template {{url: string}} C
 * @param {C | null | undefined} check
 * @param {string | null | undefined} url the address the field holds, when it is one that may be added
 * @returns {C | null}
 */
export function checkAbout(check, url) {
  return check && url && check.url === url ? check : null;
}

/**
 * Whether a typed URL may be added: a `ws(s)://` URL with a host, not
 * already in the list.
 * @param {string} raw
 * @param {readonly string[]} relays
 */
export function relayEntry(raw, relays) {
  const url = String(raw ?? "").trim();
  if (!url) return { ok: false, reason: "" };
  if (!/^wss?:\/\/[^\s/]+/i.test(url)) return { ok: false, reason: t("settings-relay-health-relay-ws-wss-url") };
  if ((relays ?? []).includes(url)) return { ok: false, reason: t("settings-relay-health-already-configured") };
  return { ok: true, reason: "", url };
}

/**
 * Whether an engine fact moved the wire — the relays, or a `sync.*` setting
 * (the switch that turns it on or off among them) — so whoever shows the
 * wire reads it again: the Relays & sync panel, People's *relays are off*
 * line, the footer's node read-out. One rule, so none of the three is left
 * saying *off* after the switch went on.
 * @param {{type?: string, keys?: readonly string[]} | null | undefined} payload an engine frame's payload
 */
export function wireMoved(payload) {
  if (payload?.type === "relays_changed") return true;
  return payload?.type === "settings_changed" && (payload.keys ?? []).some((k) => typeof k === "string" && k.startsWith("sync."));
}

/** The sentence for the switch being off, said the same everywhere. */
export const OFF_LINE = t("settings-relay-health-relays-off-turn-them-host-anyone");

/** The wire, summed up for the panel's tiles and the footer. */
export function syncSummary(report) {
  if (!report || !report.running) return { running: false, enabled: !!report?.enabled, line: t("settings-relay-health-collaboration-pump-runs-node"), tone: "quiet" };
  if (!report.enabled) return { running: true, enabled: false, line: OFF_LINE, tone: "quiet" };
  const n = report.relays.length;
  const c = report.connected_relays;
  const line = n === 0 ? t("settings-relay-health-relay-configured-node-reaches-nobody") : t("settings-relay-health-relay-relays-connected", { c, n });
  return { running: true, enabled: true, line, tone: n === 0 ? "quiet" : c > 0 ? "ok" : "danger" };
}

/**
 * What a round of checks answered, in one line — the footer's and the
 * panel's word after *Check all*. `null` before any check.
 * @param {readonly { url: string, ok: boolean }[] | null | undefined} checks
 */
export function checkAllWords(checks) {
  if (!checks || checks.length === 0) return null;
  const ok = checks.filter((c) => c.ok).length;
  const n = checks.length;
  if (ok === n) return { tone: "ok", text: n === 1 ? t("settings-relay-health-relay-reachable") : t("settings-relay-health-all-relays-reachable", { n }) };
  if (ok === 0) return { tone: "danger", text: n === 1 ? t("settings-relay-health-relay-reachable-2") : t("settings-relay-health-none-relays-reachable", { n }) };
  return { tone: "warn", text: t("settings-relay-health-relays-reachable", { ok, n }) };
}

/**
 * The registry's default relays, read from the `sync.relays` definition —
 * the one place they are written — and whether the list as it stands is
 * that default, so *Reset to the defaults* is offered only when it would
 * change something.
 * @param {readonly { key: string, default?: unknown }[] | null | undefined} defs
 */
export function defaultRelays(defs) {
  const d = (defs ?? []).find((x) => x.key === "sync.relays")?.default;
  return Array.isArray(d) ? d.filter((x) => typeof x === "string") : [];
}

export function isDefaultList(relays, defaults) {
  const a = relays ?? [];
  const b = defaults ?? [];
  return a.length === b.length && a.every((r, i) => r === b[i]);
}
