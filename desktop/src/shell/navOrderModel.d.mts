export declare const NAV_ORDER_KEY: "bisa.sidebar.order";
export declare function orderKeys(stored: unknown, defaults: readonly string[]): string[];
export declare function placeKey(order: readonly string[], key: string, index: number): readonly string[];
export declare function orderedNav<E extends { key: string }>(nav: readonly E[], order: readonly string[]): E[];
export declare function isDefaultOrder(order: readonly string[], defaults: readonly string[]): boolean;
export declare function homeKey(order: readonly string[], defaults: readonly string[]): string;
