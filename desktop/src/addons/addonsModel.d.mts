/** Types for `addonsModel.mjs`. */
import type { Addon, AddonManifest, AddonOffer, AddonPermission, AddonProblem, Origin } from "../types";

export function titleWords(showing: number, total: number): string;
export function switchWords(layerShown: boolean): { label: string; hint: string };
export function originWords(origin: Origin | unknown): string;
export function stateWords(addon: Pick<Addon, "active" | "files_present">): string;
export function permissionWords(permission: AddonPermission | unknown): string;
export function sortedPermissions<T>(permissions: readonly T[]): T[];
export function isGranted(granted: readonly AddonPermission[], permission: AddonPermission): boolean;
export function grantToggle(granted: readonly AddonPermission[], declared: readonly AddonPermission[], permission: AddonPermission, on: boolean): AddonPermission[];
export function reviewWords(manifest: Pick<AddonManifest, "name" | "version" | "license" | "permissions">): { title: string; lead: string; lines: string[]; version: string };
export function problemLines(problems: readonly AddonProblem[], tx: (text: AddonProblem["text"]) => string): string[];
export function sortAddons(addons: readonly Addon[]): Addon[];
export function offerRows(offers: readonly AddonOffer[]): AddonOffer[];
export function visibleAddons(addons: readonly Addon[], hidden: readonly string[], layerShown: boolean, switchedOn: boolean): Addon[];
export interface OverlayRow {
  addon: Addon;
  running: boolean;
  shown: boolean;
  putAway: boolean;
  canEnable: boolean;
}
export function overlayRows(addons: readonly Addon[], hidden: readonly string[], layerShown: boolean, switchedOn: boolean): OverlayRow[];
export function rowStateWords(row: OverlayRow): string;
export function withEnabled(addons: readonly Addon[], id: string, enabled: boolean): readonly Addon[];
export function pruneHidden(hidden: readonly string[], addons: readonly Addon[]): readonly string[];
export function sameAddon(a: Addon, b: Addon): boolean;
export function slotOf(addons: readonly Addon[], id: string): number;
export function barTitle(addon: Pick<Addon, "manifest">, title: string | null): string;
export interface ManifestWindow {
  width: number;
  height: number;
  min_width: number | null;
  min_height: number | null;
  max_width: number | null;
  max_height: number | null;
  resizable: boolean;
  closable: boolean;
  transparent: boolean;
  frame: "bar" | "none";
  default_dock: string;
}
export function manifestWindow(manifest: Pick<AddonManifest, "window">): ManifestWindow;
export function declaredOf(manifest: Pick<AddonManifest, "permissions">): AddonPermission[];
export function entryOf(manifest: Pick<AddonManifest, "entry">): string;
