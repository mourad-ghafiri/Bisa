import type { LinkHit } from "../ui";

/** One buffer row as the model reads it: its full-width text and whether xterm wrapped it from the row above. */
export interface TerminalRow {
  text: string;
  wrapped: boolean;
}
/** A link on a logical line, with the cells it covers (xterm's 1-based range) and the hit the link handler takes. */
export interface TerminalLink {
  range: { start: { x: number; y: number }; end: { x: number; y: number } };
  text: string;
  hit: Extract<LinkHit, { kind: "url" } | { kind: "path" }>;
}

export declare const MAX_JOINED_ROWS: number;
export declare function logicalLine(rowAt: (i: number) => TerminalRow | null, y: number, cols: number): { first: number; text: string; offsets: number[] };
export declare function linksAt(rowAt: (i: number) => TerminalRow | null, y: number, cols: number): TerminalLink[];
