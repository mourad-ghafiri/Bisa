import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import {
  configRows,
  diffWrites,
  formatIdent,
  globalIdentityOf,
  identityPairProblem,
  isEmptyWrite,
  validateEdits,
  validateValue,
} from "./gitConfigModel.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const SCHEMA_RS = join(HERE, "../../../../crates/bisa-vcs/src/config_schema.rs");

/** The schema as the node serves it — built here from the Rust source, so the fixture cannot drift. */
function schemaFromRust() {
  const src = readFileSync(SCHEMA_RS, "utf8");
  const defs = [...src.matchAll(/ConfigKeyDef \{\s*key: "([^"]+)",\s*kind: ConfigKind::(Text|Bool|Choice\(&\[([^\]]*)\]\)),[\s\S]*?scopes: (BOTH|GLOBAL_ONLY),/g)];
  assert.ok(defs.length >= 8, "the Rust schema has its keys");
  return defs.map((m) => ({
    key: m[1],
    kind: m[2] === "Text" ? { type: "text" } : m[2] === "Bool" ? { type: "bool" } : { type: "choice", options: m[3].split(",").map((s) => s.trim().replace(/"/g, "")) },
    label: m[1],
    hint: "",
    scopes: m[4] === "BOTH" ? ["global", "local"] : ["global"],
  }));
}

const schema = schemaFromRust();
const entries = [
  { key: "user.name", local: null, global: "Grace Hopper", effective: "Grace Hopper" },
  { key: "user.email", local: "ada@example.invalid", global: "grace@example.invalid", effective: "ada@example.invalid" },
  { key: "pull.rebase", local: null, global: "true", effective: "true" },
  { key: "core.autocrlf", local: "input", global: null, effective: "input" },
];

test("the schema read from the Rust source has the identity first and one global-only key", () => {
  assert.deepEqual(schema.slice(0, 3).map((d) => d.key), ["user.name", "user.email", "user.useConfigOnly"]);
  assert.deepEqual(schema.find((d) => d.key === "init.defaultBranch").scopes, ["global"]);
  assert.deepEqual(schema.find((d) => d.key === "core.autocrlf").kind, { type: "choice", options: ["true", "false", "input"] });
});

test("rows for the local layer show the layer's own value, what it inherits, and which keys it may hold", () => {
  const rows = configRows(schema, entries, "local");
  assert.equal(rows.length, schema.length, "every schema key, in order");
  const name = rows.find((r) => r.key === "user.name");
  assert.deepEqual([name.value, name.inherited, name.effective, name.editable], [null, "Grace Hopper", "Grace Hopper", true]);
  const email = rows.find((r) => r.key === "user.email");
  assert.deepEqual([email.value, email.inherited, email.effective], ["ada@example.invalid", "grace@example.invalid", "ada@example.invalid"]);
  assert.equal(rows.find((r) => r.key === "init.defaultBranch").editable, false, "global only");
  assert.equal(rows.find((r) => r.key === "commit.gpgsign").value, null, "a key the view did not list is simply unset");
});

test("rows for the global layer show the global value and never an inherited one", () => {
  const rows = configRows(schema, entries, "global");
  const email = rows.find((r) => r.key === "user.email");
  assert.deepEqual([email.value, email.inherited], ["grace@example.invalid", null]);
  assert.equal(rows.find((r) => r.key === "init.defaultBranch").editable, true);
});

test("edits become a write: changed values set, emptied values unset, untouched and uneditable keys nothing", () => {
  const rows = configRows(schema, entries, "local");
  const write = diffWrites(rows, {
    "user.name": " Ada Lovelace ",
    "user.email": "ada@example.invalid",
    "core.autocrlf": "",
    "pull.rebase": "",
    "init.defaultBranch": "main",
  });
  assert.deepEqual(write, { set: { "user.name": "Ada Lovelace" }, unset: ["core.autocrlf"] });
  assert.equal(isEmptyWrite(diffWrites(rows, {})), true);
  assert.equal(isEmptyWrite(write), false);
  assert.deepEqual(diffWrites(rows, { "user.email": "" }), { set: {}, unset: ["user.email"] }, "emptying a set value is inherit");
});

test("validation follows the kind; empty is inherit and never a problem", () => {
  assert.equal(validateValue({ type: "bool" }, "pull.rebase", "true"), null);
  assert.match(validateValue({ type: "bool" }, "pull.rebase", "yes"), /true or false/);
  assert.equal(validateValue({ type: "choice", options: ["true", "false", "input"] }, "core.autocrlf", "input"), null);
  assert.match(validateValue({ type: "choice", options: ["true", "false", "input"] }, "core.autocrlf", "maybe"), /One of/);
  assert.equal(validateValue({ type: "text" }, "user.name", "Ada Lovelace"), null);
  assert.match(validateValue({ type: "text" }, "user.name", "--global"), /dash/);
  assert.match(validateValue({ type: "text" }, "user.name", "a\nb"), /One line/);
  assert.match(validateValue({ type: "text" }, "user.email", "no-at"), /@/);
  assert.match(validateValue({ type: "text" }, "user.email", "a b@c.d"), /spaces/);
  assert.equal(validateValue({ type: "text" }, "user.email", "ada@example.invalid"), null);
  assert.equal(validateValue({ type: "text" }, "user.email", ""), null);
  const rows = configRows(schema, entries, "local");
  assert.deepEqual(Object.keys(validateEdits(rows, { "user.email": "bad", "pull.rebase": "true", "core.autocrlf": "maybe" })), ["user.email", "core.autocrlf"]);
});

test("the identity a view resolves, the global one, and whether a repository inherits", () => {
  assert.deepEqual(globalIdentityOf(entries), { name: "Grace Hopper", email: "grace@example.invalid" });
  const inheriting = entries.map((e) => (e.key === "user.email" ? { ...e, local: null, effective: e.global } : e));
  assert.equal(formatIdent({ name: "Ada", email: "a@b.c" }), "Ada <a@b.c>");
});

test("git needs both keys of the pair: a write that leaves one unset everywhere is refused with the sentence, a whole pair or an inherited half is not", () => {
  const rows = (name, email, inheritedName = null, inheritedEmail = null) => [
    { key: "user.name", value: name, inherited: inheritedName, editable: true },
    { key: "user.email", value: email, inherited: inheritedEmail, editable: true },
  ];
  assert.match(identityPairProblem(rows(null, null), { set: { "user.name": "Ada" }, unset: [] }), /needs an email/);
  assert.match(identityPairProblem(rows(null, null), { set: { "user.email": "a@b.c" }, unset: [] }), /needs a name/);
  assert.equal(identityPairProblem(rows(null, null), { set: { "user.name": "Ada", "user.email": "a@b.c" }, unset: [] }), null, "both at once");
  assert.equal(identityPairProblem(rows(null, null, null, "g@b.c"), { set: { "user.name": "Ada" }, unset: [] }), null, "the email is inherited");
  assert.match(identityPairProblem(rows("Ada", "a@b.c"), { set: {}, unset: ["user.email"] }), /needs an email/, "unsetting one of a local pair");
  assert.equal(identityPairProblem(rows("Ada", "a@b.c", null, "g@b.c"), { set: {}, unset: ["user.email"] }), null, "unset falls back to the inherited one");
  assert.equal(identityPairProblem(rows(null, null), { set: { "core.autocrlf": "true" }, unset: [] }), null, "not about the pair");
});
