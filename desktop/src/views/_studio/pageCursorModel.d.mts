import type { PageBefore } from "../../api";

export declare function olderThan(oldest: { id: string; created_at: number }, hosted: boolean): PageBefore;
