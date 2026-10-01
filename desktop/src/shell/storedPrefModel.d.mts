export declare function webStorage(): Storage | null;
/** A parse that answers `undefined` — a word off the list — falls to the fallback, so the answer is always a `T`. */
export declare function readPref<T>(storage: Storage | null | undefined, key: string, parse: (raw: string) => T | undefined, fallback: T): T;
export declare function writePref(storage: Storage | null | undefined, key: string, value: unknown): boolean;
export declare function forgetPref(storage: Storage | null | undefined, key: string): boolean;
/** Forget every key that begins with `prefix` but those that begin with one of `spared`; how many were forgotten. */
export declare function forgetPrefsUnder(storage: Storage | null | undefined, prefix: string, spared?: readonly string[]): number;
export declare function jsonPref(raw: string): unknown;
export declare function switchPref(raw: string): boolean | undefined;
export declare function switchWord(on: boolean): "1" | "0";
