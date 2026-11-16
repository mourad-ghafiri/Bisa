/**
 * The words of Settings › Connectors. Run with
 * `node --test desktop/src/views/_settings/connectorsModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  SECRET_FIELDS_BY_SCHEME,
  OAUTH_PORT_KEY,
  SECRET_FIELD_LABEL,
  accountBody,
  accountDraft,
  accountVerbs,
  accountRows,
  addedWords,
  checkLine,
  connectApplies,
  connectWords,
  connectorLine,
  DEFINITION_KEYS,
  definitionOf,
  emptyDefinition,
  forgottenWords,
  isYours,
  maySaveAccount,
  needsAccount,
  oauthLine,
  oauthPort,
  originWords,
  parseDefinition,
  problemLines,
  redirectUri,
  saveRefusedWords,
  schemeWord,
  isMultilineSecret,
  secretFields,
  secretsLine,
  secretsSetWords,
  defaultWords,
  connectedWords,
  openedWords,
  typedSecrets,
  SECRET_FIELD_WHAT,
  connectUriWords,
  flowExpired,
  flowLanded,
  flowWords,
  mayEditDefinition,
  mayPaste,
  pendingFlow,
  renamedProblem,
  savedWords,
  validationFor,
} from "./connectorsModel.mjs";

const RUST = readFileSync(new URL("../../../../crates/bisa-core/src/connector.rs", import.meta.url), "utf8");

/** The `SecretField` variants, read out of the Rust so a field added there is asked for here. */
function rustSecretFields() {
  const m = RUST.match(/pub enum SecretField \{([\s\S]*?)\n\}/);
  assert.ok(m, "SecretField is declared in connector.rs");
  const out = [];
  for (const line of m[1].split("\n")) {
    const v = line.match(/^\s{4}([A-Z][A-Za-z0-9]*)\s*,/);
    if (v) out.push(v[1].replace(/([a-z0-9])([A-Z])/g, "$1_$2").toLowerCase());
  }
  return out;
}

test("every secret field the core knows has a label, and every scheme lists only those", () => {
  const fields = rustSecretFields();
  assert.ok(fields.length >= 8, `${fields}`);
  for (const f of fields) assert.ok(SECRET_FIELD_LABEL[f], `${f} has a label`);
  for (const [scheme, list] of Object.entries(SECRET_FIELDS_BY_SCHEME)) {
    for (const f of list) assert.ok(fields.includes(f), `${scheme} names a real field: ${f}`);
  }
});

/**
 * `AuthScheme::fields()` read out of the Rust: each scheme's arm and the
 * `SecretField` variants it lists, as the desktop spells them. A field moved
 * from one scheme to another there fails here.
 */
function rustSchemeFields() {
  return rustSchemeArms("fields");
}

/**
 * One of `AuthScheme`'s field lists (`fields`, `required`) read out of the
 * Rust, by scheme. A key scheme's arm is `Self::keys(extra_keys)` — the
 * first key's field, then each further key's — read here as the first alone.
 */
function rustSchemeArms(method) {
  const m = RUST.match(new RegExp(`pub fn ${method}\\(&self\\) -> Vec<SecretField> \\{([\\s\\S]*?)\\n {4}\\}`));
  assert.ok(m, `AuthScheme::${method} is declared in connector.rs`);
  const snake = (v) => v.replace(/([a-z0-9])([A-Z])/g, "$1_$2").toLowerCase();
  const out = {};
  for (const arm of m[1].matchAll(/AuthScheme::(\w+)(?: \{[^}]*\})? =>\s*\{?\s*(vec!\[([\s\S]*?)\]|Self::keys\(extra_keys\))/g)) {
    out[snake(arm[1])] = arm[2].startsWith("Self::keys") ? ["api_key"] : [...arm[3].matchAll(/SecretField::(\w+)/g)].map((f) => snake(f[1]));
  }
  assert.match(RUST, /fn keys\(extra_keys: &\[ExtraKey\]\) -> Vec<SecretField> \{\s*std::iter::once\(SecretField::ApiKey\)\s*\.chain\(extra_keys\.iter\(\)\.map\(\|k\| k\.secret\)\)/, "a key scheme holds its first key, then each further key's field");
  return out;
}

test("the scheme's fields mirror AuthScheme::fields — less the tokens nobody types: a refresh token, and the one a client-credentials grant answers", () => {
  const rust = rustSchemeFields();
  assert.deepEqual(Object.keys(rust).sort(), Object.keys(SECRET_FIELDS_BY_SCHEME).sort(), "the same schemes");
  const typed = (scheme, f) => f !== "refresh_token" && !(scheme === "client_credentials" && f === "access_token");
  for (const [scheme, fields] of Object.entries(rust)) {
    assert.deepEqual(SECRET_FIELDS_BY_SCHEME[scheme], fields.filter((f) => typed(scheme, f)), scheme);
  }
  assert.deepEqual(SECRET_FIELDS_BY_SCHEME.client_credentials, ["client_id", "client_secret"]);
  assert.deepEqual(secretFields("none"), []);
  assert.deepEqual(secretFields({ scheme: "bearer" }), ["token"]);
  assert.deepEqual(secretFields({ scheme: "jwt", alg: "ES256" }), ["private_key"]);
  assert.ok(isMultilineSecret("private_key"), "a PEM block spans lines");
  assert.ok(!isMultilineSecret("token"));
  assert.equal(schemeWord({ scheme: "oauth2", authorization_url: "x", token_url: "y" }), "oauth2");
  assert.equal(schemeWord(null), "none");
  assert.equal(needsAccount("none"), false);
  assert.equal(needsAccount("bearer"), true);
  assert.equal(connectApplies("oauth2"), true);
  assert.equal(connectApplies("bearer"), false);
});

test("the redirect URI is the loopback callback on the configured port", () => {
  assert.equal(redirectUri(4478), "http://127.0.0.1:4478/connectors/oauth/callback");
  assert.ok(connectWords(4478).includes("http://127.0.0.1:4478/connectors/oauth/callback"));
});

test("a connector's line counts its operations, its writes and its accounts", () => {
  const row = {
    id: "slack",
    name: "Slack",
    auth: "bearer",
    hosts: ["slack.com"],
    operations: [
      { id: "post_message", writes: true },
      { id: "list_channels", writes: false },
      { id: "auth_test", writes: false },
    ],
    accounts: 2,
    origin: { catalog: { slug: "slack" } },
  };
  assert.equal(connectorLine(row), "Bearer token · slack.com · 3 operations, 1 writes · 2 accounts here");
  assert.equal(connectorLine({ ...row, operations: [row.operations[1]], accounts: 0 }), "Bearer token · slack.com · 1 operation · no account here");
  assert.equal(originWords(row.origin), "from the catalog");
  assert.equal(originWords("local"), "yours");
  assert.equal(connectorLine(null), "");
});

test("account rows put the default first, then by label", () => {
  const rows = accountRows([
    { id: "b", label: "personal", default: false },
    { id: "a", label: "work", default: true },
    { id: "c", label: "acme", default: false },
  ]);
  assert.deepEqual(rows.map((r) => r.label), ["work", "acme", "personal"]);
  assert.deepEqual(accountRows(null), []);
});

test("the secrets line says which fields are set and where, never a value", () => {
  const file = { secrets_set: ["token"], token_source: "file" };
  assert.deepEqual(secretsLine(file, "bearer"), { tone: "ok", text: "token in a 0600 file under identity/" });
  const keyring = { secrets_set: ["username"], token_source: "keyring" };
  assert.deepEqual(secretsLine(keyring, "basic"), { tone: "warn", text: "username or email in the OS keyring — password or API token still wanted" });
  assert.deepEqual(secretsLine({ secrets_set: [], token_source: "file" }, "api_key"), { tone: "warn", text: "No secret set yet — API key wanted." });
  assert.deepEqual(secretsLine({ secrets_set: [], token_source: "file" }, "none"), { tone: "quiet", text: "No secret needed." });
  const client = { secrets_set: ["client_id", "client_secret"], token_source: "file" };
  assert.equal(secretsLine(client, "oauth2").tone, "warn");
  assert.ok(secretsLine(client, "oauth2").text.startsWith("client set; not connected yet"));
  const connected = { secrets_set: ["client_id", "access_token", "refresh_token"], token_source: "file" };
  assert.equal(secretsLine(connected, "oauth2").tone, "ok");
  assert.ok(secretsLine(connected, "oauth2").text.startsWith("connected"));
  assert.ok(!JSON.stringify(secretsLine(connected, "oauth2")).includes("xoxb"));
});

test("the OAuth line reads the expiry and the scope", () => {
  const now = 1_000_000;
  assert.equal(oauthLine({ oauth: null }, now), null);
  assert.equal(oauthLine({ secrets_set: ["access_token"], oauth: { expires_at: null, scope: "read", expired: false } }, now), "token without an expiry · scope read");
  assert.equal(oauthLine({ secrets_set: ["access_token"], oauth: { expires_at: now - 1, scope: null, expired: true } }, now), "token expired; refreshed on the next call");
  assert.equal(oauthLine({ secrets_set: ["access_token"], oauth: { expires_at: now + 1800, scope: null, expired: false } }, now), "token good for 30 min");
  assert.equal(oauthLine({ secrets_set: ["access_token"], oauth: { expires_at: now + 7200, scope: null, expired: false } }, now), "token good for 2 h");
  assert.equal(oauthLine({ secrets_set: ["access_token"], oauth: { expires_at: now + 3 * 86400, scope: null, expired: false } }, now), "token good for 3 d");
});

test("a check's answer reads in one line with a tone", () => {
  assert.deepEqual(checkLine(null), { tone: "quiet", text: "Not checked yet." });
  assert.deepEqual(checkLine({ state: "connected", status: 200 }, "Slack"), { tone: "ok", text: "Connected — Slack answered (200)." });
  assert.equal(checkLine({ state: "refused", status: 401, reason: "invalid_auth" }, "Slack").tone, "danger");
  assert.ok(checkLine({ state: "refused", status: 401, reason: "invalid_auth" }, "Slack").text.includes("invalid_auth"));
  assert.equal(checkLine({ state: "unreachable", reason: "timed out" }, "Slack").tone, "warn");
  assert.equal(checkLine({ state: "no_check" }, "Slack").tone, "quiet");
});

test("a definition parses to an object or says why not, and the skeleton validates in shape", () => {
  assert.equal(parseDefinition("[]").ok, false);
  assert.equal(parseDefinition("{").ok, false);
  const parsed = parseDefinition(JSON.stringify(emptyDefinition()));
  assert.equal(parsed.ok, true);
  assert.equal(parsed.value.check, "ping");
  assert.ok(emptyDefinition().hosts.includes("api.example.com"));
  assert.deepEqual(
    problemLines({ ok: false, problems: [{ field: "hosts", text: { id: "problem-connector-declare-least-one-host-connector-may-reach" } }, { text: { id: "problem-connector-connector-needs-name" } }] }),
    ["hosts: declare at least one host the connector may reach", "a connector needs a name"],
  );
  assert.deepEqual(problemLines(null), []);
});

test("the toasts name the account and never a secret", () => {
  assert.ok(addedWords("work").startsWith("work added"));
  assert.ok(forgottenWords("work").includes("every secret"));
  assert.equal(secretsSetWords([]), "Nothing changed.");
  assert.equal(secretsSetWords(["token"]), "token set — sent once, never displayed.");
});

test("a check state this desktop does not know is said, and Save's refusal counts its problems", () => {
  const line = checkLine({ state: "throttled", status: 429 });
  assert.equal(line.tone, "warn");
  assert.ok(line.text.includes("throttled"), line.text);
  assert.equal(saveRefusedWords(1), "Not saved — one problem, listed below.");
  assert.equal(saveRefusedWords(3), "Not saved — 3 problems, listed below.");
});

test("the definition the editor opens on is the record's own fields by name — never what the store stamps, nor anything a definition does not have", () => {
  const definition = emptyDefinition();
  const record = { ...definition, origin: "local", created_at: 1700000000 };
  assert.deepEqual(definitionOf(record), definition, "the record without its origin and its moment");
  assert.deepEqual(definitionOf({ ...record, accounts: 2, installed: true }), definition, "and without what a row adds to it");
  assert.deepEqual(Object.keys(definitionOf({ id: "x", name: "X", description: "", base_url: "https://x.example", hosts: ["x.example"], auth: { scheme: "none" }, operations: [], created_at: 1 })), ["id", "name", "description", "base_url", "hosts", "auth", "operations"], "a field the record leaves out is left out");
  // The keys are the node's own: `ConnectorDefinitionBody`, field by field.
  const dto = readFileSync(new URL("../../../../crates/bisa-node/src/dto.rs", import.meta.url), "utf8");
  const body = dto.slice(dto.indexOf("pub struct ConnectorDefinitionBody"));
  const fields = [...body.slice(0, body.indexOf("\n}")).matchAll(/^\s+pub ([a-z_]+): /gm)].map((m) => m[1]);
  assert.deepEqual(fields, [...DEFINITION_KEYS]);
  assert.deepEqual(Object.keys(emptyDefinition()).filter((k) => !DEFINITION_KEYS.includes(k)), [], "the skeleton a definition starts from names nothing else");
});

test("a connector is yours or the catalog's by the wire's own Origin — never by the words on its chip", () => {
  // The wire's shape, read from the generated types: `"local"`, or `{ catalog: { slug } }`.
  const types = readFileSync(new URL("../../types.gen.ts", import.meta.url), "utf8");
  const origin = types.match(/export type Origin =([\s\S]*?);\n\/\*\*/);
  assert.ok(origin, "Origin is declared in types.gen.ts");
  assert.ok(origin[1].includes('| "local"') && /\|\s*\{\s*catalog:\s*\{\s*slug: string;/.test(origin[1]), "local, or installed from the catalog under a slug");
  assert.equal(isYours("local"), true);
  assert.equal(isYours({ catalog: { slug: "slack" } }), false, "the catalog's connector is never offered Edit or Delete");
  assert.equal(isYours(null), true);
  assert.equal(isYours(undefined), true);
  assert.equal(originWords({ catalog: { slug: "slack" } }), "from the catalog");
  assert.equal(originWords("local"), "yours");
  // The panel asks the model, and compares no words.
  const panel = readFileSync(new URL("./ConnectorsPanel.tsx", import.meta.url), "utf8");
  assert.ok(panel.includes("const custom = isYours(row.origin);"));
  assert.ok(!/originWords\([^)]*\)\s*[!=]==/.test(panel), "no word is matched to decide what a row offers");
});

test("an account's body is built by name; a blank secret is not sent, and a parameter held as a number goes back as it was held", () => {
  const added = accountBody({ label: "  work ", values: { site: "acme", empty: "  " }, secrets: { username: "ada@acme.example", password: "" } }, null);
  assert.deepEqual(added, { id: null, label: "work", params: { site: "acme" }, secrets: { username: "ada@acme.example" } });
  const held = { id: "01ACC", connector: "obsidian", label: "vault", params: { port: 27124, secure: true, site: "acme", tags: ["a"] }, default: true, secrets_set: ["token"], token_source: "file" };
  const draft = accountDraft(held);
  assert.deepEqual(draft, { label: "vault", values: { port: "27124", secure: "true", site: "acme", tags: '["a"]' } }, "the form shows every parameter as text");
  // An edit that only set a secret: every parameter goes back as the account held it.
  assert.deepEqual(accountBody({ ...draft, secrets: { token: "t-1" } }, held), { id: "01ACC", label: "vault", params: { port: 27124, secure: true, site: "acme", tags: ["a"] }, secrets: { token: "t-1" } });
  // A parameter retyped is what was typed; one emptied is dropped; no secret typed, no `secrets` key.
  const retyped = accountBody({ label: "vault", values: { ...draft.values, port: "27125", secure: "" }, secrets: {} }, held);
  assert.deepEqual(retyped, { id: "01ACC", label: "vault", params: { port: "27125", site: "acme", tags: ["a"] } });
  assert.equal("secrets" in retyped, false);
  // The body carries nothing a draft happens to hold.
  assert.deepEqual(Object.keys(accountBody({ label: "x", values: {}, secrets: {}, touched: true, open: ["a"] }, null)).sort(), ["id", "label", "params"]);
  assert.deepEqual(typedSecrets({ a: "x", b: " ", c: undefined }), { a: "x" });
  assert.deepEqual(typedSecrets(null), {});
  assert.deepEqual(accountDraft(null), { label: "", values: {} });
});

test("the account form saves once it has a label and the connector's definition is read, and one write at a time", () => {
  assert.equal(maySaveAccount({ label: "work", busy: false, definitionRead: true }), true);
  assert.equal(maySaveAccount({ label: "  ", busy: false, definitionRead: true }), false);
  assert.equal(maySaveAccount({ label: "work", busy: true, definitionRead: true }), false);
  assert.equal(maySaveAccount({ label: "work", busy: false, definitionRead: false }), false, "the parameters the connector asks for are not on screen yet");
});

test("a check's answer on screen is the node's health, never a map of the panel's own", () => {
  // The node forgets an account's health wherever its way in changes — set
  // again, connected again, a parameter edited, forgotten — and the rows
  // read it back through the bus; the panel keeps no answer of its own.
  const panel = readFileSync(new URL("./ConnectorsPanel.tsx", import.meta.url), "utf8");
  assert.ok(!panel.includes("withoutCheck("), "no map of answers to keep in step");
  assert.ok(panel.includes("a.health"), "the row's health is the node's");
});

test("the callback port is the resolved setting, and unknown until it is read — a redirect URI is never said from a guess", () => {
  assert.equal(OAUTH_PORT_KEY, "connectors.oauth.port");
  const registry = readFileSync(new URL("../../../../crates/bisa-core/src/settings.rs", import.meta.url), "utf8");
  assert.ok(registry.includes(`"${OAUTH_PORT_KEY}",`), "the key is the registry's");
  assert.equal(oauthPort(null), null);
  assert.equal(oauthPort(undefined), null);
  assert.equal(oauthPort([]), null);
  assert.equal(oauthPort([{ key: OAUTH_PORT_KEY, value: 4478, origin: "default" }]), 4478);
  assert.equal(oauthPort([{ key: OAUTH_PORT_KEY, value: 5599, origin: "machine" }]), 5599);
  assert.equal(oauthPort([{ key: OAUTH_PORT_KEY, value: "5599", origin: "machine" }]), null, "a value that is no port is no port");
  const panel = readFileSync(new URL("./ConnectorsPanel.tsx", import.meta.url), "utf8");
  assert.ok(panel.includes("oauthPort(useResolvedSettings(null).resolved)"), "read through the settings store, which reads again on settings_changed and on reconnect");
  assert.ok(!/useState\(\s*4478\s*\)/.test(panel) && !panel.includes("4478"), "no default of the registry's is repeated in the panel");
});


test("an account row shows one verb — Connect until an OAuth account holds a token, Check after — and keeps the rest in its menu", () => {
  const fresh = { default: false, secrets_set: ["client_id"] };
  const connected = { default: true, secrets_set: ["client_id", "access_token"] };
  assert.deepEqual(accountVerbs(fresh, true), { main: "connect", more: ["secrets", "check", "default", "forget"] });
  assert.deepEqual(accountVerbs(connected, true), { main: "check", more: ["connect", "secrets", "forget"] }, "connected: Connect again is in the menu, and a default is not made default twice");
  assert.deepEqual(accountVerbs({ default: false, secrets_set: [] }, false), { main: "check", more: ["secrets", "default", "forget"] }, "no OAuth: no Connect at all");
  const panel = readFileSync(new URL("./ConnectorsPanel.tsx", import.meta.url), "utf8");
  assert.ok(panel.includes("accountVerbs(a, oauth, hasCheck)") && panel.includes("<MoreMenu"), "the row draws the model's verbs; the rest behind one menu");
  assert.ok(panel.includes("onSelect: () => setForgetting(a)"), "Forget still asks first");
});

// added by the coverage pass: connectorsModel.test.mjs
test("the words of an account made default, connected, set, and a consent page opening", () => {
  assert.equal(defaultWords("work"), "work is the default account.");
  assert.ok(connectedWords("work").startsWith("work connected"));
  assert.equal(secretsSetWords([]), "Nothing changed.");
  assert.ok(secretsSetWords(["token"]).includes("token"));
  assert.ok(openedWords().length > 0);
});

// added by the connectors quality programme

const ROW = (over = {}) => ({ id: "a1", label: "work", params: {}, default: true, secrets_set: ["client_id"], token_source: "file", oauth: null, health: { state: "unknown" }, ...over });
const START = { url: "https://auth.example/consent?state=s1", redirect_uri: "http://127.0.0.1:4478/connectors/oauth/callback", expires_at: 1_000_600 };

test("a connection started remembers the row as it stood, and the landing is a change to it — never a token the row held before", () => {
  const fresh = pendingFlow(START, ROW());
  assert.equal(fresh.account, "a1");
  assert.equal(fresh.code, "");
  assert.equal(fresh.url, START.url);
  assert.equal(fresh.redirectUri, START.redirect_uri);
  assert.equal(fresh.expiresAt, 1_000_600);
  assert.equal(fresh.browserFailed, false);
  assert.equal(flowLanded(fresh, ROW()), false, "no token yet");
  assert.equal(flowLanded(fresh, ROW({ secrets_set: ["client_id", "access_token"], oauth: { expires_at: 1_003_600, scope: null, expired: false } })), true, "a token now held");
  assert.equal(flowLanded(fresh, ROW({ id: "other", secrets_set: ["access_token"] })), false, "another account's row");
  assert.equal(flowLanded(fresh, null), false);
  // Connect again: the row already holds a token when the flow begins.
  const held = ROW({ secrets_set: ["client_id", "access_token"], oauth: { expires_at: 1_003_600, scope: "read", expired: false }, health: { state: "ok" } });
  const again = pendingFlow(START, held);
  assert.equal(flowLanded(again, held), false, "the same row is not a landing");
  assert.equal(flowLanded(again, ROW({ ...held, oauth: { expires_at: 1_007_200, scope: "read", expired: false } })), true, "tokens issued anew: the expiry moved");
  assert.equal(flowLanded(again, ROW({ ...held, oauth: { expires_at: 1_003_600, scope: "read write", expired: false } })), true, "a wider scope");
  assert.equal(flowLanded(again, ROW({ ...held, health: { state: "unknown" } })), true, "the reconnect forgot the health");
  assert.equal(flowLanded(again, ROW({ ...held, secrets_set: ["client_id"] })), false, "a token gone is no landing");
  const timeless = pendingFlow({ ...START, expires_at: null }, ROW());
  assert.equal(timeless.expiresAt, null);
});

test("the paste box knows how long a code is taken, says when the flow expired, and sends once: not while busy, not blank, not expired", () => {
  const flow = pendingFlow(START, ROW());
  assert.equal(flowExpired(flow, 1_000_000), false);
  assert.equal(flowExpired(flow, 1_000_600), true, "at the moment the node forgets it");
  assert.equal(flowExpired(null, 1_000_000), false);
  assert.equal(flowWords(flow, 1_000_000), "A pasted code is taken for 10 more min.");
  assert.equal(flowWords(flow, 1_000_570), "A pasted code is taken for 1 more min.", "never zero while open");
  assert.equal(flowWords(flow, 1_000_600), "This connection expired — start it again with Connect.");
  assert.equal(flowWords(pendingFlow({ ...START, expires_at: null }, ROW()), 1), "", "no lifetime said, no line");
  assert.equal(mayPaste({ flow: null, busy: false, now: 1 }), false);
  assert.equal(mayPaste({ flow, busy: false, now: 1_000_000 }), false, "nothing typed");
  const typed = { ...flow, code: " abc " };
  assert.equal(mayPaste({ flow: typed, busy: false, now: 1_000_000 }), true);
  assert.equal(mayPaste({ flow: typed, busy: true, now: 1_000_000 }), false, "one send at a time: Enter during one sends nothing");
  assert.equal(mayPaste({ flow: typed, busy: false, now: 1_000_601 }), false, "expired");
  assert.equal(mayPaste({ flow: { ...typed, code: "   " }, busy: false, now: 1_000_000 }), false);
  assert.ok(connectUriWords("http://127.0.0.1:5000/connectors/oauth/callback").includes("http://127.0.0.1:5000/connectors/oauth/callback"));
});

test("an edit says what moved — the label, a parameter, the secrets — and Nothing changed. only when nothing did", () => {
  const account = ROW({ label: "work", params: { site: "acme", port: 8443 } });
  assert.equal(savedWords({ id: "a1", label: "work", params: { site: "acme", port: 8443 } }, account), "Nothing changed.");
  assert.equal(savedWords({ id: "a1", label: "work", params: { port: 8443, site: "acme" } }, account), "Nothing changed.", "the order of the parameters is not a change");
  assert.equal(savedWords({ id: "a1", label: "work", params: { site: "acme", port: 8443 }, secrets: { token: "t" } }, account), "token set — sent once, never displayed.");
  assert.equal(savedWords({ id: "a1", label: "personal", params: { site: "acme", port: 8443 } }, account), "personal saved.");
  assert.equal(savedWords({ id: "a1", label: "work", params: { site: "acme-eu", port: 8443 } }, account), "work saved.");
  assert.equal(savedWords({ id: "a1", label: "work", params: { site: "acme" } }, account), "work saved.", "a parameter dropped");
  assert.equal(savedWords({ id: "a1", label: "work ", params: { site: "acme", port: 8443 }, secrets: { token: "t", password: "p" } }, ROW({ label: "work", params: { site: "acme", port: 8443 } })), "Nothing changed.".replace("Nothing changed.", "token, password set — sent once, never displayed."), "a trimmed label is the same label");
  assert.equal(savedWords({ id: "a1", label: "team", params: {}, secrets: { token: "t" } }, ROW({ label: "work" })), "team saved; token set — sent once, never displayed.");
});

test("Valid. goes with the first character typed since the node read the text; a renamed id is said before any round trip; Edit waits for the definition", () => {
  const valid = { problems: [] };
  assert.equal(validationFor(null, "{}"), null);
  assert.equal(validationFor({ text: "{}", validation: valid }, "{}"), valid);
  assert.equal(validationFor({ text: "{}", validation: valid }, "{ }"), null, "a character moved: the node has not read this text");
  assert.equal(renamedProblem(null, { id: "x" }), null, "a new definition names what it likes");
  assert.equal(renamedProblem("acme", { id: "acme" }), null);
  assert.equal(renamedProblem("acme", {}), null, "no id typed yet");
  assert.equal(renamedProblem("acme", { id: "acme-eu" }), "The id cannot change here: it stays acme. For acme-eu, add a connector under that id and delete this one.");
  assert.equal(mayEditDefinition({ busy: false, definitionRead: true }), true);
  assert.equal(mayEditDefinition({ busy: true, definitionRead: true }), false);
  assert.equal(mayEditDefinition({ busy: false, definitionRead: false }), false, "the read failed or is on its way: no dialog armed for a later reload");
});

test("every secret field has the noun its input names it by — a message of its own, never a label lowercased", () => {
  for (const field of rustSecretFields()) {
    assert.equal(typeof SECRET_FIELD_WHAT[field], "string", `${field} has a noun`);
    assert.ok(SECRET_FIELD_WHAT[field].length > 0, field);
  }
  assert.equal(SECRET_FIELD_WHAT.token, "token");
  assert.equal(SECRET_FIELD_WHAT.api_key, "API key", "a noun keeps its own case");
  assert.equal(emptyDefinition().operations[0].name, "Ping");
});

test("the lines nobody had pinned: a token under a minute, an expired flag with a future time, a refusal and a silence with no reason, a jwt account's secrets", () => {
  const now = 1_000_000;
  assert.equal(oauthLine({ secrets_set: ["access_token"], oauth: { expires_at: now + 30, scope: null, expired: false } }, now), "token good for 1 min", "never zero minutes");
  assert.equal(oauthLine({ secrets_set: ["access_token"], oauth: { expires_at: now + 3600, scope: null, expired: true } }, now), "token expired; refreshed on the next call", "the node's flag wins over the clock");
  const refused = checkLine({ state: "refused", status: 401, reason: null }, "Slack");
  assert.equal(refused.tone, "danger");
  assert.ok(refused.text.includes("Slack") && refused.text.includes("(401)") && !refused.text.includes("null"), refused.text);
  const silent = checkLine({ state: "unreachable", reason: null }, "Slack");
  assert.equal(silent.tone, "warn");
  assert.ok(silent.text.includes("Slack") && !silent.text.includes("null"), silent.text);
  const none = checkLine({ state: "refused", status: null, reason: "the account holds no token; connect it first" }, "Slack");
  assert.ok(none.text.includes("holds no token") && !none.text.includes("("), "no status, no parenthesis: " + none.text);
  const jwt = secretsLine({ secrets_set: ["private_key"], token_source: "keyring" }, "jwt");
  assert.equal(jwt.tone, "ok");
  assert.ok(jwt.text.includes("private key") && !jwt.text.includes("private_key"), jwt.text);
  const bare = secretsLine({ secrets_set: [], token_source: "file" }, "jwt");
  assert.equal(bare.tone, "warn");
  assert.ok(bare.text.includes("private key") && !bare.text.includes("private_key"), bare.text);
});

// --- the hardening pass: accounts, flows, search ---------------------------

import {
  REQUIRED_SECRETS_BY_SCHEME,
  connectorFacts,
  filterConnectors,
  flowsSettled,
  missingRequired,
  requiredSecrets,
  schemeLabel,
  startsOpen,
  strayParams,
} from "./connectorsModel.mjs";

test("the fields a scheme cannot work without mirror AuthScheme::required — an OAuth client is its id alone, a PKCE client has no secret to ask for", () => {
  const rust = rustSchemeArms("required");
  assert.deepEqual(Object.keys(rust).sort(), Object.keys(REQUIRED_SECRETS_BY_SCHEME).sort(), "the same schemes");
  for (const [scheme, fields] of Object.entries(rust)) assert.deepEqual(REQUIRED_SECRETS_BY_SCHEME[scheme], fields, scheme);
  assert.deepEqual(requiredSecrets("none"), []);
  assert.deepEqual(requiredSecrets({ scheme: "oauth2" }), ["client_id"]);
  // The line asks for what is required, in the words a person reads — never the wire's.
  assert.deepEqual(secretsLine({ secrets_set: [], token_source: "file" }, "oauth2"), { tone: "warn", text: "No secret set yet — OAuth client id wanted." });
  assert.deepEqual(secretsLine({ secrets_set: [], token_source: "file" }, "basic"), { tone: "warn", text: "No secret set yet — username or email, password or API token wanted." });
  assert.deepEqual(secretsLine({ secrets_set: ["username"], token_source: "keyring" }, "basic"), { tone: "warn", text: "username or email in the OS keyring — password or API token still wanted" });
  const pkce = secretsLine({ secrets_set: ["client_id", "access_token"], token_source: "file" }, "oauth2");
  assert.equal(pkce.tone, "ok");
  assert.equal(pkce.text, "connected · OAuth client id, access token in a 0600 file under identity/");
});

test("a scheme reads as its name, and a connector's facts are separate things said separately", () => {
  assert.equal(schemeLabel("bearer"), "Bearer token");
  assert.equal(schemeLabel({ scheme: "oauth2" }), "OAuth 2");
  assert.equal(schemeLabel("api_key"), "API key");
  assert.equal(schemeLabel("none"), "No sign-in");
  assert.equal(schemeLabel("signed_request"), "signed_request", "a scheme this desktop does not know is said as the node said it");
  const row = { auth: "oauth2", hosts: ["a.example", "b.example"], operations: [{ writes: false }, { writes: true }], accounts: 2 };
  assert.deepEqual(connectorFacts(row), ["OAuth 2", "a.example, b.example", "2 operations, 1 writes", "2 accounts here"]);
  assert.equal(connectorLine(row), connectorFacts(row).join(" · "));
  assert.deepEqual(connectorFacts(null), []);
});

test("an account that holds a parameter the definition no longer declares can be edited again: the form asks only what is declared, and says what it lets go of", () => {
  const declared = [{ name: "site", required: true }, { name: "port" }];
  const account = { id: "a1", label: "work", params: { site: "acme", port: 8443, team: "T1", old: { a: 1 } }, secrets_set: [] };
  assert.deepEqual(accountDraft(account, declared), { label: "work", values: { site: "acme", port: "8443" } });
  assert.deepEqual(strayParams(account, declared), [
    { name: "old", value: '{"a":1}' },
    { name: "team", value: "T1" },
  ]);
  assert.deepEqual(strayParams(account, null), [], "nothing is called a stray before the definition is read");
  assert.deepEqual(strayParams(null, declared), []);
  const body = accountBody({ label: "work", values: { site: "acme", port: "8443", team: "T1" }, secrets: {} }, account, declared);
  assert.deepEqual(body.params, { site: "acme", port: 8443 }, "what is not declared is not sent — the store would refuse the whole save for it");
  // Without the declared list the body is built as before: by what was typed.
  assert.deepEqual(accountBody({ label: "x", values: { team: "T1" }, secrets: {} }, null).params, { team: "T1" });
});

test("a required parameter left blank is named, and holds Save", () => {
  const params = [{ name: "site", label: "Site", required: true }, { name: "region", required: true }, { name: "note" }];
  assert.deepEqual(missingRequired({ site: " ", region: "eu" }, params), ["Site"]);
  assert.deepEqual(missingRequired({}, params), ["Site", "region"], "a parameter with no label is named by its name");
  assert.deepEqual(missingRequired({ site: "a", region: "b" }, params), []);
  assert.deepEqual(missingRequired({}, null), []);
  assert.equal(maySaveAccount({ label: "work", busy: false, definitionRead: true, missing: ["Site"] }), false);
  assert.equal(maySaveAccount({ label: "work", busy: false, definitionRead: true, missing: [] }), true);
  assert.equal(maySaveAccount({ label: "work", busy: false, definitionRead: true }), true);
});

test("Check is offered only where the connector names a check: without one the row's verb is its secrets", () => {
  const plain = { default: true, secrets_set: ["token"] };
  assert.deepEqual(accountVerbs(plain, false, false), { main: "secrets", more: ["forget"] });
  assert.deepEqual(accountVerbs({ default: false, secrets_set: ["client_id"] }, true, false), { main: "connect", more: ["secrets", "default", "forget"] });
  assert.deepEqual(accountVerbs({ default: false, secrets_set: ["client_id", "access_token"] }, true, false), { main: "secrets", more: ["connect", "default", "forget"] });
  assert.deepEqual(accountVerbs(plain, false, true), { main: "check", more: ["secrets", "forget"] }, "with a check, as before");
});

test("connections are pending per account: one landing, expiring or losing its account leaves the others open", () => {
  const start = (expires_at) => ({ url: "https://p.example/consent", redirect_uri: "http://127.0.0.1:4478/connectors/oauth/callback", expires_at });
  const a = { id: "a", label: "work", secrets_set: ["client_id"], oauth: null, health: { state: "unknown" } };
  const b = { id: "b", label: "personal", secrets_set: ["client_id"], oauth: null, health: { state: "unknown" } };
  const c = { id: "c", label: "old", secrets_set: ["client_id"], oauth: null, health: { state: "unknown" } };
  const flows = { a: pendingFlow(start(2000), a), b: pendingFlow(start(1500), b), c: pendingFlow(start(2000), c) };
  // Nothing moved: the same object back, so a caller's state does not churn.
  const still = flowsSettled(flows, [a, b, c], 1000);
  assert.equal(still.flows, flows);
  assert.deepEqual([still.landed, still.expired, still.gone], [[], [], []]);
  // a landed, b ran out of time, c's account was removed elsewhere.
  const landedA = { ...a, secrets_set: ["client_id", "access_token"] };
  const after = flowsSettled(flows, [landedA, b], 1600);
  assert.deepEqual(after.flows, {});
  assert.deepEqual(after.landed, ["work"]);
  assert.deepEqual(after.expired, ["personal"]);
  assert.deepEqual(after.gone, ["c"]);
  // One at a time: a's landing leaves b's box open.
  const one = flowsSettled({ a: flows.a, b: flows.b }, [landedA, b], 1000);
  assert.deepEqual(Object.keys(one.flows), ["b"]);
  assert.deepEqual(flowsSettled({}, [a], 1000).flows, {});
});

test("the list is searched by what a person knows a connector by, and a card opens itself only where there is an account to see", () => {
  const rows = [
    { id: "slack", name: "Slack", description: "Channels and messages.", tags: ["chat"], hosts: ["slack.com"], accounts: 1 },
    { id: "google-drive", name: "Google Drive", description: "Files in a drive.", tags: ["files"], hosts: ["www.googleapis.com"], accounts: 0 },
    { id: "jira", name: "Jira", description: "Issues.", tags: ["ops"], hosts: ["*.atlassian.net"], accounts: 0 },
  ];
  const ids = (q) => filterConnectors(rows, q).map((r) => r.id);
  assert.deepEqual(ids(""), ["slack", "google-drive", "jira"]);
  assert.deepEqual(ids("  "), ["slack", "google-drive", "jira"]);
  assert.deepEqual(ids("DRIVE"), ["google-drive"], "by name, whatever the case");
  assert.deepEqual(ids("atlassian"), ["jira"], "by host");
  assert.deepEqual(ids("chat"), ["slack"], "by tag");
  assert.deepEqual(ids("issues"), ["jira"], "by description");
  assert.deepEqual(ids("google files"), ["google-drive"], "every word must be found");
  assert.deepEqual(ids("nothing-like-it"), []);
  assert.equal(startsOpen(rows[0]), true);
  assert.equal(startsOpen(rows[1]), false);
});


import { catalogConnectorLine } from "./connectorsModel.mjs";

test("the Library says of a connector how it signs in and what it offers, before it is installed", () => {
  assert.equal(catalogConnectorLine({ auth: "oauth2", operations: 5, writes: 2 }), "OAuth 2 · 5 operations, 2 write");
  assert.equal(catalogConnectorLine({ auth: "bearer", operations: 1, writes: 0 }), "Bearer token · 1 operation");
  assert.equal(catalogConnectorLine({ auth: "none", operations: 3, writes: 1 }), "No sign-in · 3 operations, 1 writes");
  assert.equal(catalogConnectorLine(null), "", "a node that says nothing of it leaves the row as it was");
  assert.equal(catalogConnectorLine(undefined), "");
});

test("an account never connected says nothing of a token: the expiry line is a connected account's", () => {
  const facts = { expires_at: null, expired: false, scope: null };
  assert.equal(oauthLine({ secrets_set: ["client_id"], oauth: facts }, 1000), null);
  assert.equal(oauthLine({ secrets_set: ["client_id", "access_token"], oauth: facts }, 1000), "token without an expiry");
});

// --- the two auth abilities: a second key, a client-credentials login -------

import { secretLabel, secretWhat } from "./connectorsModel.mjs";

const TWO_KEYS = {
  scheme: "api_key",
  place: { in: "header", name: "DD-API-KEY" },
  extra_keys: [{ secret: "app_key", label: "Application key", place: { in: "header", name: "DD-APPLICATION-KEY" } }],
};

test("a key scheme with more keys asks for each: the account's form, what is still wanted, and the definition's own name for it", () => {
  assert.deepEqual(secretFields(TWO_KEYS), ["api_key", "app_key"]);
  assert.deepEqual(requiredSecrets(TWO_KEYS), ["api_key", "app_key"]);
  // The scheme's word alone — a list row's — knows only the first key.
  assert.deepEqual(secretFields("api_key"), ["api_key"]);
  assert.deepEqual(secretFields({ scheme: "api_key", place: { in: "query", name: "k" } }), ["api_key"], "a definition from before there were more");
  // The form labels a further key as its platform does; any other field keeps the desktop's own word.
  assert.equal(secretLabel(TWO_KEYS, "app_key"), "Application key");
  assert.equal(secretLabel(TWO_KEYS, "api_key"), SECRET_FIELD_LABEL.api_key);
  assert.equal(secretLabel("api_key", "app_key"), SECRET_FIELD_LABEL.app_key);
  assert.equal(secretWhat(TWO_KEYS, "app_key"), "Application key");
  assert.equal(secretWhat(TWO_KEYS, "api_key"), "API key");
  // One key of two is said as held, the other as still wanted — by its platform's name.
  assert.deepEqual(secretsLine({ secrets_set: ["api_key"], token_source: "file" }, TWO_KEYS), { tone: "warn", text: "API key in a 0600 file under identity/ — Application key still wanted" });
  assert.equal(secretsLine({ secrets_set: ["api_key", "app_key"], token_source: "file" }, TWO_KEYS).tone, "ok");
  assert.deepEqual(secretsLine({ secrets_set: [], token_source: "file" }, TWO_KEYS), { tone: "warn", text: "No secret set yet — API key, Application key wanted." });
});

test("a client-credentials account is its client: no Connect, no token to type, and the token the grant answers is nothing a person set", () => {
  const grant = { scheme: "client_credentials", token_url: "https://auth.example.com/token" };
  assert.equal(schemeLabel(grant), "Client credentials");
  assert.equal(needsAccount(grant), true);
  assert.equal(connectApplies(grant), false, "there is no consent page to open");
  assert.deepEqual(secretFields(grant), ["client_id", "client_secret"]);
  assert.deepEqual(requiredSecrets(grant), ["client_id", "client_secret"]);
  assert.deepEqual(secretsLine({ secrets_set: ["client_id"], token_source: "file" }, grant), { tone: "warn", text: "OAuth client id in a 0600 file under identity/ — OAuth client secret still wanted" });
  const whole = secretsLine({ secrets_set: ["access_token", "client_id", "client_secret"], token_source: "keyring" }, grant);
  assert.equal(whole.tone, "ok");
  assert.ok(!whole.text.includes("access token"), whole.text);
  // A row of such an account offers Check, never Connect.
  assert.equal(accountVerbs({ default: true, secrets_set: ["client_id", "client_secret"] }, connectApplies(grant)).main, "check");
});
