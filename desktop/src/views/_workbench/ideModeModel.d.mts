export type IdeMode = "project" | "agent" | "board";
/** What a root's centre shows: documents and terminals, the conversation, or the Board. */
export type IdeCentre = "documents" | "conversation" | "board";

export declare const MODES: readonly IdeMode[];
export declare const DEFAULT_MODE: IdeMode;
/** The mode a document is shown in: opening one puts its root there. */
export declare const DOCUMENT_MODE: IdeMode;
export declare const DEFAULT_MODE_KEY: "ide.default_mode";

export declare function centreOf(mode: unknown, root: { scope: string; hasProject: boolean }): IdeCentre;
export declare function availableModes(boardEnabled?: boolean): IdeMode[];
export declare function modeOf(value: unknown, boardEnabled?: boolean): IdeMode;
export declare function modeFor(remembered: unknown, fallback: unknown, boardEnabled?: boolean): IdeMode;
export declare function nextMode(mode: IdeMode | string | unknown, boardEnabled?: boolean): IdeMode;
export declare function rememberMode(
  byRoot: Readonly<Record<string, string>>,
  root: string,
  mode: IdeMode,
  cap?: number,
): Readonly<Record<string, IdeMode>>;
export declare function parseRememberedModes(raw: unknown): Record<string, IdeMode>;
