/**
 * The MCP editor's facts: a draft round-trips through the three transports,
 * the form refuses what the node would, and a masked value goes back as the
 * mask. Run with `node --test desktop/src/views/_settings/mcpFormModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { KINDS, MASK, RESERVED, blank, FIELD_OF_REFUSAL, fieldForRefusal, probeStillAbout, fromDef, isHeaderName, isHttpUrl, isMasked, kindWords, pairsToMap, secretCount, toTransport, transportLine, validate } from "./mcpFormModel.mjs";

const core = readFileSync(new URL("../../../../crates/bisa-core/src/mcp.rs", import.meta.url), "utf8");

function def(transport, over = {}) {
  return { id: "docs", name: transport.name, description: "d", tags: ["data"], transport, enabled: true, created_at: 1, health: { state: "unknown" }, ...over };
}

test("the mask and the reserved name are the core's, word for word, and the three kinds are the core's three", () => {
  assert.ok(core.includes(`pub const MASK: &str = "${MASK}";`), "MASK mirrors bisa_core::mcp::MASK");
  assert.ok(core.includes(`pub const RESERVED_MCP_NAME: &str = "${RESERVED}";`));
  assert.deepEqual([...KINDS], ["stdio", "http", "sse"]);
  for (const k of KINDS) assert.ok(core.includes(`McpServerConfig::${k[0].toUpperCase()}${k.slice(1)} {`) || core.includes(`${k[0].toUpperCase()}${k.slice(1)} {`), `${k} is a variant`);
  for (const k of KINDS) assert.ok(kindWords(k).label && kindWords(k).hint);
});

test("a draft round-trips through each transport, dropping what is empty", () => {
  const stdio = def({ transport: "stdio", name: "pg", command: "npx", args: ["-y", "@x/pg"], env: { PGPASSWORD: MASK }, cwd: "/srv" });
  const d = fromDef(stdio);
  assert.equal(d.kind, "stdio");
  assert.equal(d.args, "-y\n@x/pg");
  assert.deepEqual(d.env, [{ k: "PGPASSWORD", v: MASK }]);
  assert.equal(d.cwd, "/srv");
  assert.deepEqual(toTransport(d), { transport: "stdio", name: "pg", command: "npx", args: ["-y", "@x/pg"], env: { PGPASSWORD: MASK }, cwd: "/srv" }, "a masked value goes back as the mask");
  const bare = toTransport({ ...blank(), name: "x", command: "x" });
  assert.deepEqual(bare, { transport: "stdio", name: "x", command: "x" }, "nothing empty is sent");

  const http = def({ transport: "http", name: "docs", url: "https://x.test/mcp", headers: { Authorization: MASK } });
  const h = fromDef(http);
  assert.equal(h.kind, "http");
  assert.deepEqual(h.headers, [{ k: "Authorization", v: MASK }]);
  assert.deepEqual(toTransport(h), { transport: "http", name: "docs", url: "https://x.test/mcp", headers: { Authorization: MASK } });

  const sse = def({ transport: "sse", name: "old", url: "https://x.test/sse" });
  const s = fromDef(sse);
  assert.equal(s.kind, "sse");
  assert.deepEqual(toTransport(s), { transport: "sse", name: "old", url: "https://x.test/sse" }, "no headers, none sent");
  assert.deepEqual(pairsToMap([{ k: " A ", v: "1" }, { k: "", v: "x" }]), { A: "1" });
  assert.equal(isMasked(MASK), true);
  assert.equal(isMasked("••••"), false);
});

test("the form refuses what the node would, in the same words, and puts the node's refusal on its field", () => {
  const d = { ...blank(), id: "Docs!", name: RESERVED, command: "" };
  const e = validate(d, false);
  assert.match(e.id, /Lowercase/);
  assert.match(e.name, /reserved/);
  assert.equal(e.command, "Nothing to run.");
  assert.equal(validate({ ...blank(), id: "ok", name: "n", command: "x", cwd: "./rel" }, false).cwd, "The working directory must be an absolute path.");
  assert.match(validate({ ...blank(), id: "ok", name: "n", command: "x", env: [{ k: "BAD KEY", v: "1" }] }, false).env, /not a variable name/);
  const remote = { ...blank(), kind: "http", id: "ok", name: "n", url: "x.test/mcp" };
  assert.match(validate(remote, false).url, /http:\/\//);
  assert.equal(validate({ ...remote, url: "https://x.test/mcp" }, false).url, undefined);
  assert.match(validate({ ...remote, url: "https://x.test/mcp", headers: [{ k: "X Api", v: "1" }] }, false).headers, /not a header name/);
  assert.match(validate({ ...remote, url: "https://x.test/mcp", headers: [{ k: "X-Api", v: "a\r\nb" }] }, false).headers, /control character/);
  assert.deepEqual(validate({ ...remote, kind: "sse", url: "https://x.test/sse" }, true), {}, "editing needs no id");
  assert.equal(isHttpUrl("http://127.0.0.1:8000/mcp"), true);
  assert.equal(isHttpUrl("ws://x/y"), false);
  assert.equal(isHeaderName("X-Api-Key"), true);
  assert.equal(isHeaderName("X Api"), false);
});

test("a row says how many secrets it keeps and what the harness reaches — never a value", () => {
  assert.equal(secretCount({ transport: "stdio", name: "x", command: "x", env: { A: MASK, B: MASK } }), "2 variables");
  assert.equal(secretCount({ transport: "http", name: "x", url: "u", headers: { A: MASK } }), "1 header");
  assert.equal(secretCount({ transport: "sse", name: "x", url: "u" }), "");
  assert.equal(transportLine({ transport: "stdio", name: "x", command: "npx", args: ["-y", "s"], cwd: "/srv" }), "npx -y s  (in /srv)");
  assert.equal(transportLine({ transport: "http", name: "x", url: "https://x.test/mcp", headers: { Authorization: "Bearer real" } }), "https://x.test/mcp");
});

test("the node's refusal is put beside the field it is about by the refusal's id — never by its words, which are in the reader's language", () => {
  assert.equal(fieldForRefusal("error-core-mcp-reserved-name"), "name");
  assert.equal(fieldForRefusal("error-core-mcp-empty-name"), "name");
  assert.equal(fieldForRefusal("error-core-mcp-invalid-url"), "url");
  assert.equal(fieldForRefusal("error-core-mcp-bad-header-name"), "headers");
  assert.equal(fieldForRefusal("error-core-mcp-bad-header-value"), "headers");
  assert.equal(fieldForRefusal("error-core-mcp-relative-cwd"), "cwd");
  assert.equal(fieldForRefusal("error-core-mcp-empty-command"), "command");
  assert.equal(fieldForRefusal("error-store-invalid-mcp-server-already-exists"), "id");
  assert.equal(fieldForRefusal("error-node-something-else"), "form", "a refusal about nothing the form holds is the form's");
  assert.equal(fieldForRefusal(null), "form");
  assert.equal(fieldForRefusal(undefined), "form");
  // Every refusal the core has for an MCP server is placed, and every id placed is a message the node can say.
  const errors = readFileSync(new URL("../../../../locales/en/errors.ftl", import.meta.url), "utf8");
  const ids = [...errors.matchAll(/^(error-[a-z0-9_-]+) = /gm)].map((m) => m[1]);
  const mcp = ids.filter((id) => id.startsWith("error-core-mcp-"));
  assert.ok(mcp.length >= 7, `the core's MCP refusals are read: ${mcp.length}`);
  for (const id of mcp) assert.ok(id in FIELD_OF_REFUSAL, `${id} has a field`);
  for (const id of Object.keys(FIELD_OF_REFUSAL)) assert.ok(ids.includes(id), `${id} is a message of errors.ftl`);
  // The form's fields are the ones it can show an error beside.
  for (const field of Object.values(FIELD_OF_REFUSAL)) assert.ok(["id", "name", "command", "cwd", "url", "headers"].includes(field));
  // The model reads no sentence: nothing in it lowercases a message or looks inside one.
  const model = readFileSync(new URL("./mcpFormModel.mjs", import.meta.url), "utf8");
  assert.ok(!model.includes("toLowerCase()") && !/\.includes\("[a-z ]+"\)/.test(model), "no word of a refusal is matched");
});

test("the form refuses a stdio server with nothing to run, or a working directory that is not absolute, in the catalog's words", () => {
  const stdio = { ...blank(), id: "files", name: "files" };
  assert.equal(validate(stdio, false).command, "Nothing to run.");
  assert.equal(validate({ ...stdio, command: "npx", cwd: "srv" }, false).cwd, "The working directory must be an absolute path.");
  assert.equal(validate({ ...stdio, command: "npx", cwd: "/srv" }, false).cwd, undefined);
});

test("a test's answer is drawn only over the draft it dialled: one edited since is told nothing by it", () => {
  assert.equal(probeStillAbout(3, 3), true);
  assert.equal(probeStillAbout(3, 4), false, "the draft changed while the test was on its way");
  const panel = readFileSync(new URL("./McpPanel.tsx", import.meta.url), "utf8");
  const test_ = panel.slice(panel.indexOf("const test = async () => {"), panel.indexOf("const err = (key"));
  assert.ok(test_.includes("const dialled = version.current;") && test_.includes("if (probeStillAbout(dialled, version.current)) setReport(r.report);"));
  assert.ok(test_.includes("if (!probeStillAbout(dialled, version.current)) return;"), "nor is its refusal put beside a field of another draft");
  const set = panel.slice(panel.indexOf("const set = (p: Partial<McpDraft>) => {"), panel.indexOf("const save = async () => {"));
  assert.ok(set.includes("version.current += 1;") && set.includes("setReport(null);"), "every edit counts, and clears the report on screen");
  assert.ok(panel.includes("[fieldForRefusal(e.refusal)]: message"), "a refusal is placed by its id");
});

