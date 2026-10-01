/**
 * What a principal is called on this screen, with no React in it — the one
 * map every author line, row and chip reads a name from.
 *
 * A member is called by the label they gave themselves, an agent by its
 * name. **You** are *You* until you name yourself (Settings › Identity);
 * then the name every other workspace sees is the one you see too. The node
 * makes up no label for anybody — a member who gave none has none on the
 * wire — so nothing here compares a label with words: a label is a name, or
 * it is absent.
 */

import { t } from "../i18n/l10n.mjs";

/**
 * Every known principal's name by pubkey.
 * @param {string | null | undefined} me this node's own pubkey; nothing before the node answered
 * @param {readonly {pubkey: string, label?: string | null}[] | null | undefined} members
 * @param {readonly {pubkey: string, name: string}[] | null | undefined} agents
 * @returns {Map<string, string>}
 */
export function principalNames(me, members, agents) {
  const names = new Map();
  for (const member of members ?? []) {
    const label = member.label?.trim();
    if (label) names.set(member.pubkey, label);
  }
  for (const agent of agents ?? []) names.set(agent.pubkey, agent.name);
  if (me && !names.has(me)) names.set(me, t("shell-profile-menu-you"));
  return names;
}

/**
 * A principal's name, else the head of its key — somebody this workspace
 * holds no member and no agent for.
 * @param {ReadonlyMap<string, string>} names
 * @param {string} pubkey
 */
export function nameIn(names, pubkey) {
  return names.get(pubkey) ?? `${pubkey.slice(0, 8)}…`;
}
