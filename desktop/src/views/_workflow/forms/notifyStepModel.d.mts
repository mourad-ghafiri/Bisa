/** Types for `notifyStepModel.mjs`. */
export declare function scopeListed(
  scope: string | null | undefined,
  channelIds: readonly string[],
  textInputNames: readonly string[],
  goalIds?: readonly string[],
  workstreamIds?: readonly string[],
): boolean;
export interface ScopeOption {
  value: string;
  label: string;
}
export declare function scopeOptions(lists: {
  channels: readonly { id: string; name: string }[];
  goals: readonly { id: string; title?: string | null; statement?: string }[];
  workstreams: readonly { project_name?: string | null; workstream: { id: string; name?: string | null } }[];
  textInputs: readonly { name: string }[];
}): { first: string; channels: ScopeOption[]; goals: ScopeOption[]; workstreams: ScopeOption[]; inputs: ScopeOption[] };
