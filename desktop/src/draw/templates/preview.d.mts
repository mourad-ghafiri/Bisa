/** Types for `preview.mjs`, plain JavaScript so `node --test` reads it. */
export interface BoxShape {
  kind: "rect" | "ellipse" | "diamond";
  x: number;
  y: number;
  w: number;
  h: number;
  fill: string | null;
  dashed: boolean;
}
export interface LineShape {
  kind: "line";
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  arrow: boolean;
}
export interface TextShape {
  kind: "text";
  x: number;
  y: number;
  text: string;
  size: number;
  anchor: "start" | "middle";
}
export interface FrameShape {
  kind: "frame";
  x: number;
  y: number;
  w: number;
  h: number;
}
export type Shape = BoxShape | LineShape | TextShape | FrameShape;
export interface Preview {
  viewBox: { x: number; y: number; w: number; h: number };
  shapes: Shape[];
}
export declare function templatePreview(skeleton: readonly Record<string, unknown>[]): Preview;
