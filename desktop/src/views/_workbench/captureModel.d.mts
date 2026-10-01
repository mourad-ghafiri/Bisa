/** Types for `captureModel.mjs`, plain JavaScript so `node --test` reads it (ide/19). */

import type { AttachmentRef, ContextRef } from "../../types";

export interface Mark {
  x: number;
  y: number;
  width: number;
  height: number;
}
export interface Capture {
  id: number;
  mark: Mark | null;
  note: string;
  shot: AttachmentRef;
  label: string;
  width: number | null;
  height: number | null;
}
export interface CaptureDraft {
  seq: number;
  captures: readonly Capture[];
  message: string;
}
export declare const EMPTY_CAPTURE_DRAFT: CaptureDraft;
export declare function addCapture(draft: CaptureDraft, taken: { shot: AttachmentRef; mark: Mark | null; label: string; width?: number | null; height?: number | null }, note: string): CaptureDraft;
export declare function removeCapture(draft: CaptureDraft, id: number): CaptureDraft;
export declare function setCaptureMessage(draft: CaptureDraft, message: string): CaptureDraft;
export declare function markWords(mark: Mark | null): string;
export declare function captureLabel(n: number, c: Capture): string;
export declare function captureChip(device: string, label: string, shot: AttachmentRef, mark: Mark | null, note: string): ContextRef;
export declare function captureChips(device: string, label: string, captures: readonly Capture[]): ContextRef[];
export declare function capturesTarget(count: number, label: string): string;
export declare function capturesContent(words: string, count: number): string;
export declare function capturesKey(scope: string, deviceId: string): string;
