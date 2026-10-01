/** Types for `governanceModel.mjs`, plain JavaScript so `node --test` reads it. */

import type { Gate, GatePolicy, Governance } from "../../types";

export declare const GATES: readonly Gate[];
export declare const POLICIES: readonly GatePolicy["policy"][];
export declare function policyBody(policy: { policy?: string; pubkeys?: readonly unknown[] } | null | undefined): GatePolicy;
export declare function governanceBody(draft: Partial<Record<Gate, { policy?: string; pubkeys?: readonly unknown[] }>> | null | undefined): Governance;
