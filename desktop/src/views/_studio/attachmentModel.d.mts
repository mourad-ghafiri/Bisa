import type { AttachmentRef } from "../../types";

export declare function formatBytes(size: number): string;
export declare function isRenderableImage(file: AttachmentRef, present: boolean): boolean;
export declare function glyphFor(mime: string): string;
export declare function describe(file: AttachmentRef, present: boolean): string;
