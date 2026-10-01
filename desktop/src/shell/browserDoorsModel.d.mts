/** Types for `browserDoorsModel.mjs`, plain JavaScript so `node --test` reads it. */

import type { IdeCentre } from "../views/_workbench/ideModeModel.mjs";
import type { BrowserHome } from "./browsersModel.mjs";

export declare function screenHome(target: { kind: string; id: string } | null, root: { scope: string; id: string } | null, route: { name: string; id?: string } | null): BrowserHome | null;
/** What the person's door to the Browser pane does: hide it, show it, or open a tab and show it. */
export declare function paneToggle(facts: { showing: boolean; seen: number }): "hide" | "show" | "open";
export declare function followCentre(centre: IdeCentre, centreTab: string | null, paneTab: string | null): { show: "pane"; key: string } | { show: "centre"; key: string } | null;
