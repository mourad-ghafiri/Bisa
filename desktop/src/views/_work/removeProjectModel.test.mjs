import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync } from "node:fs";
import { REMOVE_ACTS, asksAgain, isAdopted, checkoutWords, forGoodWords, projectRoots, removeChoices, removedSaid, rootsGo, standsOn } from "./removeProjectModel.mjs";

const ids = (choices) => choices.map((c) => c.id);

test("the acts are offered mildest first, and an archived project is not offered archiving again", () => {
  assert.deepEqual(ids(removeChoices({ archived: false, adopted: false, trash: true })), REMOVE_ACTS);
  assert.deepEqual(ids(removeChoices({ archived: true, adopted: false, trash: true })), ["forget", "delete"]);
  const choices = removeChoices({ archived: false, adopted: false, trash: true });
  assert.deepEqual(choices.map((c) => c.tone), ["default", "default", "danger"], "one act destroys, and it is last");
  assert.ok(choices.every((c) => c.description.endsWith(".")), "each act says what it does, in a sentence");
});

test("how the folder goes is the setting's word: the Trash is named only when the folder goes there", () => {
  const trashed = removeChoices({ archived: false, adopted: false, trash: true, checkouts: 3 }).at(-1);
  assert.equal(trashed.label, "Move the folder to the Trash");
  assert.match(trashed.description, /3 checkouts, workstreams and notes.*to the Trash.*recoverable/);
  const gone = removeChoices({ archived: false, adopted: false, trash: false, checkouts: 1 }).at(-1);
  assert.equal(gone.label, "Delete the folder for good");
  assert.match(gone.description, /1 checkout, workstreams and notes.*There is no undo\./);
  assert.ok(!/trash/i.test(`${gone.label} ${gone.description}`), "no promise of a Trash that is switched off");
  assert.match(removeChoices({ archived: false, adopted: false, trash: true }).at(-1).description, /its checkouts, workstreams and notes/, "the count is said only when known");
  assert.equal(checkoutWords(0), "0 checkouts");
});

test("an adopted folder is never deleted from here: the act is shown held, with the reason, not left out", () => {
  const choices = removeChoices({ archived: false, adopted: true, trash: true });
  assert.deepEqual(ids(choices), REMOVE_ACTS);
  assert.match(choices.at(-1).disabled, /adopted, not created by Bisa/);
  assert.equal(choices[0].disabled, null);
  assert.match(choices[1].description, /can be adopted again/);
});

test("only a folder going for good asks once more", () => {
  assert.equal(asksAgain("delete", false), true);
  assert.equal(asksAgain("delete", true), false, "the Trash is its own undo");
  assert.equal(asksAgain("forget", false), false);
  assert.equal(asksAgain("archive", false), false);
  const words = forGoodWords({ path: "/Users/me/bisa/projects/web-app", checkouts: 2 });
  assert.equal(words.title, "Delete the folder from disk?");
  assert.equal(words.confirmLabel, "Delete the folder");
  assert.match(words.body, /`\/Users\/me\/bisa\/projects\/web-app`.*2 checkouts.*never pushed.*no undo/);
  assert.match(forGoodWords({}).body, /the project's folder.*every checkout/);
});

test("what is said afterwards is what happened", () => {
  assert.match(removedSaid("archive", true).text, /archived/);
  assert.match(removedSaid("forget", false).text, /files are still there/);
  assert.match(removedSaid("delete", true).text, /in the Trash/);
  assert.deepEqual(removedSaid("delete", false), { tone: "ok", text: "Project and folder deleted." });
  assert.deepEqual(removedSaid("delete", true, { removed_tree: true, path: "/p" }), { tone: "ok", text: "Project forgotten — its folder is in the Trash." });
  // Asked to go and still there: said as it is, never as a success.
  const stayed = removedSaid("delete", true, { removed_tree: false, kept: "Operation not permitted", path: "/ws/projects/shop" });
  assert.equal(stayed.tone, "warn");
  assert.ok(stayed.text.includes("still on disk") && stayed.text.includes("/ws/projects/shop") && stayed.text.includes("Operation not permitted"), stayed.text);
  assert.ok(!stayed.text.includes("Trash"), "nothing went to the Trash");
  assert.equal(removedSaid("delete", false, { removed_tree: false }).tone, "warn", "a node that says no more than that is still believed");
  assert.equal(removedSaid("archive", true, { removed_tree: false }).tone, "ok", "an archive removes nothing and says nothing of folders");
});

test("only a folder the workspace made is ever offered for deletion: anything else, a record nobody can read included, is adopted", () => {
  assert.equal(isAdopted({ root: { type: "managed" } }), false);
  assert.equal(isAdopted({ root: { type: "external", path: "/Users/someone/code/shop" } }), true);
  for (const unreadable of [{ root: { type: "invented_later" } }, { root: null }, {}, null, undefined]) assert.equal(isAdopted(unreadable), true, JSON.stringify(unreadable));
  const deleting = (project) => removeChoices({ archived: false, adopted: isAdopted(project), trash: true, checkouts: 0 }).find((c) => c.id === "delete");
  assert.equal(deleting({ root: { type: "managed" } }).disabled, null);
  assert.match(deleting({ root: { type: "external" } }).disabled, /adopted, not created by Bisa/, "a person's own folder is never the platform's to delete");
  assert.ok(deleting(undefined).disabled, "and a project nobody can read is not deleted on a guess");
});


test("the count of checkouts is the catalog's to number: one checkout, no checkouts, several", () => {
  assert.equal(checkoutWords(1), "1 checkout");
  assert.equal(checkoutWords(0), "0 checkouts");
  assert.equal(checkoutWords(12), "12 checkouts");
  assert.equal(forGoodWords({ path: "/p", checkouts: 1 }).body, "This permanently removes `/p` and everything inside it, including 1 checkout and any commits that were never pushed. Deleting to the Trash is off on this machine (Settings › Project IDE › Editor), so there is no undo.");
});

test("a project stands on its primary and every checkout of it, and nobody else's", () => {
  const ref = (id, project) => ({ workstream: { id, project } });
  const list = [ref("p1", "p1"), ref("w1", "p1"), ref("w2", "p2"), ref("p2", "p2"), ref("w3", "p1"), ref("w1", "p1")];
  assert.deepEqual(projectRoots(list, "p1"), ["p1", "w1", "w3"], "the primary first, none twice");
  assert.deepEqual(projectRoots([], "p9"), ["p9"], "a list that has not caught up still names the project's own root");
  assert.deepEqual(projectRoots(null, "p9"), ["p9"]);
});

test("forgetting or deleting a project takes its roots away; archiving leaves them standing", () => {
  assert.deepEqual(REMOVE_ACTS.filter(rootsGo), ["forget", "delete"]);
  assert.equal(rootsGo("archive"), false, "one move brings an archived project back, and its shells with it");
});

test("the person is moved only when they stand on what was removed: the project's root or any checkout of it", () => {
  const roots = ["p1", "w1"];
  assert.equal(standsOn({ scope: "workstream", id: "p1" }, roots), true);
  assert.equal(standsOn({ scope: "workstream", id: "w1" }, roots), true, "a checkout of the project, not only its primary");
  assert.equal(standsOn({ scope: "workstream", id: "w2" }, roots), false);
  assert.equal(standsOn({ scope: "goal", id: "p1" }, roots), false, "a goal's root that shares no id space");
  assert.equal(standsOn(null, roots), false);
});

test("both surfaces that remove a project go through the one door, which moves nobody before the node answers", () => {
  const read = (rel) => readFileSync(new URL(rel, import.meta.url), "utf8");
  const door = read("./removeProject.ts");
  const call = door.indexOf("await api.deleteProject(");
  assert.ok(call > 0 && door.indexOf("closeTerminalsRootedAt(roots)") > call, "the tabs close after the node's answer");
  assert.ok(door.includes("if (rootsGo(act))"), "and only when the roots go");
  for (const rel of ["../_workbench/ProjectRail.tsx", "./ProjectDetail.tsx"]) {
    const text = read(rel);
    assert.ok(text.includes("removeProject(pid, chosen, trash,"), `${rel} asks the door`);
    assert.ok(!text.includes("api.deleteProject("), `${rel} calls the node through the door alone`);
  }
  const rail = read("../_workbench/ProjectRail.tsx");
  assert.ok(!/\n\s*if \(current\?\.id === (pid|wid)\) navigate\(/.test(rail), "no navigation beside the call: it follows the answer");
});

test("the door names the next home before the node is asked, forgets a remembered root that stood on the project, and both surfaces leave for that home by replace", () => {
  const read = (rel) => readFileSync(new URL(rel, import.meta.url), "utf8");
  const door = read("./removeProject.ts");
  const home = door.indexOf("const home = homeAfterLeaving(pid, lastRoot(), projects, workstreams);");
  const asked = door.indexOf("await api.deleteProject(");
  assert.ok(home > 0 && home < asked, "the home is computed from the facts in hand, before the lists move");
  assert.ok(door.includes("standsOn({ scope: \"workstream\", id: remembered }, roots)) forgetLastRoot();"), "a home is never a root that was put away or removed");
  assert.ok(door.indexOf("forgetLastRoot()") > asked, "and only once the node has answered");
  assert.ok(door.indexOf("forgetLastRoot()") < door.indexOf("if (rootsGo(act))"), "for every act, an archive included");
  assert.ok(door.includes("roots, home };"), "the home is handed back with the roots");
  // The rail: moved only when standing on what went, to the home, never to a bare `#/projects` that would pick the same root again.
  const rail = read("../_workbench/ProjectRail.tsx");
  const removal = rail.slice(rail.indexOf("<RemoveProjectDialog"), rail.indexOf("<ConfirmDialog", rail.indexOf("<RemoveProjectDialog")));
  assert.ok(removal.includes("if (standsOn(current, roots)) replace(homeRoute(home));"), "the rail leaves by replace");
  assert.ok(!removal.includes("navigate("), "Back never returns to a root that is gone");
  // About: archive and delete alike hand the home up — the person always stands on the project About is about.
  const detail = read("./ProjectDetail.tsx");
  const remove = detail.slice(detail.indexOf("const remove = ("), detail.indexOf("return (", detail.indexOf("const remove = (")));
  assert.ok(remove.includes("onLeft?.(home);"), "About leaves");
  assert.ok(!remove.includes('chosen === "archive"') && !remove.includes("navigate("), "no act stays on a project that was put away");
  const workbench = read("../Workbench.tsx");
  assert.ok(workbench.includes("onLeft={(home) => replace(homeRoute(home))}"), "the workbench moves to the home About handed up");
  assert.ok(!workbench.includes('navigate({ name: "projects" })'), "nothing in the workbench falls back to a bare `#/projects` any more");
});
