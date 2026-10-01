export declare const BASE_PX: { readonly body: number; readonly secondary: number; readonly meta: number };

export declare function typeSpecimen(scale: number): { percent: number; body: number; secondary: number; meta: number };

export declare function isDefaultScale(scale: number, fallback?: number): boolean;

export declare function sampleAttributes(input: {
  theme?: string | null;
  scheme: "light" | "dark";
  accent?: string | null;
}): Record<"data-theme" | "data-scheme" | "data-accent", string | null>;

export interface FamilyLike {
  id: string;
  label: string;
  mood: string;
  light: string;
  dark: string;
}

export interface FamilyCard<T extends string = string> {
  id: string;
  label: string;
  mood: string;
  tiles: { id: T; label: string; active: boolean }[];
  /** Only on the System card: the two palettes its single tile is split between. */
  halves: { light: T; dark: T } | null;
}

export declare function familyCards<T extends string>(
  families: readonly (FamilyLike & { light: T; dark: T })[],
  defaultFamily: FamilyLike & { light: T; dark: T },
  current: string,
): FamilyCard<T | "system">[];

export declare const SPECIMEN: { readonly ui: string; readonly mono: string; readonly body: string };
