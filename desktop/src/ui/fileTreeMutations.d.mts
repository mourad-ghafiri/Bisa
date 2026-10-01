export declare function basename(path: string): string;
export declare function splitExt(name: string): [string, string];
export declare function nextCopyName(name: string, siblings: Iterable<string>): string;
export declare function renameSelection(name: string): [number, number];
export type Disposal = "trash" | "unlink";
import type { Target } from "./fileTreeModel.mjs";
export declare function deleteCopy(args: {
  targets: readonly Target[];
  counts: Record<string, number | null>;
  disposal: Disposal | null;
  gitRoot: boolean;
}): { title: string; body: string; confirm: string; danger: boolean };
export declare function fanOutSummary(args: { verb: string; done: readonly string[]; failed: { path: string; reason: string } | null; total: number }): { ok: boolean; text: string };
export declare function revealLabel(userAgent: string | null | undefined): string;
export type MenuId =
  | "new-file"
  | "new-dir"
  | "rename"
  | "duplicate"
  | "cut"
  | "copy"
  | "paste"
  | "copy-path"
  | "copy-absolute"
  | "serve"
  | "reveal"
  | "delete"
  | "refresh";
export interface MenuSpecItem {
  id: MenuId;
  label: string;
  /** The keymap command that means the same thing, when one does. */
  command?: string;
  danger?: boolean;
  disabled?: boolean;
  separatorBefore?: boolean;
}
export type PasteSource = "tree" | "os" | "image" | null;
export declare function menuSpec(args: { targets: readonly Target[]; desktop: boolean; mutable: boolean; clipboard: PasteSource; reveal: string; serve?: boolean }): MenuSpecItem[];
export declare function viewportMenuSpec(args: { desktop: boolean; mutable: boolean; clipboard: PasteSource; reveal: string }): MenuSpecItem[];
export declare function renameError(value: string, current: string, siblings: Iterable<string>): string | null;
export declare function targetDir(path: string, dir: boolean): string;
