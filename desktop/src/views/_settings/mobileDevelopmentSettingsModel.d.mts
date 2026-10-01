/** Types for `mobileDevelopmentSettingsModel.mjs`, plain JavaScript so `node --test` reads it (ide/19). */

import type { MobileDevice, MobileToolchain } from "../../types";

export declare const ENABLED_KEY: string;
export declare const PLATFORMS_KEY: string;
export declare const AGENTS_KEY: string;
export type Platforms = "both" | "ios" | "android";
export type MobileDevelopmentPolicy = "everyone" | "assigned" | "nobody";
export declare const PLATFORMS: readonly Platforms[];
export declare const DEFAULT_PLATFORMS: Platforms;
export declare const POLICIES: readonly MobileDevelopmentPolicy[];
export declare const DEFAULT_POLICY: MobileDevelopmentPolicy;

export interface SettingSegment<T extends string> {
  id: T;
  label: string;
  icon: string;
  disabled?: boolean;
  hint?: string;
}
export declare function platformSegments(mac: boolean): SettingSegment<Platforms>[];
export declare function platformShown(value: Platforms, mac: boolean): Platforms;
export declare function platformWords(platforms: string): string;
export declare function policySegments(): SettingSegment<MobileDevelopmentPolicy>[];
export declare function policyWords(policy: string): string;
export declare function statusWords(facts: { enabled: boolean; platforms: string; flutter: boolean | null }): { tone: "ok" | "warn" | "quiet"; label: string; sentence: string };

export interface InstallWords {
  url: string;
  text: string;
  command: string | null;
}
export declare const INSTALL: Readonly<Record<"flutter" | "xcode" | "runtime" | "cocoapods" | "android" | "avd" | "java", InstallWords>>;
export interface ComponentRow {
  id: string;
  name: string;
  tone: "ok" | "warn" | "quiet";
  found: string;
  install: InstallWords | null;
  held: string | null;
}
export declare function componentRows(toolchain: MobileToolchain | null, facts: { platforms: string; mac: boolean }): ComponentRow[];
export declare function doctorRows(doctor: readonly { state: string; name: string; detail?: string | null }[] | undefined): { tone: "ok" | "warn" | "neutral"; name: string; detail: string | null }[];
export declare function checkedWords(checkedAt: number, now?: number): string;
export interface DeviceRow {
  id: string;
  name: string;
  words: string;
  up: boolean;
  verbs: ("boot" | "shutdown" | "show")[];
}
export declare function deviceRows(devices: readonly MobileDevice[], platforms: string): DeviceRow[];
export declare function simulatorProblem(draft: { name: string; devicetype: string; runtime: string }): string | null;
