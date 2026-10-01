/**
 * What a `connector` step's form says and how it spells its account — pure,
 * so `node --test` reads it and the form only draws.
 *
 * The account is one `<select>` value: the connector's default (`""`), one
 * of this machine's by id (`fixed:<id>`) or an input the run starts with
 * (`input:<name>`) — the shape a template stays portable in. A write is
 * flagged, and either gated by an approval upstream or owned by the person's
 * `unattended`; the validator refuses silence (`ungated_write`).
 */

import { t } from "../../../i18n/l10n.mjs";

export const DEFAULT_ACCOUNT = "";
const FIXED = "fixed:";
const INPUT = "input:";

/** The select's value for a step's account. */
export function accountValue(account) {
  if (!account) return DEFAULT_ACCOUNT;
  if (typeof account === "object" && "input" in account) return `${INPUT}${account.input}`;
  return `${FIXED}${account}`;
}

/** The step's account for a select's value. */
export function accountFromValue(v) {
  if (v === DEFAULT_ACCOUNT) return null;
  if (v.startsWith(INPUT)) return { input: v.slice(INPUT.length) };
  return v.slice(FIXED.length);
}

/** The select's value for one of this machine's accounts. */
export function fixedValue(id) {
  return `${FIXED}${id}`;
}

/** The select's value for an input of kind account. */
export function inputValue(name) {
  return `${INPUT}${name}`;
}

/**
 * The parameters the step sets that the operation no longer declares —
 * shown so they can be removed, never silently dropped.
 * @param {Record<string, string>} params
 * @param {readonly {name: string}[]} declared
 */
export function strayParams(params, declared) {
  return Object.keys(params ?? {}).filter((name) => !declared.some((p) => p.name === name));
}

/**
 * A parameter set or cleared: an emptied one is absent from the wire, never
 * an empty string.
 * @param {Record<string, string> | null | undefined} params
 * @param {string} name
 * @param {string} value
 * @returns {Record<string, string>}
 */
export function withParam(params, name, value) {
  const next = { ...(params ?? {}) };
  if (value.trim() === "") delete next[name];
  else next[name] = value;
  return next;
}

/** A call without the person's word on a write: the key absent, as the step was born. */
function unowned(call) {
  const { unattended: _word, ...rest } = call;
  return rest;
}

/**
 * The call after another connector is picked — a `connector` step's, or a
 * connector start's. Everything chosen under the old one was the old one's:
 * its operation, its parameters, the account, and the person's word that a
 * write runs **unattended**. None carries over — above all that word, which
 * was given for one write and would otherwise cover the next one picked.
 * The empty option is `null`, never `""`.
 */
export function withConnector(call, connector) {
  return { ...unowned(call), connector: connector || null, operation: null, params: {}, account: null };
}

/**
 * The call after another operation is picked: the old operation's parameters
 * go, and so does `unattended` — said of one write, never inherited by
 * another, and meaningless on a read. The account is the connector's and stays.
 */
export function withOperation(call, operation) {
  return { ...unowned(call), operation: operation || null, params: {} };
}

/**
 * What a picker draws beside the connectors installed here: the one the call
 * names when it is not among them — so the select shows what the definition
 * holds, never *pick one* over a name that is still written.
 * @param {string | null | undefined} connector
 * @param {readonly {id: string}[]} installed
 * @returns {string | null} the option's words, or null when there is nothing stray
 */
export function strayConnector(connector, installed) {
  if (connector == null || installed.some((c) => c.id === connector)) return null;
  return t("workflow-connector-step-form-not-installed-here", { connector });
}

/**
 * The same for the operation, among the operations a picker offers
 * (`offered` — all of them for a step, the reads for a poll) out of the
 * connector's own (`all`): one the connector has and the picker does not
 * offer is a write where only a read goes; one it does not have is said so.
 * Nothing is said until the definition is read (`all` null).
 * @param {string | null | undefined} operation
 * @param {readonly {id: string}[]} offered
 * @param {readonly {id: string}[] | null} all
 * @returns {string | null}
 */
export function strayOperation(operation, offered, all) {
  if (operation == null || all === null || offered.some((o) => o.id === operation)) return null;
  return all.some((o) => o.id === operation) ? t("workflow-connector-step-form-writes-poll-only-reads", { operation }) : t("workflow-connector-step-form-not-one-of-its-operations", { operation });
}

/** What the form says beside a writing operation, by whether the person owned the write. */
export function writeWords(unattended) {
  return unattended
    ? t("workflow-connector-step-operation-changes-something-platform-runs-nobody")
    : t("workflow-connector-step-operation-changes-something-platform-put-approval");
}

/** The switch's words. */
export const UNATTENDED_LABEL = t("workflow-connector-step-runs-unattended");
export const UNATTENDED_HINT = t("workflow-connector-step-say-write-runs-no-approval-human");
