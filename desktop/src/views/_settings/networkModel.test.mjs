/**
 * The Network panel's words agree with the registry, the shell and the node,
 * and say what they owe. Run with `node --test desktop/src/views/_settings/networkModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  CHECK_URL_DEFAULT,
  KEYS,
  SECRET_SETTING_KEYS,
  isSecretSetting,
  KIND_WORDS,
  MODES,
  PROVIDERS,
  PUBLIC_IP_URL_DEFAULT,
  checkWords,
  dnsWords,
  inForceWords,
  interfaceRows,
  internetRows,
  internetWords,
  macProxyWords,
  manualFrom,
  modeSegments,
  modeWords,
  routeWords,
  tunnelRows,
  unavailableWords,
  vpnWords,
} from "./networkModel.mjs";

const registry = readFileSync(new URL("../../../../crates/bisa-core/src/settings.rs", import.meta.url), "utf8");
const shell = readFileSync(new URL("../../../src-tauri/src/network.rs", import.meta.url), "utf8");

test("the modes are the registry's choices, in its order, and every key is registered", () => {
  const at = registry.indexOf(`"${KEYS.mode}",`);
  assert.ok(at > 0, "the mode key is registered");
  const choice = registry.slice(at, registry.indexOf("]),", at) + 3).match(/Choice\(&\[([^\]]*)\]\)/);
  assert.ok(choice, "the mode is a choice");
  assert.deepEqual([...MODES], choice[1].split(",").map((w) => w.trim().replace(/"/g, "")).filter(Boolean));
  for (const key of Object.values(KEYS)) assert.ok(registry.includes(`"${key}",`), `${key} is a registry key`);
  assert.deepEqual([...SECRET_SETTING_KEYS], [KEYS.http, KEYS.https], "a proxy URL may carry a password: typed hidden");
  assert.ok(isSecretSetting(KEYS.https) && !isSecretSetting(KEYS.mode) && !isSecretSetting("network.public_ip_url"));
  assert.deepEqual(modeSegments().map((s) => s.id), [...MODES]);
  for (const mode of MODES) assert.ok(modeWords(mode).length > 20, `${mode} has its sentence`);
  assert.equal(KEYS.publicIpUrl, "network.public_ip_url");
  const echo = registry.indexOf(`"${KEYS.publicIpUrl}",`);
  assert.ok(registry.slice(echo, echo + 400).includes(`json!("${PUBLIC_IP_URL_DEFAULT}")`), "the default echo service is the registry's");
});

test("the provider words are the shell's table, one per daemon", () => {
  const start = shell.indexOf("pub const PROVIDERS");
  const block = shell.slice(start, shell.indexOf("];", start));
  const words = [...block.matchAll(/\("[^"]+", "([^"]+)"\)/g)].map((m) => m[1]);
  assert.ok(words.length >= 10, "the table was read");
  assert.deepEqual([...PROVIDERS], [...new Set(words)]);
});

test("in force reads direct or through a proxy, names the environment as the source, and says HTTP/1.1", () => {
  const direct = inForceWords({ mode: "environment", in_force: { kind: "direct" }, environment: {}, http1_only: false });
  assert.equal(direct.label, "direct");
  assert.match(direct.sentence, /environment names no proxy/);
  const manualEmpty = inForceWords({ mode: "manual", in_force: { kind: "direct" }, environment: {}, http1_only: true });
  assert.match(manualEmpty.sentence, /names no URL/);
  assert.match(manualEmpty.sentence, /HTTP\/1\.1/);
  const via = inForceWords({
    mode: "manual",
    in_force: { kind: "proxy", https: "http://ada:•••@proxy.example:3128", http: null, no_proxy: "localhost,127.0.0.1,::1,.corp.example" },
    environment: {},
    http1_only: false,
  });
  assert.equal(via.label, "through a proxy");
  assert.match(via.sentence, /HTTPS through http:\/\/ada:•••@proxy.example:3128/);
  assert.match(via.sentence, /Bypassing localhost/);
  const env = inForceWords({ mode: "environment", in_force: { kind: "proxy", http: "http://p:1", https: "http://p:1" }, environment: { http: "http://p:1" }, http1_only: false });
  assert.match(env.sentence, /the node's environment names it/);
});

test("the check answers in words", () => {
  assert.equal(checkWords(null), null);
  const ok = checkWords({ url: "https://api.github.com/", ok: true, status: 200, elapsed_ms: 180, error: null, via_proxy: true });
  assert.equal(ok.label, "reachable");
  assert.match(ok.sentence, /200 in 180 ms, through the proxy/);
  const bad = checkWords({ url: "https://x.example/", ok: false, status: null, elapsed_ms: 10000, error: "timed out", via_proxy: false });
  assert.equal(bad.tone, "warn");
  assert.match(bad.sentence, /directly: timed out/);
  assert.equal(CHECK_URL_DEFAULT, "https://api.github.com");
});

const tunnel = (over = {}) => ({
  interface: "utun4",
  up: true,
  protocol: "tunnel",
  provider: "Tailscale",
  addresses: ["100.64.0.2"],
  peer: "100.64.0.2",
  mtu: 1280,
  default_route: false,
  routes: 12,
  dns: ["100.100.100.100"],
  search: [],
  ...over,
});
const internet = (over = {}) => ({ up: true, probe: { tcp: 23, dns: true, fetch: 140 }, public_ip: "203.0.113.7", error: null, ...over });
const wifi = (over = {}) => ({ name: "en0", kind: "wifi", label: "Wi-Fi", addresses: ["192.168.1.20"], mtu: 1500, gateway: "192.168.1.1", default_route: true, routes: 4, dns: ["192.168.1.1"], ...over });
const facts = (over = {}) => ({
  internet: internet(),
  interfaces: [wifi(), { name: "utun4", kind: "tunnel", addresses: ["100.64.0.2"], mtu: 1280, default_route: false, routes: 12, dns: ["100.100.100.100"] }],
  vpn: { up: true, tunnels: [tunnel()], services: [] },
  default_route: { interface: "en0", gateway: "192.168.1.1" },
  dns: { servers: ["192.168.1.1"], search: ["corp.example"], interface: "en0" },
  proxy: { auto_discovery: false, exceptions: [] },
  read_ms: 9,
  ...over,
});

test("the internet: reached with its latency and public IP, down with the reason, the window's word without facts", () => {
  const up = internetWords(facts());
  assert.equal(up.tone, "ok");
  assert.equal(up.label, "up");
  assert.equal(up.sentence, "Reached in 23 ms; public IP 203.0.113.7.");
  assert.match(internetWords(facts({ internet: internet({ public_ip: null, probe: { tcp: 23, dns: null, fetch: null } }) })).sentence, /public IP not read/);
  assert.match(internetWords(facts({ internet: internet({ probe: { tcp: 23, dns: false, fetch: null }, public_ip: null }) })).sentence, /but DNS does not answer/);
  assert.equal(internetWords(facts({ internet: internet({ probe: { tcp: null, dns: true, fetch: 140 } }) })).sentence, "Reached; public IP 203.0.113.7.");
  const down = internetWords(facts({ internet: { up: false, probe: {}, public_ip: null, error: "connect timed out" } }));
  assert.equal(down.tone, "danger");
  assert.equal(down.label, "down");
  assert.equal(down.sentence, "No route to the internet: connect timed out.");
  assert.equal(internetWords(facts({ internet: { up: false, probe: {}, public_ip: null, error: null } })).sentence, "No route to the internet.");
  const window = internetWords(null, true);
  assert.equal(window.label, "up");
  assert.match(window.sentence, /has not been read/);
  for (const w of [internetWords(null, false), internetWords(facts(), false)]) {
    assert.equal(w.label, "down", "the window's offline word beats stale facts");
    assert.equal(w.tone, "danger");
    assert.equal(w.sentence, "The window reports no network.");
  }
  assert.equal(internetWords(null).label, "not read");
  assert.equal(internetWords(null).tone, "quiet");
});

test("the internet's rows: the public IP with the echo's time, the connect's time, DNS", () => {
  assert.deepEqual(internetRows(facts()), [
    { label: "Public IP", value: "203.0.113.7 · 140 ms" },
    { label: "Reached", value: "23 ms" },
    { label: "DNS", value: "answers" },
  ]);
  const bare = internetRows(facts({ internet: internet({ probe: { tcp: null, dns: null, fetch: null } }) }));
  assert.deepEqual(bare, [
    { label: "Public IP", value: "203.0.113.7" },
    { label: "Reached", value: "—" },
    { label: "DNS", value: "not asked" },
  ]);
  assert.equal(internetRows(facts({ internet: internet({ public_ip: null, probe: { tcp: 5, dns: false, fetch: null } }) }))[0].value, "not read");
  assert.equal(internetRows(facts({ internet: internet({ probe: { tcp: 5, dns: false, fetch: null } }) }))[2].value, "does not answer");
});

test("an interface's row says its kind, addresses, gateway and whether it carries the default route", () => {
  const rows = interfaceRows(facts().interfaces);
  assert.deepEqual(rows[0], { label: "en0", value: "Wi-Fi · 192.168.1.20 · via 192.168.1.1 · default route" });
  assert.deepEqual(rows[1], { label: "utun4", value: "tunnel · 100.64.0.2 · 12 routes" });
  assert.equal(interfaceRows([wifi({ name: "en7", kind: "ethernet", label: null, addresses: ["10.0.0.5"], gateway: null, default_route: false, routes: 0 })])[0].value, "Ethernet · 10.0.0.5");
  assert.equal(interfaceRows([wifi({ name: "en7", kind: "ethernet", label: "USB 10/100/1000 LAN", default_route: false, routes: 1 })])[0].value, "USB 10/100/1000 LAN · 192.168.1.20 · via 192.168.1.1 · 1 route");
  assert.match(interfaceRows([wifi({ addresses: [] })])[0].value, /no address/);
  const ordered = interfaceRows([wifi({ name: "en1", default_route: false, routes: 2 }), wifi({ name: "en0" })]);
  assert.deepEqual(ordered.map((r) => r.label), ["en0", "en1"], "the default route's carrier first");
  assert.deepEqual(interfaceRows([]), []);
  assert.deepEqual(Object.keys(KIND_WORDS), ["wifi", "ethernet", "tunnel", "other"]);
  assert.equal(interfaceRows([wifi({ kind: "other", label: null })])[0].value.split(" · ")[0], "other");
});

test("the VPN's standing: a tunnel up, a connected service, a service set up, or no VPN", () => {
  assert.equal(vpnWords(null).label, "not read");
  const up = vpnWords(facts());
  assert.equal(up.label, "up");
  assert.equal(up.sentence, "Tailscale on utun4, 12 routes.");
  assert.match(vpnWords(facts({ vpn: { up: true, tunnels: [tunnel({ default_route: true, provider: null, protocol: "IKEv2" })], services: [] } })).sentence, /IKEv2 on utun4, all traffic/);
  const service = vpnWords(facts({ vpn: { up: true, tunnels: [], services: [{ name: "Office", kind: "IKEv2", connected: true }] } }));
  assert.equal(service.label, "up");
  assert.match(service.sentence, /Office \(IKEv2\)/);
  const down = vpnWords(facts({ vpn: { up: false, tunnels: [tunnel({ up: false })], services: [{ name: "Office", kind: "IKEv2", connected: false }] } }));
  assert.equal(down.label, "not connected");
  assert.match(down.sentence, /set up but not connected: Office/);
  assert.equal(vpnWords(facts({ vpn: { up: false, tunnels: [], services: [] } })).label, "no VPN");
});

test("a tunnel's rows carry only the facts there, and the route says all or split", () => {
  const rows = tunnelRows(tunnel());
  const labels = rows.map((r) => r.label);
  assert.deepEqual(labels, ["Interface", "Protocol", "Provider", "Address", "MTU", "Route", "DNS"], "the peer equal to the address is not repeated");
  assert.match(rows.find((r) => r.label === "Route").value, /split — 12 routes/);
  const all = tunnelRows(tunnel({ default_route: true, provider: null, peer: "10.0.0.1", addresses: ["10.0.0.2", "fd00::2"], search: ["corp.example"] }));
  assert.ok(all.some((r) => r.label === "Peer" && r.value === "10.0.0.1"));
  assert.ok(all.some((r) => r.label === "Addresses"));
  assert.ok(!all.some((r) => r.label === "Provider"));
  assert.match(all.find((r) => r.label === "Route").value, /all traffic/);
  assert.ok(all.some((r) => r.label === "Search domains"));
  // The public IP joins the tunnel's rows only when all traffic leaves by it.
  const exit = tunnelRows(tunnel({ default_route: true }), "203.0.113.7");
  const at = exit.findIndex((r) => r.label === "Public IP");
  assert.ok(at > 0 && exit[at - 1].label === "Route", "after the route");
  assert.equal(exit[at].value, "203.0.113.7");
  assert.ok(!tunnelRows(tunnel(), "203.0.113.7").some((r) => r.label === "Public IP"), "a split tunnel is not the exit");
  assert.ok(!tunnelRows(tunnel({ default_route: true })).some((r) => r.label === "Public IP"), "no address, no row");
});

test("the resolvers and the route read as sentences", () => {
  assert.equal(dnsWords(facts()), "192.168.1.1 on en0; search domains corp.example.");
  assert.equal(dnsWords(facts({ dns: { servers: [], search: [] } })), "No resolver is configured.");
  assert.equal(routeWords(facts()), "en0, via 192.168.1.1.");
  assert.match(routeWords(facts({ default_route: null })), /offline/);
});

test("the Mac's proxy: none, a pair the platform can copy, a PAC file it cannot", () => {
  const none = macProxyWords({ auto_discovery: false, exceptions: [] });
  assert.equal(none.sentence, "System Settings names no proxy.");
  assert.equal(none.followable, false);
  assert.equal(manualFrom({ auto_discovery: false, exceptions: [] }), null);
  const pair = { http: "proxy.corp.example:3128", https: "proxy.corp.example:3128", auto_discovery: false, exceptions: ["*.local", ".corp.example"] };
  const words = macProxyWords(pair);
  assert.equal(words.sentence, "HTTP proxy.corp.example:3128 · HTTPS the same. Bypassing *.local, .corp.example.");
  assert.equal(words.followable, true);
  assert.deepEqual(manualFrom(pair), {
    [KEYS.mode]: "manual",
    [KEYS.http]: "http://proxy.corp.example:3128",
    [KEYS.https]: "http://proxy.corp.example:3128",
    [KEYS.noProxy]: "*.local, .corp.example",
  });
  const pac = macProxyWords({ pac_url: "http://p/proxy.pac", auto_discovery: false, exceptions: [] });
  assert.match(pac.sentence, /PAC file .* cannot follow/);
  assert.equal(pac.followable, false);
  assert.equal(manualFrom({ pac_url: "http://p/proxy.pac", auto_discovery: false, exceptions: [] }), null);
  // Only HTTPS named: it stands for both.
  assert.equal(manualFrom({ https: "p:1", auto_discovery: false, exceptions: [] })[KEYS.http], "");
});

test("why the facts cannot be read, in one sentence", () => {
  assert.match(unavailableWords("not_desktop"), /desktop app/);
  assert.match(unavailableWords("no_reader"), /no network reader/);
  assert.equal(unavailableWords("its own words"), "its own words");
});
