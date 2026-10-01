import type { ArtifactKind, ArtifactRef } from "../../types";

export declare const ARTIFACT_KINDS: readonly ArtifactKind[];
export declare function kindWords(kind: string): { label: string; glyph: string };
export declare function isTextKind(kind: string): boolean;
export declare function inlinePreview(kind: string): "image" | "live" | "poster";
export declare const MAX_LIVE_FRAMES: number;
export declare function bytesWords(n: number): string;
export declare function artifactKey(messageId: string, ordinal: number): string;
export declare function parseArtifactKey(key: string | null | undefined): { message: string; ordinal: number } | null;
export interface VersionGroup<T extends { title: string }> {
  title: string;
  latest: T;
  versions: T[];
}
export declare function artifactVersions<T extends { title: string }>(rows: readonly T[]): VersionGroup<T>[];
export declare function versionWords(count: number): string;
export declare function versionLabel(index: number, count: number): string;
export declare function languagePath(artifact: Pick<ArtifactRef, "name" | "kind">): string;
export declare const PLAIN_TEXT_BYTES: number;
export declare function kindOf(name: string, mime: string): ArtifactKind;
export declare function mimeOfName(name: string): string;
export declare function defaultTitle(name: string): string;
export declare function artifactFromFile(file: { sha256: string; name: string; mime: string; size: number }, title?: string | null): ArtifactRef;
