/**
 * What a principal is called, tested where the rule lives. No DOM.
 *
 * Run with `npm test` from `desktop/`.
 */
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { nameIn, principalNames } from "./namesModel.mjs";

const AGENTS = [{ pubkey: "a1", name: "Scout" }];

test("you are You until you name yourself; then your name is the one everybody sees", () => {
  // The owner's row carries no label until the person gives one: nothing was made up for it.
  const unnamed = principalNames("me", [{ pubkey: "me" }, { pubkey: "ada", label: "Ada" }], AGENTS);
  assert.equal(unnamed.get("me"), "You");
  assert.equal(unnamed.get("ada"), "Ada");
  assert.equal(unnamed.get("a1"), "Scout");
  assert.equal(principalNames("me", [{ pubkey: "me", label: null }], []).get("me"), "You", "null is no name");
  assert.equal(principalNames("me", [{ pubkey: "me", label: "   " }], []).get("me"), "You", "blank is no name");
  assert.equal(principalNames("me", [], []).get("me"), "You", "no member row read yet: still you");
  // Named: the label, trimmed — whatever its words. A person may call themselves anything.
  assert.equal(principalNames("me", [{ pubkey: "me", label: " Mourad " }], []).get("me"), "Mourad");
  assert.equal(principalNames("me", [{ pubkey: "me", label: "workspace owner" }], []).get("me"), "workspace owner", "a label is a name: no words of it are special");
});

test("a member without a label has no name here, and an unknown principal is the head of its key", () => {
  const names = principalNames("me", [{ pubkey: "me" }, { pubkey: "0123456789abcdef", label: "" }], AGENTS);
  assert.equal(names.has("0123456789abcdef"), false);
  assert.equal(nameIn(names, "0123456789abcdef"), "01234567…");
  assert.equal(nameIn(names, "a1"), "Scout");
  assert.equal(nameIn(names, "me"), "You");
  // Before the node answered there is no *me* to name.
  assert.equal(principalNames(null, null, null).size, 0);
  assert.equal(principalNames(undefined, undefined, AGENTS).size, 1);
});

test("the shell reads its names from the model and compares no label with words", () => {
  const shell = readFileSync(new URL("./useWorkspaceData.ts", import.meta.url), "utf8");
  assert.ok(shell.includes("principalNames(info?.pubkey, info?.members, agents)"));
  assert.ok(shell.includes("nameIn(names, pubkey)"));
  assert.ok(!shell.includes("workspace owner"), "the node makes up no label, so none is compared");
});
