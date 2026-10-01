export type ChipRole = "text-dim" | "accent" | "warn" | "ok" | "danger";
export type ChipSize = "compact" | "md" | "lg";
export declare const TONE_CLASS: Readonly<Record<ChipRole, string>>;
export declare const TONE_FILL: Readonly<Record<ChipRole, string>>;
export declare const CHIP_SIZE: Readonly<Record<ChipSize, { chip: string; current: string; glyph: number; gap: string }>>;
export declare function chipClasses(role: string): string;
