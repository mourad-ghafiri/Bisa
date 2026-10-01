/** Types for `workstreamFramesModel.mjs`. */

/** The facts that move one workstream's record, each naming it as `workstream`. */
export declare const RECORD_FRAMES: readonly string[];
/** The facts that move what every checkout of a project says, each naming the project. */
export declare const PROJECT_FRAMES: readonly string[];
/** Whether a fact of this type is worth reading every workstream's status again. */
export declare function movesStatuses(type: unknown): boolean;
/** Whether a fact moves this workstream: one that names it, or one about the project it is a checkout of. */
export declare function movesWorkstream(payload: { type?: unknown; workstream?: unknown; project?: unknown } | null | undefined, wid: string, project: string | null | undefined): boolean;
