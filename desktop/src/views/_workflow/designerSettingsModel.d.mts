/** Types for `designerSettingsModel.mjs`. */
export interface DesignerScreenSettings {
  snap: boolean;
  grid: number;
  minimap: boolean;
  autosaveDelayMs: number;
}
export declare const DESIGNER_DEFAULTS: Readonly<DesignerScreenSettings>;
export declare const GRID_BOUNDS: Readonly<{ min: number; max: number }>;
export declare const AUTOSAVE_BOUNDS: Readonly<{ min: number; max: number }>;
export declare const VALIDATE_DELAY_MS: number;
export declare function designerSettingsOf(resolved: readonly { key: string; value: unknown }[] | null | undefined): DesignerScreenSettings;
