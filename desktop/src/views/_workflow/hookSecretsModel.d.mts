export declare function shownWith<S extends { path: string }>(shown: readonly S[], minted: readonly S[] | null | undefined): readonly S[];
/** What an act that armed a listener minted: the secrets its answer carries. */
export declare function mintedBy<S>(answer: { secrets?: readonly S[] | null } | null | undefined): readonly S[];
/** The secret a hook start's form shows after a rotation: the one minted for that very step, else none. */
export declare function shownFor<S extends { step: string }>(secret: S | null | undefined, step: string): S | null;
