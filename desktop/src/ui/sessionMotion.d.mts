import type { StateLike } from "./sessionState.mjs";

export type Motion = "spin" | "breathe" | "nudge" | "pop" | "shake" | "none";

export declare const MOTIONS: readonly Motion[];
export declare function motionOf(state: StateLike): Motion;
export declare function repeats(motion: Motion): boolean;
export declare function motionClass(motion: Motion): string;
