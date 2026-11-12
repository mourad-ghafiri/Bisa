/** The answer of a write, while `listed` is still the list it was made over. */
export declare function heldFor<L, A>(listed: L, held: { over: L; answer: A } | null | undefined): A | null;
export declare function holding<L, A>(listed: L, answer: A): { over: L; answer: A };
