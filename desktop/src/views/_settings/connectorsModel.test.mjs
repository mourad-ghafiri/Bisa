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
  const m = RUST.match(/pub fn fields\(&self\) -> &'static \[SecretField\] \{([\s\S]*?)\n {4}\}/);
  assert.ok(m, "AuthScheme::fields is declared in connector.rs");
  const snake = (v) => v.replace(/([a-z0-9])([A-Z])/g, "$1_$2").toLowerCase();
  const out = {};
  for (const arm of m[1].matchAll(/AuthScheme::(\w+)(?: \{ \.\. \})? => &\[([\s\S]*?)\]/g)) {
    out[snake(arm[1])] = [...arm[2].matchAll(/SecretField::(\w+)/g)].map((f) => snake(f[1]));
  }
  return out;
}

test("the scheme's fields mirror AuthScheme::fields — the refresh token alone left out, since nobody types one", () => {
  const rust = rustSchemeFields();
  assert.deepEqual(Object.keys(rust).sort(), Object.keys(SECRET_FIELDS_BY_SCHEME).sort(), "the same schemes");
  for (const [scheme, fields] of Object.entries(rust)) {
    assert.deepEqual(SECRET_FIELDS_BY_SCHEME[scheme], fields.filter((f) => f !== "refresh_token"), scheme);
  }
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
  assert.equal(connectorLine(row), "bearer · slack.com · 3 operations, 1 writes · 2 accounts here");
  assert.equal(connectorLine({ ...row, operations: [row.operations[1]], accounts: 0 }), "bearer · slack.com · 1 operation · no account here");
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
  assert.deepEqual(secretsLine(keyring, "basic"), { tone: "warn", text: "username in the OS keyring — password still wanted" });
  assert.deepEqual(secretsLine({ secrets_set: [], token_source: "file" }, "api_key"), { tone: "warn", text: "No secret set yet — api_key wanted." });
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
  assert.equal(oauthLine({ oauth: { expires_at: null, scope: "read", expired: false } }, now), "token without an expiry · scope read");
  assert.equal(oauthLine({ oauth: { expires_at: now - 1, scope: null, expired: true } }, now), "token expired; refreshed on the next call");
  assert.equal(oauthLine({ oauth: { expires_at: now + 1800, scope: null, expired: false } }, now), "token good for 30 min");
  assert.equal(oauthLine({ oauth: { expires_at: now + 7200, scope: null, expired: false } }, now), "token good for 2 h");
  assert.equal(oauthLine({ oauth: { expires_at: now + 3 * 86400, scope: null, expired: false } }, now), "token good for 3 d");
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
  assert.ok(panel.includes("accountVerbs(a, oauth)") && panel.includes("<MoreMenu"), "the row draws the model's verbs; the rest behind one menu");
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
  assert.equal(oauthLine({ oauth: { expires_at: now + 30, scope: null, expired: false } }, now), "token good for 1 min", "never zero minutes");
  assert.equal(oauthLine({ oauth: { expires_at: now + 3600, scope: null, expired: true } }, now), "token expired; refreshed on the next call", "the node's flag wins over the clock");
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
  assert.ok(jwt.text.includes("private_key"), jwt.text);
  const bare = secretsLine({ secrets_set: [], token_source: "file" }, "jwt");
  assert.equal(bare.tone, "warn");
  assert.ok(bare.text.includes("private_key"), bare.text);
});
