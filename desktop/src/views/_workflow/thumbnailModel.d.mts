import type { Definition } from "./workflowGraph.mjs";
import type { Family } from "./stepKinds.mjs";

export declare const PAD: number;
export declare const LOOP_OUT: number;

export interface ThumbNode {
  id: string;
  kind: string;
  /** What the card is drawn as: an event's pill, a gateway's, a loop's or a task's card. */
  family: Family;
  name: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface ThumbEdge {
  id: string;
  /** `boundary`: a divert's path, off the card's lower right. */
  kind: "then" | "on_fail" | "loop" | "boundary";
  points: { x: number; y: number }[];
}

export interface Thumbnail {
  viewBox: { x: number; y: number; w: number; h: number };
  nodes: ThumbNode[];
  edges: ThumbEdge[];
}

export declare function thumbnail(wf: Definition | { steps?: readonly Record<string, unknown>[] }): Thumbnail;
