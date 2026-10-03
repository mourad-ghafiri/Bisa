/**
 * The footer's network read-out says one word, in one order, with the
 * panel's sentences. Run with `node --test desktop/src/shell/networkStatModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { VALUES, barWord, footnote, overlaySections, statWords } from "./networkStatModel.mjs";

const tunnel = (over = {}) => ({
  interface: "utun4",
  up: true,
  protocol: "tunnel",
  provider: "Tailscale",
  addresses: ["100.64.0.2"],
  peer: "100.64.0.2",
  mtu: 1280,
  default_route: true,
  routes: 3,
  dns: ["100.100.100.100"],
  search: [],
  ...over,
});
const internet = (over = {}) => ({ up: true, probe: { tcp: 23, dns: true, fetch: 140 }, public_ip: "203.0.113.7", error: null, ...over });
const facts = (over = {}) => ({
  internet: internet(),
  interfaces: [
    { name: "en0", kind: "wifi", label: "Wi-Fi", addresses: ["192.168.1.20"], mtu: 1500, gateway: "192.168.1.1", default_route: false, routes: 4, dns: ["192.168.1.1"] },
    { name: "utun4", kind: "tunnel", addresses: ["100.64.0.2"], mtu: 1280, gateway: "100.64.0.1", default_route: true, routes: 3, dns: ["100.100.100.100"] },
  ],
  vpn: { up: true, tunnels: [tunnel()], services: [] },
  default_route: { interface: "utun4", gateway: "100.64.0.1" },
  dns: { servers: ["100.100.100.100"], search: [], interface: "utun4" },
  proxy: { auto_discovery: false, exceptions: [] },
  read_ms: 9,
  ...over,
});
const noVpn = (over = {}) =>
  facts({
    vpn: { up: false, tunnels: [], services: [] },
    interfaces: [{ name: "en0", kind: "wifi", label: "Wi-Fi", addresses: ["192.168.1.20"], mtu: 1500, gateway: "192.168.1.1", default_route: true, routes: 4, dns: ["192.168.1.1"] }],
    default_route: { interface: "en0", gateway: "192.168.1.1" },
    dns: { servers: ["192.168.1.1"], search: [] },
    ...over,
  });
const down = (over = {}) => facts({ internet: { up: false, probe: { tcp: null, dns: false, fetch: null }, public_ip: null, error: "connect timed out" }, ...over });
const direct = () => ({ mode: "environment", http: null, https: null, no_proxy: [], http1_only: false, in_force: { kind: "direct" }, environment: {}, problems: [] });
const proxied = () => ({
  mode: "manual",
  http: null,
  https: "http://ada:•••@proxy.example:3128",
  no_proxy: [],
  http1_only: false,
  in_force: { kind: "proxy", http: null, https: "http://ada:•••@proxy.example:3128", no_proxy: "localhost,127.0.0.1,::1" },
  environment: {},
  problems: [],
});

test("the word is DOWN over VPN over UP, each with its tone and the panel's sentences", () => {
  assert.deepEqual([...VALUES], ["DOWN", "VPN", "UP", "—"]);
  const vpn = statWords(facts(), proxied(), true);
  assert.equal(vpn.value, "VPN");
  assert.equal(vpn.tone, "ok");
  assert.match(vpn.title, /^VPN up — Tailscale on utun4, all traffic/);
  assert.match(vpn.title, /internet up, Reached in 23 ms; public IP 203\.0\.113\.7/);
  assert.match(vpn.title, /through a proxy/);
  const up = statWords(noVpn(), proxied(), true);
  assert.equal(up.value, "UP");
  assert.equal(up.tone, "ok");
  assert.match(up.title, /^Up — Reached in 23 ms; public IP 203\.0\.113\.7\./);
  assert.match(up.title, /through a proxy/);
  assert.match(up.title, /no VPN/);
  assert.doesNotMatch(statWords(noVpn(), direct(), true).title, /proxy/, "the proxy is only said when there is one");
  const off = statWords(down(), direct(), true);
  assert.equal(off.value, "DOWN");
  assert.equal(off.tone, "danger");
  assert.equal(off.title, "Down — No route to the internet: connect timed out. (VPN up on utun4)", "a tunnel with no route is not a working VPN, and is named");
  assert.equal(statWords(down({ vpn: { up: false, tunnels: [], services: [] } }), direct(), true).title, "Down — No route to the internet: connect timed out.");
  const set = statWords(noVpn({ vpn: { up: false, tunnels: [], services: [{ name: "Office", kind: "IKEv2", connected: false }] } }), direct(), true);
  assert.equal(set.value, "UP");
  assert.match(set.title, /VPN not connected: A VPN is set up but not connected: Office/);
});

test("the bar draws a word only for a VPN — up and down are the dot's tone, the dash its quiet", () => {
  assert.equal(barWord(statWords(facts(), proxied(), true)), "VPN");
  assert.equal(barWord(statWords(noVpn(), proxied(), true)), null, "up is the dot's ok tone");
  assert.equal(barWord(statWords(down(), direct(), true)), null, "down is the dot's danger tone");
  assert.equal(barWord(statWords(null, null, null)), null, "before the first read: the quiet dot");
  const bar = readFileSync(new URL("./NetworkStat.tsx", import.meta.url), "utf8");
  assert.ok(bar.includes("barWord(words)") && bar.includes("words.value"), "the bar draws barWord's word and keeps every value in its name");
});

test("the window's offline word beats stale facts; its online word never beats the probe", () => {
  const offline = statWords(facts(), direct(), false);
  assert.equal(offline.value, "DOWN");
  assert.equal(offline.tone, "danger");
  assert.match(offline.title, /^Down — The window reports no network\./);
  assert.equal(statWords(down(), direct(), true).value, "DOWN", "a link with nothing behind it");
  assert.equal(statWords(facts(), direct(), null).value, "VPN", "no window word: the facts alone");
});

test("before a read the bar is a dash; off the shell it is the window's word", () => {
  assert.deepEqual(statWords(null, null, null), { value: "—", tone: "quiet", title: "Network — not read yet." });
  assert.deepEqual(statWords(null, direct(), null).value, "—", "the node alone says nothing about the internet");
  const browser = statWords(null, direct(), true);
  assert.equal(browser.value, "UP");
  assert.equal(browser.tone, "ok");
  assert.match(browser.title, /^Up — The window reports a network; this Mac's network has not been read\./);
  const browserOff = statWords(null, proxied(), false);
  assert.equal(browserOff.value, "DOWN");
  assert.equal(browserOff.tone, "danger");
});

test("the overlay's sections come in order, the Mac's absent off the shell and unread before a read", () => {
  const keys = (s) => s.map((x) => x.key);
  const full = overlaySections(facts(), proxied(), true, { desktop: true });
  assert.deepEqual(keys(full), ["internet", "vpn", "interfaces", "mac", "mac-proxy", "platform"]);
  assert.equal(full[0].label, "up");
  assert.equal(full[0].tone, "ok");
  assert.deepEqual(full[0].rows.map((r) => r.label), ["Public IP", "Reached", "DNS"]);
  assert.equal(full[0].rows[0].value, "203.0.113.7 · 140 ms");
  assert.equal(full[1].label, "up");
  assert.ok(full[1].rows.some((r) => r.label === "Provider" && r.value === "Tailscale"));
  assert.ok(full[1].rows.some((r) => r.label === "Public IP" && r.value === "203.0.113.7"), "the tunnel is the exit");
  assert.deepEqual(full[2].rows[0], { label: "utun4", value: "tunnel · 100.64.0.2 · via 100.64.0.1 · default route" });
  assert.deepEqual(full[2].rows[1], { label: "en0", value: "Wi-Fi · 192.168.1.20 · via 192.168.1.1 · 4 routes" });
  assert.equal(full[2].sentence, null);
  assert.equal(full[3].rows[0].value, "utun4, via 100.64.0.1.");
  assert.equal(full[4].sentence, "System Settings names no proxy.");
  assert.equal(full[5].label, "through a proxy");
  assert.deepEqual(full[5].rows, []);
  const offline = overlaySections(down(), direct(), true, { desktop: true });
  assert.equal(offline[0].tone, "danger");
  assert.equal(offline[0].label, "down");
  assert.deepEqual(offline[0].rows, [], "a down internet has no rows worth reading");
  assert.equal(overlaySections(facts({ interfaces: [] }), direct(), true, { desktop: true })[2].sentence, "No interface is up.");
  const browser = overlaySections(null, direct(), false, { desktop: false });
  assert.deepEqual(keys(browser), ["internet", "platform"]);
  assert.equal(browser[0].label, "down");
  assert.match(browser[0].sentence, /desktop app/);
  const unread = overlaySections(null, null, null, { desktop: true });
  assert.deepEqual(keys(unread), ["internet", "platform"]);
  assert.equal(unread[0].label, "not read");
  assert.match(unread[0].sentence, /not been read yet/);
  assert.match(unread[1].sentence, /not been read/);
  const problems = overlaySections(noVpn(), { ...direct(), problems: ["network.proxy.http is not a URL"] }, true, { desktop: true });
  assert.deepEqual(problems[5].rows, [{ label: "Problem", value: "network.proxy.http is not a URL" }]);
});

test("the footnote says the cadence and when the last read landed", () => {
  assert.equal(footnote(30000, null), "read every 30 s · not read yet");
  assert.equal(footnote(30000, 1000, 1012), "read every 30 s · read 12 s ago");
});
