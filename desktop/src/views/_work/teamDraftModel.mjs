/**
 * What the team dialog holds and what a save of it writes (06 — Agents and
 * teams), out of the component so a test can reach it.
 *
 * A **draft** is the team as it stands, its roster as wire keys — and never
 * the members nobody stored: the core agents arrive in `members` at read
 * time and are in no stored list, so a draft that held one would offer a
 * member you can appear to remove and cannot. A **save** names what the
 * dialog owns. Editing, it names only what changed: the node changes the
 * fields a body names on the record as it stands, so a dialog left open
 * while a member was removed elsewhere saves its rename and leaves the
 * roster as it now is.
 */

import { assigneeToWire, requireWire } from "./assigneeWire.mjs";
import { isImplicitMember } from "../rosterModel.mjs";

/** One value as the wire would carry it. */
const said = (value) => JSON.stringify(value ?? null);

/** A blank draft: a team is named first and staffed after. */
export function emptyTeamDraft() {
  return { name: "", purpose: "", photo: null, members: [], tags: [] };
}

/**
 * The draft of a team as it stands.
 * @param {{name: string, purpose?: string | null, photo?: object | null, members?: readonly object[], tags?: readonly string[]}} team
 */
export function teamDraftOf(team) {
  return {
    name: team.name,
    purpose: team.purpose ?? "",
    photo: team.photo ?? null,
    members: (team.members ?? []).filter((m) => !isImplicitMember(m)).map(assigneeToWire),
    tags: [...(team.tags ?? [])],
  };
}

/** Whether the dialog may save: a name, and no write on its way. @param {{name: string}} d @param {boolean} busy */
export function maySaveTeam(d, busy) {
  return !busy && d.name.trim() !== "";
}

/** The fields a PATCH names, from a draft. */
const patchFields = (d) => ({
  name: d.name.trim(),
  purpose: d.purpose.trim() || null,
  photo: d.photo,
  members: d.members.map(requireWire),
  tags: d.tags,
});

/**
 * What a save sends.
 *
 * A `team:` key can only have arrived by editing a team that already held
 * one — a hand-edited record. Editing, it is sent as it stands rather than
 * dropped: the node refuses nesting and names the member, which is the
 * sentence worth reading. A new team starts from an empty roster and is
 * typed to the two legal arms, so nothing is lost narrowing there.
 * @param {ReturnType<typeof emptyTeamDraft>} d the draft
 * @param {{id: string} | null} team the team edited; `null` for a new one
 * @param {ReturnType<typeof emptyTeamDraft> | null} [seeded] the draft as the dialog opened
 * @returns {{kind: "patch", id: string, body: object} | {kind: "create", body: object}}
 */
export function teamSaveOf(d, team, seeded = null) {
  const now = patchFields(d);
  if (team) {
    if (!seeded) return { kind: "patch", id: team.id, body: now };
    const before = patchFields(seeded);
    return { kind: "patch", id: team.id, body: Object.fromEntries(Object.entries(now).filter(([field, value]) => said(value) !== said(before[field]))) };
  }
  return {
    kind: "create",
    body: {
      name: now.name,
      ...(now.purpose ? { purpose: now.purpose } : {}),
      ...(d.photo ? { photo: d.photo } : {}),
      members: now.members.filter((m) => !("team" in m)),
      tags: d.tags,
    },
  };
}
