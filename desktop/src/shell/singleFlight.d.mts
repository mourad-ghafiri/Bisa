export interface SingleFlight {
  /** Join the flight for `key`, starting it with `start` when none is in the air. */
  join<T>(key: string, start: (signal: AbortSignal) => Promise<T>, signal?: AbortSignal | null): Promise<T>;
  inFlight(key: string): boolean;
  joiners(key: string): number;
}
export declare function createSingleFlight(): SingleFlight;
