/** Types for `teamDraftModel.mjs`. */
import type { Assignee, AttachmentRef, TeamDef, TeamPatchBody } from "../../types";

/** A team as the dialog holds it: its roster as wire keys, the core agents never among them. */
export interface TeamDraft {
  name: string;
  purpose: string;
  photo: AttachmentRef | null;
  members: string[];
  tags: string[];
}

/** The body of a new team. */
export interface NewTeamBody {
  name: string;
  purpose?: string;
  photo?: AttachmentRef;
  members: ({ human: string } | { agent: string })[];
  tags: string[];
}

export type TeamSave = { kind: "patch"; id: string; body: TeamPatchBody & { members?: Assignee[] } } | { kind: "create"; body: NewTeamBody };

export declare function emptyTeamDraft(): TeamDraft;
export declare function teamDraftOf(team: Pick<TeamDef, "name" | "purpose" | "photo" | "members" | "tags">): TeamDraft;
export declare function maySaveTeam(d: Pick<TeamDraft, "name">, busy: boolean): boolean;
/** What a save sends: the fields that changed of a team edited, or a new team. */
export declare function teamSaveOf(d: TeamDraft, team: Pick<TeamDef, "id"> | null, seeded?: TeamDraft | null): TeamSave;
