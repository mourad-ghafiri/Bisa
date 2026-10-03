/**
 * The words of Settings › Connectors, pure over the node's answers:
 * `GET /connectors` (every definition with its operations and how many
 * accounts this machine holds), `GET /connectors/{cid}` (the definition
 * whole and its accounts — which secret fields are set and where they live,
 * never a value), a check's answer, an OAuth start, a validation. The panel
 * draws; this file decides what each row says and in what tone.
 */

import { t, tx } from "../../i18n/l10n.mjs";

/**
 * The secret fields each auth scheme holds — the core's
 * `AuthScheme::fields()`, mirrored here so the dialog asks for exactly what
 * the scheme uses. An OAuth2 account also takes a pasted `access_token` for
 * a platform whose token a person already holds; a `jwt` account holds the
 * private key the platform signs each request with.
 */
export const SECRET_FIELDS_BY_SCHEME = Object.freeze({
  none: [],
  api_key: ["api_key"],
  bearer: ["token"],
  basic: ["username", "password"],
  oauth2: ["client_id", "client_secret", "access_token"],
  jwt: ["private_key"],
});

/** The one word for each field, as a form labels it. */
export const SECRET_FIELD_LABEL = Object.freeze({
  api_key: t("settings-decisions-panel-api-key"),
  token: t("settings-code-host-panel-token-2"),
  username: t("settings-connectors-username-email"),
  password: t("settings-connectors-password-api-token"),
  client_id: t("settings-connectors-oauth-client-id"),
  client_secret: t("settings-connectors-oauth-client-secret"),
  access_token: t("settings-connectors-access-token-pasted-optional"),
  refresh_token: t("settings-connectors-refresh-token"),
  private_key: t("settings-connectors-private-key-pem"),
});

/**
 * The fields whose value spans lines — a PEM block — and so take a text area
 * rather than a one-line password box.
 */
const MULTILINE_SECRET_FIELDS = Object.freeze(["private_key"]);

/** Whether a secret field is entered across lines. */
export function isMultilineSecret(field) {
  return MULTILINE_SECRET_FIELDS.includes(field);
}

/** The scheme's word from a row's `auth` string or a definition's `auth` object. */
export function schemeWord(auth) {
  if (!auth) return "none";
  return typeof auth === "string" ? auth : (auth.scheme ?? "none");
}

/** The secret fields a scheme's account can hold. */
export function secretFields(auth) {
  return [...(SECRET_FIELDS_BY_SCHEME[schemeWord(auth)] ?? [])];
}

/** Whether a scheme needs an account at all. */
export function needsAccount(auth) {
  return schemeWord(auth) !== "none";
}

/** Whether *Connect* — the browser flow — applies. */
export function connectApplies(auth) {
  return schemeWord(auth) === "oauth2";
}

/**
 * The verbs of one account row: the one it shows — *Connect* while an OAuth
 * account holds no token, *Check* otherwise — and the rest, in the order the
 * row's menu lists them. A row of five buttons read as a toolbar; one verb
 * and a menu reads as a row.
 * @param {{ default: boolean, secrets_set: readonly string[] }} account
 * @param {boolean} oauth
 * @returns {{ main: "connect" | "check", more: ("connect" | "secrets" | "check" | "default" | "forget")[] }}
 */
export function accountVerbs(account, oauth) {
  const connected = account.secrets_set.includes("access_token");
  const main = oauth && !connected ? "connect" : "check";
  /** @type {("connect" | "secrets" | "check" | "default" | "forget")[]} */
  const more = [];
  if (oauth && connected) more.push("connect");
  more.push("secrets");
  if (main !== "check") more.push("check");
  if (!account.default) more.push("default");
  more.push("forget");
  return { main, more };
}

/** The redirect URI a person registers at the platform, for a port. */
export function redirectUri(port) {
  return `http://127.0.0.1:${port}/connectors/oauth/callback`;
}

/**
 * Whether a connector is the person's own definition — `"local"` on the wire
 * — and not one installed from the catalog (`{ catalog: { slug } }`, the
 * wire's `Origin`). Only a person's own is edited or deleted here; the
 * catalog's is refreshed by the platform.
 * @param {import("../../types").Origin | null | undefined} origin
 */
export function isYours(origin) {
  return !(origin !== null && typeof origin === "object" && "catalog" in origin);
}

/** "from the catalog" or "yours". */
export function originWords(origin) {
  return isYours(origin) ? t("settings-connectors-yours") : t("settings-connectors-from-catalog");
}

/**
 * One definition's facts in a line: the scheme, the hosts, the operations
 * and how many write, the accounts.
 * @param {import("../../types").ConnectorRow | null | undefined} row
 */
export function connectorLine(row) {
  if (!row) return "";
  const ops = row.operations ?? [];
  const writes = ops.filter((o) => o.writes).length;
  const opWords = t("settings-connectors-operation-operations-writes-write", { ops: ops.length, writes, flag: (writes > 0) ? "yes" : "no" });
  const accounts = row.accounts === 0 ? t("settings-connectors-account-here") : t("settings-connectors-account-accounts-here", { accounts: row.accounts });
  return `${schemeWord(row.auth)} · ${(row.hosts ?? []).join(", ")} · ${opWords} · ${accounts}`;
}

/**
 * The account rows: the default first, then by label.
 * @param {import("../../types").ConnectorAccountRow[] | null | undefined} accounts
 */
export function accountRows(accounts) {
  return [...(accounts ?? [])].sort((a, b) => Number(b.default) - Number(a.default) || a.label.localeCompare(b.label));
}

/**
 * Which secret fields an account holds and where they live — never a value.
 * @param {import("../../types").ConnectorAccountRow | null | undefined} row
 * @param {string} auth the connector's scheme word
 * @returns {{tone: "ok" | "warn" | "quiet", text: string}}
 */
export function secretsLine(row, auth) {
  if (!row) return { tone: "quiet", text: "" };
  const where = row.token_source === "keyring" ? t("settings-code-host-accounts-os-keyring") : t("settings-connectors-file-under-identity");
  const set = row.secrets_set ?? [];
  const wanted = secretFields(auth).filter((f) => f !== "access_token");
  if (set.length === 0) {
    return { tone: wanted.length === 0 ? "quiet" : "warn", text: wanted.length === 0 ? t("settings-connectors-secret-needed") : t("settings-connectors-secret-set-yet-wanted", { wanted: wanted.join(", ") }) };
  }
  const missing = wanted.filter((f) => !set.includes(f));
  if (schemeWord(auth) === "oauth2") {
    const connected = set.includes("access_token");
    const words = connected ? t("settings-connectors-connected") : t("settings-connectors-client-set-connected-yet");
    return { tone: connected ? "ok" : "warn", text: `${words} · ${set.join(", ")} ${where}` };
  }
  const held = `${set.join(", ")} ${where}`;
  return { tone: missing.length === 0 ? "ok" : "warn", text: missing.length > 0 ? t("settings-connectors-held-still-wanted", { held, missing: missing.join(", ") }) : held };
}

/**
 * What an OAuth account knows about its token, in one line; `null` for a
 * scheme that has none.
 * @param {import("../../types").ConnectorAccountRow | null | undefined} row
 * @param {number} now unix seconds
 */
export function oauthLine(row, now) {
  const facts = row?.oauth;
  if (!facts) return null;
  const scope = facts.scope ? t("settings-connectors-scope-tail", { scope: facts.scope }) : "";
  if (facts.expires_at === null || facts.expires_at === undefined) return t("settings-connectors-token-without-expiry", { scope });
  if (facts.expired || facts.expires_at <= now) return t("settings-connectors-token-expired-refreshed-next-call", { scope });
  const left = facts.expires_at - now;
  const words = left < 3600 ? t("settings-connectors-min", { left: Math.max(1, Math.round(left / 60)) }) : left < 86400 ? t("settings-connectors-hours", { left: Math.round(left / 3600) }) : t("settings-connectors-days", { left: Math.round(left / 86400) });
  return t("settings-connectors-token-good", { words, scope });
}

/**
 * A check's answer as a line. `null` before the check.
 * @param {import("../../types").AccountCheck | null | undefined} check
 * @param {string} name the connector's name
 * @returns {{tone: "ok" | "warn" | "danger" | "quiet", text: string}}
 */
export function checkLine(check, name = t("settings-connectors-platform")) {
  if (!check) return { tone: "quiet", text: t("settings-code-host-accounts-checked-yet") };
  const status = check.status ? ` (${check.status})` : "";
  switch (check.state) {
    case "connected":
      return { tone: "ok", text: t("settings-connectors-connected-answered", { name, status }) };
    case "refused":
      return { tone: "danger", text: t("settings-connectors-refused-credential-set-secrets-again-connect", { name, status, reason: check.reason, flag: (check.reason) ? "yes" : "no" }) };
    case "unreachable":
      return { tone: "warn", text: t("settings-connectors-did-answer-answer", { name, reason: check.reason, flag: (check.reason) ? "yes" : "no" }) };
    case "no_check":
      return { tone: "quiet", text: t("settings-connectors-s-definition-names-check-operation-first", { name }) };
    default:
      // A state this desktop does not know — a newer node's word — is said,
      // never read as nothing happened.
      return { tone: "warn", text: t("settings-connectors-check-answered-state-desktop-does-know", { state: String(check.state) }) };
  }
}

/**
 * What an account's form opens on: the label and each parameter as the text
 * a person edits — a value that is not a string shown as its JSON.
 * @param {import("../../types").ConnectorAccountRow | null | undefined} account
 * @returns {{label: string, values: Record<string, string>}}
 */
export function accountDraft(account) {
  return {
    label: account?.label ?? "",
    values: Object.fromEntries(Object.entries(account?.params ?? {}).map(([k, v]) => [k, typeof v === "string" ? v : JSON.stringify(v)])),
  };
}

/**
 * The body of `PUT /connectors/{cid}/accounts`, by name: the account's id
 * when one is edited (`null` adds one), the label, the parameters that hold
 * something, and the secret fields that were typed — one left blank keeps
 * what is stored, so it is not sent; no secret typed, no `secrets` key.
 *
 * A parameter the account holds as something other than a string — a
 * number, a switch — and that was not retyped is sent back **as it was
 * held**: the form shows it as text, and an edit that only set a secret must
 * not turn `8443` into `"8443"`.
 * @param {{label: string, values: Record<string, string>, secrets: Record<string, string | undefined>}} draft
 * @param {import("../../types").ConnectorAccountRow | null | undefined} account the account being edited, or null
 * @returns {import("../../types").NewConnectorAccount}
 */
export function accountBody(draft, account) {
  const held = account?.params ?? {};
  /** @type {Record<string, unknown>} */
  const params = {};
  for (const [name, typed] of Object.entries(draft.values ?? {})) {
    if (typeof typed !== "string" || typed.trim() === "") continue;
    const was = held[name];
    params[name] = was !== undefined && typeof was !== "string" && JSON.stringify(was) === typed ? was : typed;
  }
  const secrets = typedSecrets(draft.secrets);
  /** @type {import("../../types").NewConnectorAccount} */
  const body = { id: account?.id ?? null, label: (draft.label ?? "").trim(), params };
  if (Object.keys(secrets).length > 0) body.secrets = secrets;
  return body;
}

/** The secret fields a person typed something into, by name — a blank one keeps what is stored. */
export function typedSecrets(secrets) {
  return Object.fromEntries(Object.entries(secrets ?? {}).filter(([, v]) => typeof v === "string" && v.trim() !== ""));
}

/**
 * Whether the account form may be saved: a label, no write on its way, and
 * the connector's definition read — until it is, the form cannot show the
 * parameters the connector asks for, and an account saved without them is
 * one the first call refuses.
 * @param {{label: string, busy: boolean, definitionRead: boolean}} state
 */
export function maySaveAccount({ label, busy, definitionRead }) {
  return !busy && definitionRead && label.trim() !== "";
}

/**
 * The checks on screen without one account's: what *Check* answered was
 * about the secrets the account held then, so an account whose secrets were
 * set again, connected again, or forgotten has no answer standing.
 * @template T
 * @param {Record<string, T>} checks
 * @param {string} account
 * @returns {Record<string, T>}
 */
export function withoutCheck(checks, account) {
  if (!(account in checks)) return checks;
  const next = { ...checks };
  delete next[account];
  return next;
}

/** The registry key of the loopback port the OAuth callback listens on — a machine setting. */
export const OAUTH_PORT_KEY = "connectors.oauth.port";

/**
 * The callback port as the node resolves it, or `null` until the resolved
 * settings are read: the redirect URI is something a person copies into
 * another platform's console, so it is never said from a guess.
 * @param {readonly {key: string, value: unknown}[] | null | undefined} resolved
 * @returns {number | null}
 */
export function oauthPort(resolved) {
  const value = resolved?.find((s) => s.key === OAUTH_PORT_KEY)?.value;
  return typeof value === "number" && Number.isInteger(value) && value > 0 ? value : null;
}

/** The toast when Save found problems: how many, and where they are shown. */
export function saveRefusedWords(count) {
  return count === 1 ? t("settings-connectors-saved-one-problem-listed-below") : t("settings-connectors-saved-problems-listed-below", { count });
}

/** The blurb beside *Connect*, for an OAuth2 scheme. */
export function connectWords(port) {
  return t("settings-connectors-register-own-oauth-client-platform-redirect", { port: redirectUri(port) });
}

/** The problems of a definition, one line each; empty when it validates. */
export function problemLines(validation) {
  if (!validation) return [];
  return (validation.problems ?? []).map((p) => (p.field ? `${p.field}: ${tx(p.text)}` : tx(p.text)));
}

/**
 * A definition typed as JSON. `ok` with the value when it parses to an
 * object; the parser's words otherwise.
 * @param {string} text
 * @returns {{ok: true, value: object} | {ok: false, error: string}}
 */
export function parseDefinition(text) {
  try {
    const value = JSON.parse(text);
    if (!value || typeof value !== "object" || Array.isArray(value)) return { ok: false, error: t("settings-connectors-definition-json-object") };
    return { ok: true, value };
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : String(e) };
  }
}

/** The keys of a definition (`ConnectorDefinition`), in the order the editor shows them: every field of the record but the two the store stamps. */
export const DEFINITION_KEYS = Object.freeze(["id", "name", "description", "tags", "base_url", "hosts", "insecure_tls", "auth", "params", "operations", "check"]);

/**
 * The definition the editor opens on for a connector installed here: the
 * record's own fields **by name** — never its origin or the moment it was
 * made, which the store stamps, nor anything a record may come to carry that
 * a definition does not. A field the record leaves out is left out.
 * @param {Record<string, unknown>} connector a `Connector` record
 */
export function definitionOf(connector) {
  return Object.fromEntries(DEFINITION_KEYS.filter((key) => connector?.[key] !== undefined).map((key) => [key, connector[key]]));
}

/** The skeleton a custom definition starts from. */
export function emptyDefinition() {
  return {
    id: "my-service",
    name: t("settings-connectors-my-service"),
    description: t("settings-connectors-what-platform-one-sentence"),
    tags: ["ops"],
    base_url: "https://api.example.com",
    hosts: ["api.example.com"],
    auth: { scheme: "bearer" },
    params: [],
    operations: [
      {
        id: "ping",
        name: "Ping",
        description: t("settings-connectors-answers-service-s-status"),
        method: "get",
        path: "/status",
        params: [],
        output: { select: null },
        writes: false,
      },
    ],
    check: "ping",
  };
}

/** The toasts. */
export function addedWords(label) {
  return t("settings-connectors-added-set-secrets-then-press-check", { label });
}
export function defaultWords(label) {
  return t("settings-connectors-default-account", { label });
}
export function connectedWords(label) {
  return t("settings-connectors-connected-tokens-keystore", { label });
}
export function secretsSetWords(fields) {
  return fields.length === 0 ? t("settings-connectors-nothing-changed") : t("settings-connectors-set-sent-once-never-displayed", { fields: fields.join(", ") });
}
export function forgottenWords(label) {
  return t("settings-connectors-forgotten-every-secret-held", { label });
}
export function openedWords() {
  return t("settings-connectors-browser-opening-platform-s-consent-page");
}
