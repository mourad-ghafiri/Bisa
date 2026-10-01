/** Types for `virtualListModel.mjs`, plain JavaScript so `node --test` reads it. */

export interface Window {
  first: number;
  last: number;
}
export interface Frame {
  scrollTop: number;
  viewHeight: number;
}

export declare function indexAt(offsets: readonly number[], y: number): number;
export declare function windowOf(at: { scrollTop: number; viewHeight: number; rowHeight: number; count: number; overscan: number }): Window;
export declare function windowAt(at: { offsets: readonly number[]; scrollTop: number; viewHeight: number; overscan: number }): Window;
export declare function spacerHeight(count: number, rowHeight: number): number;
export declare function clampScroll(scrollTop: number, total: number, viewHeight: number): number;
export declare function localFrame(panel: { scrollTop: number; viewHeight: number; offsetTop: number; total: number }): Frame;
