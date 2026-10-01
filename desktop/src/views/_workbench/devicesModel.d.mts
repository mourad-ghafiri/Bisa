/** Types for `devicesModel.mjs`, plain JavaScript so `node --test` reads it (ide/19). */

import type { MobileDevice } from "../../types";

export declare function deviceById(devices: readonly MobileDevice[], id: string): MobileDevice | null;
export declare function isUp(device: MobileDevice): boolean;
export declare function platformAllows(platforms: string, platform: string): boolean;
export declare function deviceIcon(device: MobileDevice): "device" | "simulator";
export declare function deviceTitle(device: MobileDevice): string;
export declare function deviceNote(device: MobileDevice): string | null;

export interface DeviceMenuItem {
  id: string;
  label: string;
  hint?: string;
  icon: string;
  disabled?: boolean;
  danger?: boolean;
  separatorBefore?: boolean;
}
export interface DeviceMenu {
  main: DeviceMenuItem;
  items: DeviceMenuItem[];
}
export declare function deviceMenu(facts: {
  enabled: boolean;
  flutter: boolean | null;
  platforms: string;
  devices: readonly MobileDevice[];
  tabs: readonly string[];
  running: readonly string[];
  busy: boolean;
}): DeviceMenu;
