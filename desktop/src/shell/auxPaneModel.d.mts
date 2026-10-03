/** Types for `auxPaneModel.mjs`, plain JavaScript so `node --test` reads it. */

export declare function toggledAux(showing: { kind: string | null; id: string | null }, next: string, nextId?: string): { aux: string; auxId: string | null } | null;

export declare const AUX_WIDTH_KEY: "bisa.aux.width";
export declare const AUX_DEFAULT_WIDTH: 380;
export declare const AUX_MIN_WIDTH: 300;
export declare const AUX_MAX_SHARE: 0.7;
export declare const SCREEN_MIN_WIDTH: 420;
export interface AuxBounds {
  min: number;
  max: number;
}
export declare function auxBounds(available: number | null | undefined): AuxBounds;
export declare function shownWidth(stored: number, bounds: AuxBounds): number;
