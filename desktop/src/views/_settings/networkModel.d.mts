/**
 * Types for `networkModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

import type { InterfaceFacts, InterfaceKind, MacProxy, NetworkFacts, TunnelFacts } from "../../shell/networkApi";
import type { NetworkCheck, NetworkStatus, ProxyMode } from "../../types";

export declare const MODES: readonly ProxyMode[];
export declare const KEYS: Readonly<{ mode: string; http: string; https: string; noProxy: string; http1Only: string; publicIpUrl: string }>;
export declare const CHECK_URL_DEFAULT: string;
export declare const PUBLIC_IP_URL_DEFAULT: string;
export declare const PROVIDERS: readonly string[];
export declare const KIND_WORDS: Readonly<Record<InterfaceKind, string>>;

export interface Words {
  /** The state as a machine word — `up` · `down` · `not-read`, `proxy` · `direct`, `no-vpn` — compared on, never the label. */
  kind?: string;
  tone: "ok" | "warn" | "neutral" | "quiet" | "danger";
  label: string;
  sentence: string;
}

export interface Row {
  label: string;
  value: string;
}

export declare function modeSegments(): { id: ProxyMode; label: string }[];
export declare function modeWords(mode: ProxyMode): string;
export declare function inForceWords(status: NetworkStatus): Words;
export declare function checkWords(result: NetworkCheck | null): Words | null;
export declare function internetWords(facts: NetworkFacts | null, online?: boolean | null): Words;
export declare function internetRows(facts: NetworkFacts): Row[];
export declare function interfaceRows(interfaces: readonly InterfaceFacts[]): Row[];
export declare function vpnWords(facts: NetworkFacts | null): Words;
export declare function tunnelRows(tunnel: TunnelFacts, publicIp?: string | null): Row[];
export declare function dnsWords(facts: NetworkFacts): string;
export declare function routeWords(facts: NetworkFacts): string;
export declare function macProxyWords(proxy: MacProxy): { sentence: string; followable: boolean };
export declare function manualFrom(proxy: MacProxy): Record<string, string> | null;
export declare function unavailableWords(reason: "not_desktop" | "no_reader" | string): string;
export declare const SECRET_SETTING_KEYS: readonly string[];
export declare function isSecretSetting(key: string): boolean;
