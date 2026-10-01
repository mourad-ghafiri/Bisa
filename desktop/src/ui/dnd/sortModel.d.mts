export declare function sortableDrop(ids: readonly string[], activeId: string, overId: string): { from: number; to: number } | null;
export declare function moveIndex<T>(list: readonly T[], from: number, to: number): readonly T[];
export declare function cycle(ids: readonly string[], active: string | null, dir: 1 | -1): string | null;
export declare function sortableId(family: string | null | undefined, listId: string, id: string): string;
export declare function hoverIndex(ids: readonly string[], overId: string | null | undefined, activeId: string): number;
