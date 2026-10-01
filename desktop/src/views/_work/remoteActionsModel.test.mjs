/**
 * The Remotes section's words and rows. Run with `node --test desktop/src/views/_work/remoteActionsModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { deleteRemoteWords, fetchedWords, fetchWords, groupRemoteBranches, hostPage, noBranchesWords, remoteActions, remoteTitle } from "./remoteActionsModel.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const icons = readFileSync(join(here, "../../ui/icons.ts"), "utf8");

test("a remote row offers Fetch on hover, then edit, copy, open on the host and Delete in its menu — every glyph a real one", () => {
  const acts = remoteActions({ name: "origin", url: "git@github.com:acme/shop.git" });
  assert.deepEqual(acts.map((a) => a.id), ["fetch", "edit_url", "copy_url", "open_host", "delete"]);
  assert.equal(acts[0].label, "Fetch origin");
  assert.ok(acts[0].hover && !acts[0].consented);
  assert.equal(acts[3].label, "Open on github.com");
  assert.ok(acts[4].danger && acts[4].consented && acts[4].separatorBefore);
  for (const a of acts) assert.match(icons, new RegExp(`^  ${a.icon}: `, "m"), `${a.icon} is a glyph`);
  // A local path has no host to open on; the git@ form becomes the https page.
  assert.deepEqual(remoteActions({ name: "backup", url: "/Volumes/disk/shop.git" }).map((a) => a.id), ["fetch", "edit_url", "copy_url", "delete"]);
  assert.equal(hostPage("git@github.com:acme/shop.git"), "https://github.com/acme/shop");
  assert.equal(hostPage("https://gitlab.com/acme/shop.git"), "https://gitlab.com/acme/shop");
  assert.equal(hostPage("/Volumes/disk/shop.git"), null);
  assert.equal(hostPage(null), null);
  // While an operation runs, everything but a copy is off with the reason.
  const busy = remoteActions({ name: "origin", url: "https://github.com/acme/shop" }, { busy: true });
  assert.ok(busy.find((a) => a.id === "fetch").disabled);
  assert.match(busy.find((a) => a.id === "delete").reason, /running/);
  assert.ok(!busy.find((a) => a.id === "copy_url").disabled);
});

test("remote branches group under their remote, newest first, naming the local branch whose upstream is them", () => {
  const groups = groupRemoteBranches(
    [
      { name: "origin", url: "u" },
      { name: "fork", url: "v" },
    ],
    [
      { remote: "origin", name: "main", head: "a", subject: "s", timestamp: 10 },
      { remote: "origin", name: "feature/x", head: "b", subject: "t", timestamp: 30 },
      { remote: "fork", name: "main", head: "c", subject: "u", timestamp: 20 },
    ],
    [{ name: "main", upstream: "origin/main" }, { name: "wip", upstream: null }],
  );
  assert.deepEqual(
    groups.map((g) => [g.remote.name, g.branches.map((b) => [b.full, b.trackedBy])]),
    [
      ["origin", [["origin/feature/x", null], ["origin/main", "main"]]],
      ["fork", [["fork/main", null]]],
    ],
  );
  assert.deepEqual(groupRemoteBranches([{ name: "empty", url: "u" }], [], [])[0].branches, []);
});

test("a remote's tooltip says where it lives, how it is reached and the URL as written — the row itself says the name alone", () => {
  assert.equal(remoteTitle({ name: "origin", url: "git@github.com:acme/shop.git" }), "github.com · acme/shop · SSH · git@github.com:acme/shop.git");
  assert.equal(remoteTitle({ name: "origin", url: "https://gitlab.com/acme/shop.git" }), "gitlab.com · acme/shop · HTTPS · https://gitlab.com/acme/shop.git");
  assert.match(remoteTitle({ name: "local", url: "/tmp/repo.git" }), /Local · \/tmp\/repo\.git$/);
});

test("the words: fetch all, what came in, the delete confirmation that tells the truth about Safety, the empty group", () => {
  assert.equal(fetchWords(1), "Fetch");
  assert.equal(fetchWords(3), "Fetch all");
  assert.equal(fetchWords(3, { running: true }), "Fetching…");
  assert.equal(fetchedWords([{ name: "origin", ok: true }]), "Fetched origin.");
  assert.equal(fetchedWords([{ name: "origin", ok: true }, { name: "fork", ok: true }]), "Fetched 2 remotes.");
  assert.match(fetchedWords([{ name: "origin", ok: true }, { name: "fork", ok: false, error: "no route" }]), /1 of 2 — fork: no route/);
  const del = deleteRemoteWords({ name: "fork", url: "git@x:y/z" });
  assert.equal(del.title, "Delete the remote fork?");
  assert.match(del.body, /Nothing is pinned in Safety/);
  assert.equal(del.confirm, "Delete");
  assert.equal(del.url, "git@x:y/z");
  assert.ok(del.danger);
  assert.equal(del.done, "Deleted the remote fork.");
  assert.equal(noBranchesWords("origin"), "Nothing fetched yet — Fetch brings in what origin has.");
});
