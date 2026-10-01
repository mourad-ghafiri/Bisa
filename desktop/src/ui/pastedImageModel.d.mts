/** Types for `pastedImageModel.mjs`, plain JavaScript so `node --test` reads it. */

export declare function pastedImageName(at?: Date): string;
export declare function suggestedName(name: string, at?: Date): string;
export declare function extensionOf(name: string): string;
export declare function withExtension(value: string, ext: string): string;
export declare function imageNameError(value: string, taken: readonly string[]): string | null;
export declare function pasteIntake<T extends { name: string; type: string }>(
  entries: readonly T[],
  hasText: boolean,
): { pictures: T[]; files: T[]; askShell: boolean };
