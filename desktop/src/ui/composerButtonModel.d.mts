/** Types for `composerButtonModel.mjs`. This file is the only reason TypeScript never has to read it. */

export declare const SEND_HANDOVER_GRACE_MS: number;
export declare function handoverElapsed(settledAt: number | null, now: number, graceMs?: number): boolean;
export declare function composerButton(state: { busy: boolean; stop: { label?: string } | null | undefined; sendable: boolean; disabled: boolean }): {
  kind: "send" | "sending" | "stop";
  label: string;
  hint: string;
  enabled: boolean;
};
