export type ToggleSide = "rail" | "right";
export declare function toggleWords(side: ToggleSide, open: boolean): { label: string; command: string };

import type { IdeMode } from "./ideModeModel.mjs";
export type ModeIcon = "file" | "agent" | "board";
export declare const MODE_COMMAND: "toggle_ide_mode";
export declare const BOARD_COMMAND: "board";
export declare function modeWords(mode: IdeMode): { label: string; icon: ModeIcon; hint: string; command: string };
export declare function modeSwitchLabel(mode: IdeMode, boardEnabled?: boolean): string;
export declare function modeSegments(boardEnabled?: boolean): { id: IdeMode; label: string; icon: ModeIcon; hint: string; command: string }[];
