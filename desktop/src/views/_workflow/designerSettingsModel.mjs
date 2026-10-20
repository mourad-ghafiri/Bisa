/**
 * The designer's canvas switches and clocks, from the settings registry
 * (`workflow.*`): the defaults are the registry's, so a node that cannot
 * be asked draws the same canvas, and every number is held to a sane range
 * rather than read as typed. Plain `.mjs`, so `node --test` reads it.
 */

import { boolOf, numberOf } from "../../shell/settingsModel.mjs";

export const DESIGNER_DEFAULTS = Object.freeze({ snap: true, grid: 16, minimap: false, autosaveDelayMs: 800 });
/** The grid's spacing, in canvas pixels. */
export const GRID_BOUNDS = Object.freeze({ min: 4, max: 64 });
/** The quiet before an autosave, in milliseconds. */
export const AUTOSAVE_BOUNDS = Object.freeze({ min: 100, max: 30_000 });
/** The quiet before a drawing is put to the node's validator, in milliseconds. */
export const VALIDATE_DELAY_MS = 200;

/**
 * @param {readonly {key: string, value: unknown}[] | null | undefined} resolved
 * @returns {{snap: boolean, grid: number, minimap: boolean, autosaveDelayMs: number}}
 */
export function designerSettingsOf(resolved) {
  return {
    snap: boolOf(resolved, "workflow.designer.snap", DESIGNER_DEFAULTS.snap),
    grid: numberOf(resolved, "workflow.designer.grid", DESIGNER_DEFAULTS.grid, GRID_BOUNDS),
    minimap: boolOf(resolved, "workflow.designer.minimap", DESIGNER_DEFAULTS.minimap),
    autosaveDelayMs: numberOf(resolved, "workflow.autosave.delay_ms", DESIGNER_DEFAULTS.autosaveDelayMs, AUTOSAVE_BOUNDS),
  };
}
