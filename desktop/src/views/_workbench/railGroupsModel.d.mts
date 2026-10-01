import type { AttachmentRef } from "../../types";

/** Per-group adornment kept in the `rail.groups` setting, keyed by group name. */
export interface GroupMeta {
  photo?: AttachmentRef;
}
export type RailGroups = Record<string, GroupMeta>;

export declare function groupPhoto(meta: RailGroups | null | undefined, name: string): AttachmentRef | null;
export declare function withGroupPhoto(meta: RailGroups | null | undefined, name: string, ref: AttachmentRef | null): RailGroups;
export declare function renameGroupMeta(meta: RailGroups | null | undefined, old: string, next: string): RailGroups;
