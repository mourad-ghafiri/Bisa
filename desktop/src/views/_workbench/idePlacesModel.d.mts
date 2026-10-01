/** Types for `idePlacesModel.mjs`. */

export declare const RAIL_PLACE: string;
export declare const BOARD_PLACE: string;
/** The root a path of the Project IDE shows, or `null` for a path that is no root's. */
export declare function rootOfPath(path: string): string | null;
export declare function idePlace(root: string): string;
export declare function gitPlace(scope: string): string;
export declare function placesOfRoot(root: string): string[];
export declare function docsPrefix(root: string): string;
