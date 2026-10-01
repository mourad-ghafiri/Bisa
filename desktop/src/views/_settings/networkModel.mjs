/**
 * The words of Settings › Capabilities › Network (ide/13), and of the
 * footer's network read-out: how the platform reaches the internet — the
 * proxy mode, what is in force, the one check — and what this Mac's network
 * is — whether the internet is reachable and by which public IP, every
 * interface that is up, the VPN, its tunnel, the route, the resolvers, the
 * proxy System Settings names. Pure, so `node --test` reads them; the modes
 * are the registry's (`network.proxy.mode`), the facts the shell's
 * (`src-tauri/src/network.rs`), the status the node's (`GET /network`).
 */

import { t as tr } from "../../i18n/l10n.mjs";

/** The proxy modes, in the registry's order — `network.proxy.mode`'s choices. */
export const MODES = Object.freeze(["environment", "none", "manual"]);

/** The registry keys the panel writes, spelled once. */
export const KEYS = Object.freeze({
  mode: "network.proxy.mode",
  http: "network.proxy.http",
  https: "network.proxy.https",
  noProxy: "network.proxy.no_proxy",
  http1Only: "network.http1_only",
  publicIpUrl: "network.public_ip_url",
});

/** The text settings a password may ride in — a proxy URL's userinfo: typed into the kit's secret field, hidden by default (ide/13 §Secret fields). */
export const SECRET_SETTING_KEYS = Object.freeze([KEYS.http, KEYS.https]);

/** Whether a text setting is typed hidden. */
export function isSecretSetting(key) {
  return SECRET_SETTING_KEYS.includes(key);
}

/** The URL the check offers first: a code host every workspace reaches. */
export const CHECK_URL_DEFAULT = "https://api.github.com";

/** The echo service the registry offers first — held equal to the key's default in `settings.rs`. */
export const PUBLIC_IP_URL_DEFAULT = "https://api.ipify.org";

/** The interface kinds' words — the shell's `InterfaceKind`, one word each. */
export const KIND_WORDS = Object.freeze({ wifi: "Wi-Fi", ethernet: tr("settings-network-ethernet"), tunnel: "tunnel", other: "other" });

/** The provider words the shell's table says — held equal to `PROVIDERS` in `network.rs`. */
export const PROVIDERS = Object.freeze([
  "Tailscale",
  "WireGuard",
  "OpenVPN",
  tr("settings-network-cisco-secure-client"),
  "GlobalProtect",
  tr("settings-network-cloudflare-warp"),
  "Zscaler",
  "Mullvad",
  "NordVPN",
  "ExpressVPN",
  tr("settings-network-proton-vpn"),
  "Viscosity",
  "Tunnelblick",
]);

const MODE_LABEL = Object.freeze({ environment: tr("settings-mcp-panel-environment"), none: tr("settings-network-none"), manual: tr("settings-network-manual") });

/** The three-way switch's segments. */
export function modeSegments() {
  return MODES.map((id) => ({ id, label: MODE_LABEL[id] }));
}

/**
 * What a mode means, under the switch.
 * @param {"environment" | "none" | "manual"} mode
 */
export function modeWords(mode) {
  switch (mode) {
    case "none":
      return tr("settings-network-proxy-whatever-node-s-environment-says");
    case "manual":
      return tr("settings-network-urls-bypass-below-platform-s-own");
    default:
      return tr("settings-network-proxy-node-s-own-environment-names");
  }
}

/**
 * The words for `GET /network`'s in-force line: a chip and a sentence.
 * @param {{ mode: string, in_force: { kind: string, http?: string | null, https?: string | null, no_proxy?: string | null }, environment: { http?: string | null, https?: string | null, no_proxy?: string | null }, http1_only: boolean }} status
 */
export function inForceWords(status) {
  const version = status.http1_only ? ` ${tr("settings-network-http1-only")}` : "";
  if (status.in_force.kind === "direct") {
    const why =
      status.mode === "environment"
        ? ` ${tr("settings-network-node-s-environment-names-proxy")}`
        : status.mode === "manual"
          ? ` ${tr("settings-network-manual-proxy-names-url")}`
          : "";
    return { kind: "direct", tone: "quiet", label: tr("settings-network-direct"), sentence: tr("settings-network-every-call-leaves-machine-without-proxy", { why, version }) };
  }
  const via = [
    status.in_force.https ? tr("settings-network-https-through", { https: status.in_force.https }) : null,
    status.in_force.http ? tr("settings-network-http-through", { http: status.in_force.http }) : null,
  ].filter(Boolean);
  const bypass = status.in_force.no_proxy ? ` ${tr("settings-network-bypassing", { no_proxy: status.in_force.no_proxy })}` : "";
  const inForce = status.mode === "environment" ? tr("settings-network-proxy-in-force-from-environment", { via: via.join(", ") }) : tr("settings-network-proxy-in-force", { via: via.join(", ") });
  return { kind: "proxy", tone: "ok", label: tr("settings-network-through-proxy"), sentence: `${inForce}${bypass}${version}` };
}

/**
 * The check's answer as a chip and a sentence.
 * @param {{ url: string, ok: boolean, status?: number | null, elapsed_ms: number, error?: string | null, via_proxy: boolean } | null} result
 */
export function checkWords(result) {
  if (!result) return null;
  const way = result.via_proxy ? tr("settings-network-through-proxy-2") : tr("settings-network-directly");
  if (result.ok) {
    return { kind: "reachable", tone: "ok", label: tr("settings-network-reachable-word"), sentence: tr("settings-network-answered-ms", { url: result.url, status: result.status ?? "—", elapsed_ms: result.elapsed_ms, way }) };
  }
  return { kind: "not-reachable", tone: "warn", label: tr("settings-network-reachable"), sentence: tr("settings-network-did-not-answer", { url: result.url, way, error: result.error ?? tr("settings-network-request-failed") }) };
}

/**
 * Whether the internet is reachable: a chip and a sentence. `online` is the
 * window's own word (`navigator.onLine`) — the fallback without facts, and
 * the one thing that beats them: a window that says offline has no link at
 * all, while one that says online may have a link with nothing behind it.
 * @param {import("../../shell/networkApi").NetworkFacts | null} facts
 * @param {boolean | null} [online]
 */
export function internetWords(facts, online = null) {
  if (online === false) return { kind: "down", tone: "danger", label: tr("settings-network-down"), sentence: tr("settings-network-window-reports-network") };
  if (!facts) {
    if (online === true) return { kind: "up", tone: "ok", label: tr("settings-network-up"), sentence: tr("settings-network-window-reports-network-mac-s-network") };
    return { kind: "not-read", tone: "quiet", label: tr("settings-code-host-panel-read"), sentence: tr("settings-network-mac-s-internet-has-been-read") };
  }
  const net = facts.internet;
  if (!net.up) return { kind: "down", tone: "danger", label: tr("settings-network-down"), sentence: tr("settings-network-route-internet-internet", { error: net.error, flag: (net.error) ? "yes" : "no" }) };
  const reached = net.probe.tcp != null ? tr("settings-network-reached-ms", { tcp: net.probe.tcp }) : tr("settings-network-reached");
  return { kind: "up", tone: "ok", label: tr("settings-network-up"), sentence: tr("settings-network-internet-up", { reached, dns: net.probe.dns === false ? "failing" : "fine", public_ip: net.public_ip ?? tr("settings-code-host-panel-read") }) };
}

/**
 * The internet's rows: the public IP with the echo's time, the connect's
 * time, whether DNS answered.
 * @param {import("../../shell/networkApi").NetworkFacts} facts
 */
export function internetRows(facts) {
  const net = facts.internet;
  const ip = net.public_ip ? (net.probe.fetch != null ? tr("settings-network-ms", { public_ip: net.public_ip, fetch: net.probe.fetch }) : net.public_ip) : tr("settings-code-host-panel-read");
  return [
    { label: tr("settings-network-public-ip"), value: ip },
    { label: tr("settings-network-reached"), value: net.probe.tcp != null ? tr("settings-network-ms-2", { tcp: net.probe.tcp }) : "—" },
    { label: "DNS", value: net.probe.dns == null ? tr("settings-network-asked") : net.probe.dns ? "answers" : tr("settings-network-does-answer") },
  ];
}

/**
 * One row per interface that is up — `en0` → `Wi-Fi · 192.168.1.20 · via
 * 192.168.1.1 · default route` — the one carrying the default route first.
 * @param {readonly import("../../shell/networkApi").InterfaceFacts[]} interfaces
 */
export function interfaceRows(interfaces) {
  const ordered = [...interfaces].sort((a, b) => Number(b.default_route) - Number(a.default_route));
  return ordered.map((i) => ({
    label: i.name,
    value: [
      i.label ?? KIND_WORDS[i.kind] ?? i.kind,
      i.addresses.length > 0 ? i.addresses.join(", ") : tr("settings-network-address"),
      i.gateway ? tr("settings-network-via-gateway", { gateway: i.gateway }) : null,
      i.default_route ? tr("settings-network-default-route") : i.routes > 0 ? tr("settings-network-route-routes", { routes: i.routes }) : null,
    ]
      .filter(Boolean)
      .join(" · "),
  }));
}

/**
 * The VPN's standing: a chip and a sentence.
 * @param {import("../../shell/networkApi").NetworkFacts | null} facts
 */
export function vpnWords(facts) {
  if (!facts) return { kind: "not-read", tone: "quiet", label: tr("settings-code-host-panel-read"), sentence: tr("settings-network-mac-s-network-has-been-read") };
  const up = facts.vpn.tunnels.filter((t) => t.up);
  if (up.length > 0) {
    const t = up[0];
    const who = t.provider ?? t.protocol;
    const route = t.default_route ? tr("settings-network-all-traffic") : t.routes > 0 ? tr("settings-network-route-routes", { routes: t.routes }) : tr("settings-network-routes-yet");
    const more = up.length > 1 ? tr("settings-network-more-tunnels", { more: up.length - 1 }) : "";
    return { kind: "up", tone: "ok", label: tr("settings-network-up"), sentence: tr("settings-network-words", { who, interface: t.interface, route, more }) };
  }
  const connected = facts.vpn.services.filter((s) => s.connected);
  if (connected.length > 0) {
    return { kind: "up", tone: "ok", label: tr("settings-network-up"), sentence: tr("settings-network-services-connected-no-tunnel", { services: connected.map((s) => `${s.name} (${s.kind})`).join(", ") }) };
  }
  if (facts.vpn.services.length > 0) {
    return {
      kind: "not-connected",
      tone: "neutral",
      label: tr("settings-network-connected"),
      sentence: tr("settings-network-vpn-set-up-not-connected", { services: facts.vpn.services.map((s) => `${s.name} (${s.kind})`).join(", ") }),
    };
  }
  return { kind: "no-vpn", tone: "quiet", label: tr("settings-network-vpn"), sentence: tr("settings-network-vpn-up-mac-system-settings-knows") };
}

/**
 * One tunnel's facts as rows — a row only when the fact is there. The
 * public IP joins when all traffic leaves by the tunnel: the address the
 * world sees is then the tunnel's exit.
 * @param {import("../../shell/networkApi").TunnelFacts} t
 * @param {string | null} [publicIp]
 */
export function tunnelRows(t, publicIp = null) {
  const rows = [
    { label: tr("settings-network-interface"), value: t.interface },
    { label: tr("settings-network-protocol"), value: t.protocol },
  ];
  if (t.provider) rows.push({ label: tr("settings-network-provider"), value: t.provider });
  if (t.addresses.length > 0) rows.push({ label: t.addresses.length === 1 ? tr("settings-network-address-2") : tr("settings-network-addresses"), value: t.addresses.join(", ") });
  if (t.peer && t.peer !== t.addresses[0]) rows.push({ label: tr("settings-network-peer"), value: t.peer });
  if (typeof t.mtu === "number") rows.push({ label: "MTU", value: String(t.mtu) });
  rows.push({ label: tr("settings-network-route"), value: t.default_route ? tr("settings-network-all-traffic-default-route-leaves") : t.routes > 0 ? tr("settings-network-split-route-routes-through", { routes: t.routes }) : tr("settings-network-none-through") });
  if (t.default_route && publicIp) rows.push({ label: tr("settings-network-public-ip"), value: publicIp });
  if (t.dns.length > 0) rows.push({ label: "DNS", value: t.dns.join(", ") });
  if (t.search.length > 0) rows.push({ label: tr("settings-network-search-domains"), value: t.search.join(", ") });
  return rows;
}

/**
 * What the machine asks for names.
 * @param {import("../../shell/networkApi").NetworkFacts} facts
 */
export function dnsWords(facts) {
  if (facts.dns.servers.length === 0) return tr("settings-network-resolver-configured");
  const where = facts.dns.interface ? ` ${tr("settings-network-on-interface", { interface: facts.dns.interface })}` : "";
  const search = facts.dns.search.length > 0 ? `; ${tr("settings-network-search-domains-list", { domains: facts.dns.search.join(", ") })}` : "";
  return `${facts.dns.servers.join(", ")}${where}${search}.`;
}

/**
 * Where the machine's traffic leaves by.
 * @param {import("../../shell/networkApi").NetworkFacts} facts
 */
export function routeWords(facts) {
  if (!facts.default_route) return tr("settings-network-default-route-mac-offline");
  return tr("settings-network-via", { interface: facts.default_route.interface, gateway: facts.default_route.gateway });
}

/**
 * The proxy System Settings names: a sentence, and whether the platform
 * could follow it (a PAC file or auto-discovery is a script, not a URL).
 * @param {import("../../shell/networkApi").MacProxy} proxy
 */
export function macProxyWords(proxy) {
  const parts = [];
  if (proxy.http) parts.push(tr("settings-network-http", { http: proxy.http }));
  if (proxy.https) parts.push(proxy.https === proxy.http ? tr("settings-network-https-same") : tr("settings-network-https", { https: proxy.https }));
  if (proxy.socks) parts.push(tr("settings-network-socks", { socks: proxy.socks }));
  const followable = Boolean(proxy.http || proxy.https);
  if (proxy.pac_url) parts.push(tr("settings-network-pac-file-script-platform-cannot-follow", { pac_url: proxy.pac_url }));
  if (proxy.auto_discovery) parts.push(tr("settings-network-automatic-discovery-something-platform-can-follow"));
  if (parts.length === 0) return { sentence: tr("settings-network-system-settings-names-proxy"), followable: false };
  const exceptions = proxy.exceptions.length > 0 ? ` ${tr("settings-network-bypassing-2", { exceptions: proxy.exceptions.join(", ") })}` : "";
  return { sentence: `${parts.join(" · ")}.${exceptions}`, followable };
}

/**
 * What *Use in Bisa* writes at machine scope: the manual mode, the two
 * URLs from the Mac's host:port pairs, the exceptions as the bypass. `null`
 * when System Settings names nothing the platform can follow.
 * @param {import("../../shell/networkApi").MacProxy} proxy
 */
export function manualFrom(proxy) {
  if (!proxy.http && !proxy.https) return null;
  const url = (hostPort) => (hostPort ? `http://${hostPort}` : "");
  return {
    [KEYS.mode]: "manual",
    [KEYS.http]: url(proxy.http),
    [KEYS.https]: url(proxy.https ?? proxy.http),
    [KEYS.noProxy]: proxy.exceptions.join(", "),
  };
}

/**
 * Why the facts cannot be read here, in one sentence.
 * @param {"not_desktop" | "no_reader" | string} reason
 */
export function unavailableWords(reason) {
  switch (reason) {
    case "not_desktop":
      return tr("settings-network-mac-s-network-read-desktop-app");
    case "no_reader":
      return tr("settings-network-desktop-app-has-network-reader-system");
    default:
      return reason;
  }
}
