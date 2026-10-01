export type DesignerPane = "properties" | "agent" | "runs";

export interface DesignerPanelState {
  open: boolean;
  tab: DesignerPane;
}

export declare const PANES: readonly DesignerPane[];
export declare const DEFAULT_PANE: DesignerPane;
export declare const PANE_LABEL: Readonly<Record<DesignerPane, string>>;
export declare const PANE_ICON: Readonly<Record<DesignerPane, string>>;
export declare const PANE_COMMAND: Readonly<Record<DesignerPane, string>>;
export declare const PANEL_COMMAND: string;
/** The pane a keymap command shows, or `null` for a command that names none. */
export declare function paneOfCommand(command: unknown): DesignerPane | null;
export declare const PANEL_PARAM: string;
export declare function isPane(value: unknown): value is DesignerPane;
export declare function paneOf(value: unknown): DesignerPane;
export declare function pressPane(state: DesignerPanelState, target: string): DesignerPanelState;
export declare function paneForSelection(state: DesignerPanelState, selected: string | null): DesignerPanelState;
export declare function panelTabs(screen: { open: boolean; tab: DesignerPane }): {
  id: DesignerPane;
  label: string;
  icon: string;
  showing: boolean;
}[];
