/** Types for `leaveGuardModel.mjs`, plain JavaScript so `node --test` reads it. */
export type HoldKind = "note" | "drawing";
export interface GuardWords {
  title: string;
  description: string;
  note: string;
}
export declare function leaveDecision(hold: { dirty: () => boolean } | null | undefined): "ask" | "go";
export declare function holdKey(kind: HoldKind, id: string): string;
export declare function leaveWords(kind: HoldKind, title: string | null | undefined): GuardWords;
