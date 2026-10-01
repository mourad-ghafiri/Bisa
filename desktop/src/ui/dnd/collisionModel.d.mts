export declare function smallestFirst<Id extends string | number>(within: readonly Id[], rectOf: (id: Id) => { width: number; height: number } | null | undefined): Id[];
