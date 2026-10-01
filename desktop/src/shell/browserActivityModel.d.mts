/** Types for `browserActivityModel.mjs`, plain JavaScript so `node --test` reads it. */

export type Activity = Readonly<Record<string, number>>;

export declare const NO_ACTIVITY: Activity;
export declare function began(state: Activity, key: string): Activity;
export declare function ended(state: Activity, key: string): Activity;
export declare function forgotten(state: Activity, key: string): Activity;
export declare function busyKeys(state: Activity): string[];
