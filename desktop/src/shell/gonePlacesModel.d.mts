/** Types for `gonePlacesModel.mjs`. */

/** The paths an engine fact takes out of the place memory; none for a fact that removes nothing. */
export declare function gonePaths(payload: { type?: string } & Record<string, unknown>): string[];
/** The paths — `/hosts/<host>` — of the hosted workspaces a remembered place names and the person is no longer a member of. */
export declare function leftHosts(remembered: readonly string[], hosts: readonly string[]): string[];
/** The keys of the hosts the person is a member of; a section without its host's key is left out, never thrown on. */
export declare function memberHostKeys(hosted: readonly { host?: { host?: { pubkey?: unknown } | null; state?: { state?: string } | null } | null }[] | null | undefined): string[];
/** Whether the memberships the shell holds are the node's answer — the only list a host's absence from means the person left. */
export declare function membershipsKnown(workspace: { ready: boolean; offline: string | null; hostsRead: boolean }): boolean;
