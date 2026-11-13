/**
 * What is gone is forgotten (`crates/desktop.md` §Routing): the paths a
 * fact on the bus takes out of the place memory. A remembered place must
 * never lead to a dead page — so a goal deleted, a workflow deleted, a
 * project deleted, a conversation deleted, a checkout closed and a hosted
 * workspace left each name the paths that stood on them. An **archived**
 * thing is still there, and is kept.
 *
 * Facts only, no React and no store: `App.tsx` hears the bus and forgets
 * what this names. What went while the app was closed raises no fact; the
 * screen that opens on it finds nothing and leaves (`useGonePlace.ts`).
 * Plain `.mjs`, so `node --test` reads it.
 */

import { isMember } from "./hostedModel.mjs";

import { href } from "../routeModel.mjs";
import { draftKey } from "../ui/composerDraftModel.mjs";
import { storedAddressKey } from "../views/_studio/addressModel.mjs";

/** A route's path, without its hash mark. */
function pathFor(route) {
  return href(route).slice(1);
}

/**
 * The paths an engine fact takes out of the memory; none for a fact that
 * removes nothing.
 * @param {{type?: string} & Record<string, unknown>} payload an engine event's payload
 * @returns {string[]}
 */
export function gonePaths(payload) {
  const text = (v) => (typeof v === "string" && v ? v : null);
  switch (payload?.type) {
    case "goal_deleted": {
      const id = text(payload.goal);
      return id ? [pathFor({ name: "goal", id }), pathFor({ name: "workbench", scope: "goal", id })] : [];
    }
    case "workflow_deleted": {
      const id = text(payload.workflow);
      return id ? [pathFor({ name: "workflow", id })] : [];
    }
    case "project_deleted": {
      const id = text(payload.project);
      return id ? [pathFor({ name: "workbench", scope: "workstream", id })] : [];
    }
    case "conversation_changed": {
      const id = text(payload.id);
      return id && payload.change === "deleted" ? [pathFor({ name: "conversation", id })] : [];
    }
    case "workstream_changed": {
      const id = text(payload.workstream);
      const closed = payload.state && typeof payload.state === "object" && payload.state.state === "closed";
      return id && closed ? [pathFor({ name: "workbench", scope: "workstream", id })] : [];
    }
    default:
      return [];
  }
}

/** The wire kind of a channel's record (`KIND_CHANNEL`). */
const KIND_CHANNEL = 33405;

/**
 * The channel a `conversation` frame says was **removed** — a standing one
 * or a direct one, deleted here or in another window — or `null` for any
 * other frame. Its place goes ({@link channelPaths}) and what was kept of
 * it by its scope: the draft in its box, whom it addressed, where its
 * thread was being read.
 * @param {{scope?: string, kind?: number, snapshot?: boolean, removed?: boolean} | null | undefined} frame
 * @returns {string | null}
 */
export function goneChannel(frame) {
  if (!frame || frame.snapshot !== true || frame.removed !== true || frame.kind !== KIND_CHANNEL) return null;
  return typeof frame.scope === "string" && frame.scope ? frame.scope : null;
}

/**
 * The places a channel stood at: its page as a channel and as a direct
 * channel — the id alone does not say which it was, and forgetting a place
 * nobody kept costs nothing.
 * @param {string} id
 * @returns {string[]}
 */
export function channelPaths(id) {
  return [pathFor({ name: "channel", id }), pathFor({ name: "dm", id })];
}

/**
 * What the browser's storage keeps by a scope that is gone: the draft in
 * its box and whom it addressed. A channel made again under the id starts
 * with neither.
 * @param {string} scope
 * @returns {string[]}
 */
export function scopePrefKeys(scope) {
  return [draftKey(scope), storedAddressKey(scope)];
}

/**
 * The paths of the hosted workspaces a person is no longer in: every host
 * a remembered place names that the memberships no longer list as one they
 * are a member of.
 * @param {readonly string[]} remembered the paths the memory keeps
 * @param {readonly string[]} hosts the hosts the person is a member of
 * @returns {string[]} one path a host, `/hosts/<host>`
 */
export function leftHosts(remembered, hosts) {
  const out = new Set();
  for (const path of remembered) {
    const m = /^\/hosts\/([^/]+)\//.exec(path);
    if (!m) continue;
    let host = m[1];
    try {
      host = decodeURIComponent(host);
    } catch {
      // A host that is no escape is no host: its place goes.
    }
    if (!hosts.includes(host)) out.add(`/hosts/${m[1]}`);
  }
  return [...out];
}

/**
 * The keys of the hosts the person is a member of, from the hosted sections
 * as the node answered them. A section missing its host or its key — an
 * answer of another shape, a build in between — is left out rather than
 * thrown on: this is read in an effect of `App`'s, above every inner
 * boundary, where a throw took the whole window.
 * @param {readonly { host?: { host?: { pubkey?: unknown } | null, state?: { state?: string } | null } | null }[] | null | undefined} hosted
 * @returns {string[]}
 */
export function memberHostKeys(hosted) {
  const out = [];
  for (const section of hosted ?? []) {
    const host = section?.host;
    if (!isMember(host)) continue;
    const key = host?.host?.pubkey;
    if (typeof key === "string" && key) out.push(key);
  }
  return out;
}

/**
 * Whether the memberships the shell holds are the node's answer: the
 * workspace was read, the node is there, and the last read of the hosts
 * answered. Only then does a host missing from the list mean the person is
 * no longer in it — a read that failed leaves the list as it was, or empty
 * at a first load, and says nothing of who left.
 * @param {{ready: boolean, offline: string | null, hostsRead: boolean}} workspace
 */
export function membershipsKnown(workspace) {
  return !!workspace.ready && !workspace.offline && !!workspace.hostsRead;
}
