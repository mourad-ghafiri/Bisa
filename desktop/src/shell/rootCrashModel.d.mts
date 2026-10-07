/** Types for `rootCrashModel.mjs`. */

export type RootDoor = "try_again" | "reload" | "restart_node" | "reveal_log" | "open_data_folder" | "quit";

export interface RootCrash {
  markCrashed(error: unknown): void;
  clearCrash(): void;
  crashed(): boolean;
  error(): unknown;
}

export declare function createRootCrash(): RootCrash;
export declare const rootCrash: RootCrash;
export declare const ROOT_CRASH_DOORS: readonly RootDoor[];
export declare function rootCrashDoors(inShell: boolean): RootDoor[];
export declare function doorRank(door: string): "primary" | "default" | "ghost";
export declare function rootCrashWords(): { title: string; body: string };
export declare function rootDoorWords(door: string): string;
export declare function rootDoorFailedWords(door: string, reason: unknown): string;
