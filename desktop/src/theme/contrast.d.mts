export interface Oklch {
  l: number;
  c: number;
  h: number;
  alpha: number;
}
export declare function parseOklch(text: string): Oklch | null;
export declare function oklchToLinearSrgb(c: Pick<Oklch, "l" | "c" | "h">): { r: number; g: number; b: number };
export declare function composite(over: string, under: string): string;
export declare function luminance(text: string): number;
export declare function contrast(a: string, b: string): number;
export declare function deltaL(a: string, b: string): number;
export declare function hueDistance(a: string, b: string): number;
export declare function toHex(text: string): string | null;
