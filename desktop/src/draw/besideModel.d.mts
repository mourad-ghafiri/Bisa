/** The corner's inset and the air between the two cards, in CSS pixels. */
export declare const INSET: number;
/** How far left of its corner the Draw panel stands: beside a floating Notes panel when the window holds both, else 0. */
export declare function besideOffset(p: { notesWidth: number | null; drawWidth: number; viewport: number }): number;
