/** Types for `newDrawingModel.mjs`, plain JavaScript so `node --test` reads it. */
import type { OwnerScope } from "./drawModel.mjs";

export declare function defaultTitle(template: { id: string; label: string } | null | undefined, untitledWord: string): string;
export declare function titleFollowsTemplate(current: string, previousDefault: string): boolean;
export declare function firstTarget<T extends { here?: boolean }>(targets: readonly T[]): T | null;
export declare function targetKey(scope: OwnerScope): string;
export declare function canCreate(title: string): boolean;
