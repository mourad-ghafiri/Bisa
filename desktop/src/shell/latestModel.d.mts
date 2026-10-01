/** Types for `latestModel.mjs`. */

export interface Latest {
  /** A read begins: its ticket, and every ticket before it out of date. */
  begin(): number;
  /** Whether this read's answer may be applied: the newest asked for, the reader still there. */
  lands(ticket: number): boolean;
  /** The reader left: nothing asked for so far lands. */
  close(): void;
  /** The reader is there again. */
  open(): void;
}

export declare function createLatest(): Latest;
