/**
 * The desktop shell's network command — the Tauri side of the footer's
 * network word and of Settings › Capabilities › Network's *This Mac*
 * (ide/01: a machine fact is the shell's to read): the internet probe and
 * the public IP the echo service at `network.public_ip_url` answers (an
 * empty URL asks none), every interface that is up, the VPN, the route, the
 * resolvers, System Settings' proxy. Tauri-only: a browser dev session has
 * no `#[tauri::command]`s, so this answers `null` and the panel says the
 * facts are the desktop app's to read. The shapes mirror
 * `src-tauri/src/network.rs`.
 */

import { inDesktopShell } from "../api";

/** A VPN service System Settings knows (`scutil --nc list`). */
export interface VpnService {
  name: string;
  /** The protocol word: `IKEv2`, `IPsec`, `L2TP`, a provider's own. */
  kind: string;
  connected: boolean;
}

/** One tunnel interface with what the other readers say about it. */
export interface TunnelFacts {
  interface: string;
  up: boolean;
  protocol: string;
  provider?: string | null;
  addresses: string[];
  peer?: string | null;
  mtu?: number | null;
  /** The machine's default route leaves by this tunnel. */
  default_route: boolean;
  /** How many routes the table sends through it. */
  routes: number;
  dns: string[];
  search: string[];
}

export interface VpnFacts {
  up: boolean;
  tunnels: TunnelFacts[];
  services: VpnService[];
}

export interface DefaultRoute {
  interface: string;
  gateway: string;
}

export interface DnsFacts {
  servers: string[];
  search: string[];
  interface?: string | null;
}

/** What System Settings names under Proxies (`scutil --proxy`). */
export interface MacProxy {
  http?: string | null;
  https?: string | null;
  socks?: string | null;
  pac_url?: string | null;
  auto_discovery: boolean;
  exceptions: string[];
}

/** What each probe answered; absent where it failed or was not asked. */
export interface Probe {
  /** A TCP connect to a well-known address on 443, in ms — the faster of two. */
  tcp?: number | null;
  /** Whether the echo service's name resolved; absent when none is named or it never answered. */
  dns?: boolean | null;
  /** The echo request, in ms, when it answered an address. */
  fetch?: number | null;
}

/** Whether the internet is reachable, and how it was asked. */
export interface InternetFacts {
  /** The echo service answered an address, or a TCP connect went through. */
  up: boolean;
  probe: Probe;
  /** The address the echo service saw this machine as. */
  public_ip?: string | null;
  /** Why the echo request gave no address, when it did not. */
  error?: string | null;
}

export type InterfaceKind = "wifi" | "ethernet" | "tunnel" | "other";

/** One interface that is up, with what the other readers say about it. */
export interface InterfaceFacts {
  name: string;
  kind: InterfaceKind;
  /** The hardware port's word — `Wi-Fi`, `Thunderbolt Bridge` — when System Settings names one. */
  label?: string | null;
  addresses: string[];
  mtu?: number | null;
  gateway?: string | null;
  /** The machine's default route leaves by it. */
  default_route: boolean;
  /** How many routes the table sends through it. */
  routes: number;
  /** The resolvers bound to it. */
  dns: string[];
}

export interface NetworkFacts {
  internet: InternetFacts;
  /** Every interface that is up, the tunnels among them. */
  interfaces: InterfaceFacts[];
  vpn: VpnFacts;
  default_route?: DefaultRoute | null;
  dns: DnsFacts;
  proxy: MacProxy;
  read_ms: number;
}

/**
 * This Mac's network facts; `null` off the desktop shell, and `null` from a
 * shell on a platform with no readers. `publicIpUrl` is the echo service the
 * person named; empty asks none.
 */
export async function networkFacts(publicIpUrl: string): Promise<NetworkFacts | null> {
  if (!inDesktopShell()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<NetworkFacts | null>("network_facts", { publicIpUrl });
}
