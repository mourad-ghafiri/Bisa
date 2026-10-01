export interface Pool {
  held: string[];
}
export declare const BUDGET: number;
export declare const RELEASE_AFTER_MS: number;
export declare function emptyPool(): Pool;
export declare function wants(args: { visible: boolean | undefined; active?: boolean }): boolean;
export declare function holds(pool: Pool, key: string): boolean;
export declare function acquire(pool: Pool, key: string): { pool: Pool; granted: boolean };
export declare function release(pool: Pool, key: string): Pool;
