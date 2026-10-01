/**
 * The kit's second entry: the flow canvas, alone.
 *
 * `ui/index.ts` is the wall of re-exports every screen imports from, and it
 * deliberately does **not** carry the canvas: `@xyflow/react` is the largest
 * library the app ships, and one re-export would pull it into the main chunk
 * for every screen that never draws a graph. The two designer
 * surfaces import from here instead, so the library lands in their chunk
 * only. The import rule holds: this is still `../ui`, one file down, and no
 * screen names the library itself.
 */

export { FlowCanvas, FlowHandle } from "./FlowCanvas";
export type { FlowCanvasProps, FlowEdge, FlowNode, FlowNodeProps, FlowPosition, FlowSize, FlowViewport } from "./FlowCanvas";
