export declare function pressRailTab<T extends string>(state: { open: boolean; tab: T }, target: T): { open: boolean; tab: T };
export declare function nextRailIndex(key: string, at: number, count: number): number | null;
export declare function railAnchor<T extends string>(tabs: readonly T[], state: { open: boolean; tab: T }): T | null;
