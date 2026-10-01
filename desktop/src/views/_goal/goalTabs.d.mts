export type GoalTab = "progress" | "conversation" | "workflow";
export declare const GOAL_TABS: readonly GoalTab[];
export declare const DEFAULT_TAB: GoalTab;
export declare const GOAL_TAB_LABEL: Readonly<Record<GoalTab, string>>;
export declare function tabOf(param: string | null | undefined): GoalTab;
