import type { Ident, OriginLike } from "./gitIdentityModel.mjs";

export interface Ask {
  project: string;
  slug: string;
  workstream: string;
  reason: string;
  origin: OriginLike | null;
  global: Ident | null;
}

export interface Queue {
  asks: Ask[];
  skipped: string[];
}

export interface PendingLike {
  project: string;
  slug: string;
  workstream: string;
  reason: string;
}

export declare function emptyQueue(): Queue;
export declare function seed(queue: Queue, pending: readonly PendingLike[] | null | undefined, global?: Ident | null): Queue;
export declare function onFrame(queue: Queue, payload: { type: string; [k: string]: unknown } | null | undefined): Queue;
export declare function remove(queue: Queue, project: string): Queue;
export declare function skip(queue: Queue, project: string): Queue;
export declare function head(queue: Queue): Ask | null;
export declare function remaining(queue: Queue): number;
