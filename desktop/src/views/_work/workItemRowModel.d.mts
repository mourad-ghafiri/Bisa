export declare const NO_INSTRUCTIONS: "No instructions";
export declare function headline(instructions: string | null | undefined): string;
/** Whether an engine frame moved one work item, by the item it names. */
export declare function movesItem(event: { payload?: { type?: string; work_item?: string | null; presence?: { work_item?: string | null } | null } | null } | null | undefined, itemId: string): boolean;
