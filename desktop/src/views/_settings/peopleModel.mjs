/**
 * The people of a workspace, as facts (14-collaboration): the four roles in
 * words, what each may do, how a person's row reads, what an invitation
 * says about itself, and what removing or demoting somebody means. The
 * People panel and the Inbox draw these; nothing here fetches.
 *
 * The matrix itself is the node's (`GET /workspace/roles`) — a screen that
 * restated it would drift. What lives here is the wording around it.
 */

import { t } from "../../i18n/l10n.mjs";

/** The roles a person on another node may hold, in the order a picker offers them. */
export const HOSTED_ROLES = ["admin", "member", "guest"];

/** One word each — the picker's labels and the chips'. */
const ROLE_LABEL = {
  owner: t("settings-people-owner"),
  admin: t("settings-people-admin"),
  member: t("settings-people-member"),
  guest: t("settings-people-guest"),
};

/** What a role reaches, in one sentence — the picker's hint. */
export const ROLE_HINT = {
  owner: t("settings-people-workspace-theirs-everything"),
  admin: t("settings-people-every-channel-direct-messages-agents-mention"),
  member: t("settings-people-every-standing-channel-direct-messages-agents"),
  guest: t("settings-people-only-channels-they-put-direct-messages"),
};

/** The words a permission wears in the matrix table. */
const PERMISSION_LABEL = {
  read_workspace: t("settings-people-read-every-standing-channel"),
  post_in_channels: t("settings-people-post-channels-they-reach"),
  open_dms: t("settings-people-open-direct-messages"),
  mention_agents: t("settings-people-wake-agents-mention"),
  decide_gates: t("settings-people-decide-gates-governance-names-them"),
  manage_channels: t("settings-people-make-edit-channels"),
  manage_people: t("settings-people-invite-promote-remove-people"),
  manage_agents: t("settings-people-install-edit-agents"),
  manage_governance: t("settings-people-change-governance"),
  manage_settings: t("settings-people-change-settings"),
};

/** The word for a role; the role itself when this list has none. */
export function roleLabel(role) {
  return ROLE_LABEL[role] ?? String(role ?? "");
}

/** The word for a permission; the key itself when this list has none. */
export function permissionLabel(p) {
  return PERMISSION_LABEL[p] ?? String(p ?? "");
}

/**
 * The matrix as rows for a table: one per permission, a cell per role, from
 * the node's roles. Roles come in the order the node listed them.
 * @param {{ roles: { role: string, permissions: string[] }[], permissions: { permission: string, words: string }[] } | null | undefined} matrix
 */
export function matrixRows(matrix) {
  if (!matrix) return { roles: [], rows: [] };
  const roles = matrix.roles.map((r) => r.role);
  const rows = matrix.permissions.map((p) => ({
    permission: p.permission,
    label: permissionLabel(p.permission),
    words: p.words,
    holds: roles.map((role) => (matrix.roles.find((r) => r.role === role)?.permissions ?? []).includes(p.permission)),
  }));
  return { roles, rows };
}

/** A person's name as a row reads it: the label, else the key's first letters. */
export function personName(person) {
  const label = person?.label ?? "";
  if (label.trim()) return label.trim();
  const key = String(person?.pubkey ?? "");
  return key ? `${key.slice(0, 8)}…` : t("settings-people-someone");
}

/**
 * A person's second line: how they got here and what they use.
 * @param {{ role: string, client?: string | null, invited_by?: string | null, channels?: string[] }} person
 */
export function personWords(person) {
  const parts = [];
  if (person.role === "guest") {
    const n = (person.channels ?? []).length;
    parts.push(n === 0 ? t("settings-people-channel-yet") : n === 1 ? t("settings-people-on-channel", { channel: person.channels[0] }) : t("settings-people-channels", { n }));
  }
  if (person.client) parts.push(t("settings-people-from", { client: person.client }));
  return parts.join(" · ");
}

/** What changing a role means, said before it is done. */
export function roleChangeWords(from, to) {
  if (from === to) return null;
  if (to === "guest") return t("settings-people-they-keep-only-channels-they-put");
  if (to === "member") return from === "guest" ? t("settings-people-every-standing-channel-opens-them-they") : t("settings-people-they-lose-channels-people-every-standing");
  if (to === "admin") return t("settings-people-they-may-make-channels-invite-promote");
  return null;
}

/** What removing a person means, said before it is done. */
export const REMOVE_WORDS =
  t("settings-people-they-stop-receiving-anything-come-off");

/** An invitation's tone per state, and whether it still means something. */
const INVITE_STATE = Object.freeze({
  pending: { tone: "accent", open: true },
  requested: { tone: "warn", open: true },
  accepted: { tone: "ok", open: false },
  refused: { tone: "quiet", open: false },
  revoked: { tone: "quiet", open: false },
  expired: { tone: "quiet", open: false },
});

/** An invitation's state, in the catalog's words and a tone. */
export function inviteState(invite) {
  const state = invite?.state?.state ?? "pending";
  const known = Object.hasOwn(INVITE_STATE, state) ? INVITE_STATE[state] : { tone: "quiet", open: false };
  return { word: t("settings-people-invite-state", { state }), ...known };
}

/** The invitations that still mean something, newest first, then the rest. */
export function orderInvites(invites) {
  const all = [...(invites ?? [])];
  all.sort((a, b) => {
    const oa = inviteState(a).open ? 0 : 1;
    const ob = inviteState(b).open ? 0 : 1;
    return oa - ob || (b.created_at ?? 0) - (a.created_at ?? 0);
  });
  return all;
}

/** How long an invitation lives, in words, from the hours setting. */
export function expiryWords(hours) {
  const h = Number(hours) || 0;
  if (h <= 0) return "";
  if (h < 24) return t("settings-people-hour-hours", { h });
  const d = Math.round(h / 24);
  return t("settings-people-day-days", { d });
}

/** What an invitation offers, in one line: the role and the channels. */
export function inviteOffer(invite) {
  const role = roleLabel(invite?.role);
  const channels = invite?.channels ?? [];
  if (invite?.role === "guest") {
    return channels.length ? t("settings-people-role-on-channels", { role, channels: channels.map((c) => `#${c}`).join(", ") }) : t("settings-people-channel-yet-2", { role });
  }
  return role;
}

/**
 * Why a key typed under *Admit by key* cannot be admitted, or null when it
 * can: it is 64 hex characters, and it is nobody already here — not a
 * member, not you.
 * @param {string} raw @param {readonly {pubkey: string}[]} people @param {string} me
 * @returns {{key: string, problem: string | null}}
 */
export function admitKey(raw, people, me) {
  const key = String(raw ?? "").trim().toLowerCase();
  if (!key) return { key, problem: "" };
  if (!/^[0-9a-f]{64}$/.test(key)) return { key, problem: t("settings-people-key-64-hexadecimal-characters") };
  if (String(me ?? "").toLowerCase() === key) return { key, problem: t("settings-people-own-key") };
  if ((people ?? []).some((p) => String(p.pubkey).toLowerCase() === key)) return { key, problem: t("settings-people-already-member") };
  return { key, problem: null };
}
