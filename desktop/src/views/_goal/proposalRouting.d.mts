import type { NeedsAction } from "../../types";

/** The subject an adoption's approval gate wears: `adopt:<workflow>`. */
export declare const ADOPT_SUBJECT: string;
/** Whether a gate's or an ask's subject names an adoption. */
export declare function namesAdoption(subject: string | null | undefined): boolean;
export declare function isAdoptProposal(action: NeedsAction | null | undefined): boolean;
export declare function adoptAction(actions: readonly NeedsAction[] | null | undefined): NeedsAction | null;
export declare function bandActions(actions: readonly NeedsAction[] | null | undefined): NeedsAction[];
