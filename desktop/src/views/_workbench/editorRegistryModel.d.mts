/** Types for `editorRegistryModel.mjs`, plain JavaScript so `node --test` can step it. */

export interface PendingLine {
  readonly key: string | null;
  readonly line: number | null;
}

export interface PendingAction {
  readonly key: string | null;
  readonly action: string | null;
}

export interface RegistryState {
  /** The editor that last took focus, or none. */
  readonly active: string | null;
  /** The one line waiting to be shown, or none. */
  readonly pending: PendingLine;
  /** The one editor action waiting to be run, or none. */
  readonly action: PendingAction;
}

export declare const INITIAL: RegistryState;
export declare const MOUNT_GRACE_MS: number;
export declare function actionRequested(state: RegistryState, key: string, action: string, ranNow: boolean): RegistryState;
export declare function actionTaken(state: RegistryState, key: string): { action: string | null; state: RegistryState };
export declare function activated(state: RegistryState, key: string): RegistryState;
export declare function released(state: RegistryState, key: string): RegistryState;
export declare function lineRequested(state: RegistryState, key: string, line: number, shownNow: boolean): RegistryState;
export declare function lineTaken(state: RegistryState, key: string): { line: number | null; state: RegistryState };
export declare function dirtyCount(unsavedKeys: readonly string[], sources: readonly { dirty: () => boolean }[]): number;
