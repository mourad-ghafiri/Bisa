/**
 * Structural equality for a store's snapshot list: same reference,
 * same length, and every element equal by JSON serialisation.
 */
export declare function sameJsonList(a: readonly unknown[], b: readonly unknown[]): boolean;
