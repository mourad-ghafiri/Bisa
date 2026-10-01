/**
 * A tiny least-recently-used map: `get`/`set` move a key to
 * most-recently-used, and a `set` past the capacity drops the oldest key.
 */
export declare class Lru<V> {
  constructor(capacity: number);
  readonly capacity: number;
  get(key: string): V | undefined;
  has(key: string): boolean;
  setCapacity(capacity: number): void;
  set(key: string, value: V): void;
  delete(key: string): boolean;
  readonly size: number;
  keys(): string[];
}
