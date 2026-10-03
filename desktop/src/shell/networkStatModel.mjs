/**
 * The footer's network read-out, as facts — no React. One short word in the
 * bar — *DOWN* (red) while this Mac cannot reach the internet, else *VPN*
 * while a tunnel is up, else *UP*; a dash before the first read — a tone,
 * the sentence the tooltip carries, and the overlay's sections in order.
 * The proxy has no word of its own any more: it is in the sentence and the
 * overlay. The words are the ones Settings › Capabilities › Network says
 * (`views/_settings/networkModel.mjs`), so the footer and the panel never
 * disagree about the same Mac.
 *
 * `online` is the window's own word (`navigator.onLine`): off the shell it is
 * the only one; on it, *offline* beats a stale read — a window that says so
 * has no link at all — and *online* never beats the probe, since a link with
 * nothing behind it lies.
 */

import { dnsWords, inForceWords, interfaceRows, internetRows, internetWords, macProxyWords, routeWords, tunnelRows, unavailableWords, vpnWords } from "../views/_settings/networkModel.mjs";
import { t as tr } from "../i18n/l10n.mjs";

/** The words the bar can say, in the order they win; the dash is before the first read. */
export const VALUES = Object.freeze(["DOWN", "VPN", "UP", "—"]);

/**
 * The bar's word, its tone and the tooltip's sentence.
 * @param {import("./networkApi").NetworkFacts | null} facts
 * @param {import("../types").NetworkStatus | null} status
 * @param {boolean | null} online the window's `navigator.onLine`; `null` where there is no window
 */
export function statWords(facts, status, online) {
  const net = internetWords(facts, online);
  const vpn = facts ? vpnWords(facts) : null;
  const force = status ? inForceWords(status) : null;
  // The tooltip is pieces, each a message, joined the one typographic way.
  const proxy = force?.kind === "proxy" ? [tr("shell-network-stat-through-proxy", { sentence: force.sentence })] : [];
  if (net.kind === "down") {
    const tunnel = facts?.vpn.tunnels.find((t) => t.up);
    const title = tunnel ? tr("shell-network-stat-down-vpn-up-on", { sentence: net.sentence, interface: tunnel.interface }) : tr("shell-network-stat-down", { sentence: net.sentence });
    return { value: "DOWN", tone: "danger", title };
  }
  if (net.kind === "not-read") return { value: "—", tone: "quiet", title: tr("shell-network-stat-network-read-yet") };
  if (vpn?.kind === "up") {
    return { value: "VPN", tone: "ok", title: [tr("shell-network-stat-vpn-up-internet-up", { vpn: vpn.sentence, internet: net.sentence }), ...proxy].join(" · ") };
  }
  const vpnPart = vpn && vpn.kind !== "no-vpn" ? [tr("shell-network-stat-vpn-state", { label: vpn.label, sentence: vpn.sentence })] : facts ? [tr("shell-network-stat-vpn")] : [];
  return { value: "UP", tone: "ok", title: [tr("shell-network-stat-up", { sentence: net.sentence }), ...vpnPart, ...proxy].join(" · ") };
}

/**
 * The word the bar draws beside the glyph: only *VPN*, the one fact a dot
 * cannot say. Up and down are the dot's tone (ok, danger) and the dash is
 * its quiet; every value stays the accessible name and the tooltip's head.
 * @param {{ value: string }} words what `statWords` answered
 * @returns {string | null}
 */
export function barWord(words) {
  return words.value === "VPN" ? words.value : null;
}

/**
 * The overlay's sections, in order: the internet, the VPN, every interface
 * that is up, this Mac's route and resolver, the proxy System Settings
 * names, and what the platform leaves through. Off the shell and before a
 * read, the internet is the window's word and the Mac's sections are absent.
 * @param {import("./networkApi").NetworkFacts | null} facts
 * @param {import("../types").NetworkStatus | null} status
 * @param {boolean | null} online
 * @param {{ desktop: boolean }} where
 */
export function overlaySections(facts, status, online, where) {
  const sections = [];
  const net = internetWords(facts, online);
  if (!where.desktop) {
    sections.push({ key: "internet", title: tr("shell-network-stat-internet"), tone: net.tone, label: net.label, sentence: unavailableWords("not_desktop"), rows: [] });
  } else if (!facts) {
    sections.push({ key: "internet", title: tr("shell-network-stat-internet"), tone: net.tone, label: net.label, sentence: net.sentence, rows: [] });
  } else {
    sections.push({ key: "internet", title: tr("shell-network-stat-internet"), tone: net.tone, label: net.label, sentence: net.sentence, rows: net.kind === "up" ? internetRows(facts) : [] });
    const vpn = vpnWords(facts);
    sections.push({
      key: "vpn",
      title: "VPN",
      tone: vpn.tone,
      label: vpn.label,
      sentence: vpn.sentence,
      rows: facts.vpn.tunnels.filter((t) => t.up).flatMap((t) => tunnelRows(t, facts.internet.public_ip ?? null)),
    });
    const interfaces = interfaceRows(facts.interfaces);
    sections.push({ key: "interfaces", title: tr("shell-network-stat-interfaces"), sentence: interfaces.length > 0 ? null : tr("shell-network-stat-interface-up"), rows: interfaces });
    sections.push({
      key: "mac",
      title: tr("shell-network-stat-mac"),
      sentence: null,
      rows: [
        { label: tr("shell-network-stat-default-route"), value: routeWords(facts) },
        { label: "DNS", value: dnsWords(facts) },
      ],
    });
    sections.push({ key: "mac-proxy", title: tr("shell-network-stat-proxy-system-settings"), sentence: macProxyWords(facts.proxy).sentence, rows: [] });
  }
  if (!status) {
    sections.push({ key: "platform", title: tr("shell-network-stat-platform"), tone: "quiet", sentence: tr("shell-network-stat-what-platform-s-calls-leave-through"), rows: [] });
  } else {
    const force = inForceWords(status);
    sections.push({
      key: "platform",
      title: tr("shell-network-stat-platform"),
      tone: force.tone,
      label: force.label,
      sentence: force.sentence,
      rows: status.problems.map((p) => ({ label: tr("shell-network-stat-problem"), value: p })),
    });
  }
  return sections;
}

/** The overlay's footnote: the cadence, and when the last read landed. */
export function footnote(pollMs, readAt, now = Date.now() / 1000) {
  const every = tr("shell-network-stat-read-every", { s: Math.round(pollMs / 1000) });
  if (readAt == null) return tr("shell-network-stat-read-yet", { every });
  const ago = Math.max(0, Math.floor(now - readAt));
  return tr("shell-network-stat-read-s-ago", { every, ago });
}
