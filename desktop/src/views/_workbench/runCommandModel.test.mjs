/**
 * The run command's words: ⌘⇧R's door, the Terminal caret's item, what an
 * unapproved command is held with. Run with
 * `node --test desktop/src/views/_workbench/runCommandModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { APPROVAL_PLACE, approvalWords, runCommandItem, runDoor } from "./runCommandModel.mjs";

const server = (id, folder, port) => ({ id, owner: { kind: "workstream", workstream: "w1", folder }, port, url: `http://127.0.0.1:${port}/`, page: `http://127.0.0.1:${port}/` });
const run = { command: "npm run dev", trusted: true };

test("⌘⇧R's door opens the newest server, else runs the approved command, else opens the folder picker — an unapproved command is no door", () => {
  assert.equal(runDoor({ run: null, servers: [] }).id, "serve-folder");
  assert.match(runDoor({ run: null, servers: [] }).hint, /the root or a folder of the checkout/);
  assert.equal(runDoor({ run, servers: [] }).id, "run");
  assert.match(runDoor({ run, servers: [] }).hint, /npm run dev/);
  assert.equal(runDoor({ run: { ...run, trusted: false }, servers: [] }).id, "serve-folder", "not approved here: the chord offers a folder to serve, it never opens a settings card");
  const door = runDoor({ run, servers: [server("s1", "", 4173), server("s2", "site", 4174)] });
  assert.equal(door.id, "open:s2", "a server up is opened, never started twice");
  assert.match(door.hint, /site\/ · :4174/);
});

test("the Terminal caret's run item: none without a command, Run `…` when approved with the chord when it is the door, held to its approval when not", () => {
  assert.equal(runCommandItem(null, { door: "serve-folder", busy: false }), null);
  assert.equal(runCommandItem({ command: "", trusted: true }, { door: "run", busy: false }), null, "a blank command is none");
  const item = runCommandItem(run, { door: "run", busy: false });
  assert.deepEqual(item, { id: "run", label: "Run `npm run dev`", hint: "In a terminal here, through the login shell", icon: "play", disabled: false, separatorBefore: true, command: "run_project" });
  assert.equal(runCommandItem(run, { door: "open:s1", busy: false }).command, undefined, "a server up: the chord is the Browser's Open");
  assert.equal(runCommandItem(run, { door: "run", busy: true }).disabled, true, "no checkout on disk yet: held");
  const held = runCommandItem({ ...run, trusted: false }, { door: "serve-folder", busy: true });
  assert.equal(held.id, "approve-run");
  assert.equal(held.label, "Run `npm run dev` — not approved here", "a menu row has no hint: the label says it");
  assert.equal(held.disabled, false, "opening the card that approves it is never held");
  assert.equal(held.command, undefined, "an unapproved command wears no chord");
  assert.match(held.hint, new RegExp(APPROVAL_PLACE));
});

test("the unapproved words name the command and the card that approves it", () => {
  assert.equal(approvalWords("npm run dev"), "`npm run dev` is not approved on this machine — review and approve it under About › Settings › Workstream scripts.");
  assert.equal(APPROVAL_PLACE, "About › Settings › Workstream scripts");
});
