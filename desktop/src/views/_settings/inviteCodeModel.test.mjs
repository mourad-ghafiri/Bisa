import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { inviteLink, joinCodeFromUrls, parseInviteCode } from "./inviteCodeModel.mjs";

const NPROFILE = "nprofile1qqsabc123";

test("both forms parse to the same halves and turn into each other", () => {
  const link = parseInviteCode(` bisa://join/${NPROFILE}/AbCd01 `);
  assert.deepEqual(link, { kind: "link", nprofile: NPROFILE, secret: "abcd01" });
  const code = parseInviteCode(`${NPROFILE}:abcd01`);
  assert.deepEqual(code, { kind: "code", nprofile: NPROFILE, secret: "abcd01" });
  assert.equal(inviteLink(code), `bisa://join/${NPROFILE}/abcd01`);
  assert.equal(parseInviteCode(`BISA://JOIN/${NPROFILE}/ff/`).kind, "link", "the scheme is not case-sensitive");
});

test("what is not a code says why", () => {
  assert.deepEqual(parseInviteCode(""), { kind: "none", reason: "" });
  assert.match(parseInviteCode("bisa://join/onlyone").reason, /bisa:\/\/join/);
  assert.match(parseInviteCode("hello").reason, /<nprofile>:<secret>/);
  assert.match(parseInviteCode("npub1abc:ff").reason, /nprofile/);
  assert.match(parseInviteCode(`${NPROFILE}:zz`).reason, /hex/);
});

test("why a paste is no code is said in the catalog's words, each shape named", () => {
  assert.equal(parseInviteCode("bisa://join/nprofile1abc").reason, "A link is bisa://join/<nprofile>/<secret>.");
  assert.equal(parseInviteCode("just words").reason, "A code is <nprofile>:<secret> — or paste the whole bisa://join/… link.");
  assert.equal(parseInviteCode("").reason, "", "nothing pasted: nothing to say");
  const model = readFileSync(new URL("./inviteCodeModel.mjs", import.meta.url), "utf8");
  assert.ok(!/reason: "[^"]/.test(model), "no sentence of its own");
});

test("a deep link's URLs yield the join code, and nothing else does", () => {
  assert.equal(joinCodeFromUrls([`bisa://join/${NPROFILE}/ff`]), `bisa://join/${NPROFILE}/ff`);
  assert.equal(joinCodeFromUrls(["bisa://other/x", "https://example.com"]), null);
  assert.equal(joinCodeFromUrls(null), null);
});
