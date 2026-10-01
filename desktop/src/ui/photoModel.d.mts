/** Types for `photoModel.mjs`, plain JavaScript so `node --test` reads it. */

export interface PhotoProfile {
  readonly kind: "picture" | "face";
  readonly edge: number;
  readonly mime: string;
  readonly maxBytes: number;
  readonly qualities: readonly number[];
}
export declare const PHOTO_PROFILES: { readonly picture: PhotoProfile; readonly face: PhotoProfile };
export declare const PHOTO_EDGE: number;
export declare const THUMB_EDGE: number;
export declare const PHOTO_MIME: string;
export declare const THUMB_CACHE_MAX: number;
export declare const PHOTO_TYPES: readonly string[];
export interface Crop {
  sx: number;
  sy: number;
  sw: number;
  sh: number;
  dw: number;
  dh: number;
}
export declare function isPhotoType(mime: string | null | undefined): boolean;
export declare function coverCrop(width: number, height: number, edge: number): Crop;
export declare function photoName(original: string | null | undefined, profile?: PhotoProfile): string;
export declare function photoRefusal(reason: "type" | "decode" | "encode" | "size", profile?: PhotoProfile): string;
export declare function fitsProfile(bytes: number, profile: PhotoProfile): boolean;
export declare function photoOfPrincipal<P extends { sha256: string }>(members: readonly { pubkey: string; photo?: P | null }[], agents: readonly { pubkey: string; photo?: P | null }[], pubkey: string): P | null;
export declare function thumbKey(sha: string, edge: number): string;
export declare function evictOrder(entries: readonly { key: string; at: number }[], keep: number): string[];
