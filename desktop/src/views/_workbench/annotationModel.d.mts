import type { ContextRef } from "../../types";
import type { PageRef } from "./contextChips.mjs";
import type { InspectorMark } from "../../ui/artifact/pageInspector.mjs";

/** An element the person pointed at, and the change wanted for it. */
export interface Annotation {
  /** A per-draft sequence — a React key; the number shown is the position. */
  id: number;
  selector: string;
  tag: string;
  excerpt: string;
  text: string;
  note: string;
}
export interface AnnotationDraft {
  seq: number;
  annotations: readonly Annotation[];
  message: string;
}
export interface Pick {
  selector: string;
  tag: string;
  excerpt: string;
  text: string;
}

export declare const EMPTY_DRAFT: AnnotationDraft;
export declare function addAnnotation(draft: AnnotationDraft, pick: Pick, note: string): AnnotationDraft;
export declare function removeAnnotation(draft: AnnotationDraft, id: number): AnnotationDraft;
export declare function setMessage(draft: AnnotationDraft, message: string): AnnotationDraft;
export declare function marksOf(draft: AnnotationDraft): InspectorMark[];
export declare function annotationLabel(n: number, a: Pick<Annotation, "tag" | "note">): string;
export declare function staleWords(lost: readonly number[], n: number): string | null;
export declare function annotationChips(page: PageRef, annotations: readonly Annotation[]): ContextRef[];
export declare function annotationsTarget(count: number, page: PageRef): string;
export declare function annotationsContent(words: string, count: number): string;
export declare function annotationsKey(scope: string, page: PageRef): string;

