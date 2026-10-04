export interface Rect {
  left: number;
  top: number;
  width: number;
  height: number;
}
export declare function sameRect(a: Rect | null | undefined, b: Rect | null | undefined): boolean;
export declare function roundRect(r: { left: number; top: number; width: number; height: number } | null | undefined): Rect | null;
export declare function slotStyle(
  rect: Rect | null | undefined,
  visible: boolean,
): {
  position: "fixed";
  left: number;
  top: number;
  width: number;
  height: number;
  visibility: "visible" | "hidden";
  pointerEvents: "auto" | "none";
  /** Present only while hidden: a hidden layer paints no frost. */
  backdropFilter?: "none";
  WebkitBackdropFilter?: "none";
};
