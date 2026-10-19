/** Types for `monacoModelsModel.mjs`, plain JavaScript so `node --test` reads it. */

export interface Disposable {
  dispose(): void;
}

/** One model per open tab, kept across unmounts and let go when the tab closes. */
export declare class KeptModels<T extends Disposable = Disposable> {
  /** The model held under `key`, else `make()` — held from now on. */
  acquire(key: string, make: () => T): T;
  /** Whether a model is held under `key`. */
  holds(key: string): boolean;
  /** Dispose and let go of each held key; a key nobody held is nothing. Answers how many went. */
  release(keys: readonly string[]): number;
  /** How many tabs hold a model — never how many editors are mounted. */
  get count(): number;
}
