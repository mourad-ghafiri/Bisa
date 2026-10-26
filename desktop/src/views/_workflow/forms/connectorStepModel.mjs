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

/**
 * What the account picker draws for an account the call names and the
 * picker does not offer: one of this machine's by id that is no longer
 * here, an input that no longer exists, one that is no account input any
 * more, or an account input of another connector. Nothing for the default,
 * for an account the picker offers, or — for one by id — while the
 * machine's accounts are not read yet (`accounts` null).
 * @param {import("./connectorStepModel.d.mts").StepAccount} account
 * @param {readonly {id: string}[] | null} accounts this machine's accounts of the connector, or null while unread
 * @param {readonly {name: string, kind: string, connector?: string | null}[]} inputs
 * @param {string | null | undefined} connector the call's connector
 * @returns {string | null}
 */
export function strayAccount(account, accounts, inputs, connector) {
  if (!account) return null;
  if (typeof account === "object" && "input" in account) {
    const name = account.input;
    const input = (inputs ?? []).find((i) => i.name === name);
    if (!input) return t("workflow-connector-step-form-input-no-such", { name });
    if (input.kind !== "account") return t("workflow-connector-step-form-input-not-account", { name });
    const other = input.connector ?? null;
    if (other === (connector ?? null)) return null;
    return other === null
      ? t("workflow-connector-step-form-input-no-connector", { name })
      : t("workflow-connector-step-form-input-other-connector", { name, other, connector: connector ?? "" });
  }
  if (accounts === null) return null;
  return accounts.some((a) => a.id === account) ? null : t("workflow-connector-step-form-account-not-here", { id: account });
}

/**
 * The hint under the connector picker: the read's failure when the list
 * could not be read — never *nothing installed* over a list nobody read —
 * else where more come from, or that none is installed.
 * @param {{count: number, error: string | null | undefined}} state
 */
export function connectorPickerHint({ count, error }) {
  if (error) return t("workflow-connector-step-form-connectors-not-read", { error });
  return count === 0 ? t("workflow-connector-step-form-nothing-installed-here-yet-settings-connectors") : t("workflow-connector-step-form-connector-installed-here-slug");
}

/**
 * The hint under the operation picker: the definition's read failure when
 * it failed — the one place it is said — else the words given (the
 * operation's description, or what the picker offers).
 * @param {string | null | undefined} error
 * @param {string} words
 */
export function definitionHint(error, words) {
  return error ? t("workflow-connector-step-form-definition-not-read", { error }) : words;
}

/**
 * The line under the account picker: nothing while the definition is on
 * its way or its read failed (the operation's hint says so), *no account
 * yet* when the machine holds none and no input stands in, else the
 * default's words.
 * @param {{loading: boolean, error: string | null | undefined, count: number, inputs: number}} state
 * @returns {string | null}
 */
export function accountsNote({ loading, error, count, inputs }) {
  if (error || loading) return null;
  if (count === 0 && inputs === 0) return t("workflow-connector-step-form-machine-holds-no-account-yet-default");
  return t("workflow-connector-step-form-connector-s-default-account-node-one");
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
