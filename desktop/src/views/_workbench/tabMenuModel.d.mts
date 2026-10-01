export type DocTabMenuId = "close" | "close-others" | "close-right" | "close-saved" | "close-all" | "keep" | "pin" | "split-right" | "split-down" | "copy-path" | "copy-absolute" | "reveal-files" | "reveal-os";
export type BrowserTabMenuId = "reload" | "copy-url" | "open-outside" | "annotate" | "close" | "close-others";
export type TerminalTabMenuId = "focus" | "new-shell" | "restart" | "send-to-agent" | "show-agents" | "close" | "close-others" | "close-exited";

export interface TabMenuSpecItem<Id extends string> {
  id: Id;
  label: string;
  /** The keymap command that means the same thing, when one does. */
  command?: string;
  danger?: boolean;
  disabled?: boolean;
  separatorBefore?: boolean;
}

export declare function docTabMenu(ctx: {
  pinned: boolean;
  preview: boolean;
  others: number;
  right: number;
  saved: number;
  canSplit: boolean;
  inRoot: boolean;
  onDisk: boolean;
  desktop: boolean;
  reveal: string;
}): TabMenuSpecItem<DocTabMenuId>[];

export declare function browserTabMenu(ctx: { others: number; blank: boolean; annotatable: boolean }): TabMenuSpecItem<BrowserTabMenuId>[];

export declare function terminalTabMenu(ctx: { live: boolean; harness: boolean; others: number; exited: number }): TabMenuSpecItem<TerminalTabMenuId>[];
