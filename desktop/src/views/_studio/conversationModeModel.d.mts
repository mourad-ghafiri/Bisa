/**
 * Types for `conversationModeModel.mjs`. This file is the only reason
 * TypeScript never has to read it.
 */

import type { ConversationMode, ConversationOrigin } from "../../types";

export declare const CONVERSATION_MODES: readonly ConversationMode[];
export declare const MODE_LABEL: Readonly<Record<ConversationMode, string>>;
export declare const MODE_MEANING: Readonly<Record<ConversationMode, string>>;
export declare const MODE_ICON: Readonly<Record<ConversationMode, "person" | "run" | "toolRead">>;
export declare const DEFAULT_MODE: ConversationMode;
export declare const CHECKOUT_ORIGIN_KINDS: readonly ConversationOrigin["kind"][];
export declare const PLAN_NEEDS_GUARD_HINT: string;
export declare const BUILD_MESSAGE: string;

export declare function isCheckoutOrigin(kind: ConversationOrigin["kind"] | string | null | undefined): boolean;
export declare function modeOf(conversation: { mode?: ConversationMode | null } | null | undefined): ConversationMode;
export declare function nextMode(mode: ConversationMode, toolGuard?: boolean): ConversationMode;
export declare function modeChoices(toolGuard: boolean): {
  id: ConversationMode;
  label: string;
  description: string;
  icon: "person" | "run" | "toolRead";
  disabled: boolean;
  hint: string | undefined;
}[];
export declare function planBannerWords(): { build: string; refine: string };
