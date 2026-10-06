/**
 * Types for `domFindModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

export interface Segments {
  starts: number[];
  text: string;
}

export interface Span {
  node: number;
  start: number;
  end: number;
}

export interface Band {
  top: number;
  bottom: number;
}

export declare function segmentsOf(texts: readonly string[]): Segments;
export declare function revealOffset(match: Band, port: Band, scrollTop: number): number | null;
export declare function locate(segments: Pick<Segments, "starts">, texts: readonly string[], match: { start: number; end: number }): Span[];
