import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { REMOTE_PROTOCOLS, isRemoteUrl, parseRemote, protocolWords, remoteSummary } from "./remoteModel.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));

test("the three shapes of a GitHub remote are one repository, each with its protocol", () => {
  const want = { host: "github.com", owner: "acme", name: "shop" };
  assert.deepEqual(parseRemote("https://github.com/acme/shop.git"), { protocol: "https", ...want });
  assert.deepEqual(parseRemote("https://github.com/acme/shop"), { protocol: "https", ...want });
  assert.deepEqual(parseRemote("git@github.com:acme/shop.git"), { protocol: "scp", ...want });
  assert.deepEqual(parseRemote("ssh://git@github.com:22/acme/shop.git"), { protocol: "ssh", ...want });
  assert.deepEqual(parseRemote("  https://GitHub.com/acme/shop/  "), { protocol: "https", ...want }, "trimmed, host lowercased");
  assert.equal(remoteSummary("git@github.com:acme/shop.git"), "github.com · acme/shop");
  // An SSH alias is a host until ssh says what it stands for — the node resolves it.
  assert.deepEqual(parseRemote("git@github-work:acme/shop.git"), { protocol: "scp", host: "github-work", owner: "acme", name: "shop" });
  assert.equal(parseRemote("git://github.com/acme/shop.git").protocol, "other");
});

test("a local path reads as its last two components, with no host", () => {
  assert.deepEqual(parseRemote("/tmp/work/storefront-origin.git"), { protocol: "local", host: "", owner: "work", name: "storefront-origin" });
  assert.equal(remoteSummary("/tmp/work/storefront-origin.git"), "work/storefront-origin");
  assert.deepEqual(parseRemote("C:\\repos\\thing.git"), { protocol: "local", host: "", owner: "repos", name: "thing" }, "a drive letter is not an scp host");
  assert.deepEqual(parseRemote("thing.git"), { protocol: "local", host: "", owner: "", name: "thing" });
  assert.equal(parseRemote("file:///tmp/work/x.git").protocol, "local");
});

test("the protocol words say how a push reaches the host, and the vocabulary is the Rust crate's", () => {
  assert.deepEqual(protocolWords("https"), { label: "HTTPS", transport: "https" });
  assert.deepEqual(protocolWords("ssh"), { label: "SSH", transport: "ssh" });
  assert.deepEqual(protocolWords("scp"), { label: "SSH", transport: "ssh" }, "git@host:owner/name is SSH without a scheme");
  assert.deepEqual(protocolWords("local"), { label: "Local", transport: "local" });
  assert.equal(protocolWords(null).transport, "other");
  const src = readFileSync(join(HERE, "../../../../crates/bisa-codehost/src/lib.rs"), "utf8");
  const block = src.slice(src.indexOf("pub enum RemoteProtocol {"), src.indexOf("}", src.indexOf("pub enum RemoteProtocol {")));
  const variants = [...block.matchAll(/^\s{4}([A-Z][a-z]+),/gm)].map((m) => m[1].toLowerCase());
  assert.deepEqual([...REMOTE_PROTOCOLS], variants, "the mirror lists exactly the Rust variants, in order");
});

test("what has no repository shape is shown as it is, and what git would refuse is refused first", () => {
  assert.equal(parseRemote(""), null);
  assert.equal(parseRemote(null), null);
  assert.equal(parseRemote("https://github.com/"), null);
  assert.equal(remoteSummary("https://github.com/"), "https://github.com/");
  assert.equal(isRemoteUrl("git@github.com:acme/shop.git"), true);
  assert.equal(isRemoteUrl("  "), false);
  assert.equal(isRemoteUrl("--upload-pack=x"), false, "option-shaped");
  assert.equal(isRemoteUrl("a b"), false, "whitespace inside");
});

test("on a host the owner is the whole namespace, as the node reads it: a subgroup is one repository", () => {
  for (const url of ["https://gitlab.com/acme/platform/web.git", "git@gitlab.com:acme/platform/web.git", "ssh://git@gitlab.com:2222/acme/platform/web"]) {
    const p = parseRemote(url);
    assert.deepEqual([p.host, p.owner, p.name], ["gitlab.com", "acme/platform", "web"], url);
  }
  assert.equal(remoteSummary("https://gitlab.com/acme/platform/web.git"), "gitlab.com · acme/platform/web");
  // A credential in the authority is never part of what is shown.
  assert.equal(remoteSummary("https://someone:not-a-real-value@git.example.com/acme/web.git"), "git.example.com · acme/web");
});
