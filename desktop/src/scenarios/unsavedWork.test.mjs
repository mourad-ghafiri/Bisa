/**
 * Scenario: unsaved work is never lost silently (ide/03 §Tabs).
 *
 * ⌘N, a few words, then another file opened on top — the untitled document
 * is off screen and its editor unmounted. Its buffer is the tab's, so it is
 * still unsaved, still holds what was typed, and closing it asks. Closing it
 * without saving is what forgets the buffer; a tab that only moved, or a
 * rename on disk, forgets nothing. A scenario steps the models the way the
 * components do — no DOM.
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import { guardWords } from "../views/_workbench/closeGuardModel.mjs";
import { changedOnDisk, edited, emptyBuffer, isUnsaved, loaded, remounted, unsavedKeys, untitledBuffer } from "../views/_workbench/editorModel.mjs";
import { closeTab, closedTabs, emptyWorkbench, openTab, replaceTab, retargetTabs, rootKey, tabId, tabLabel, tabsFor } from "../views/_workbench/workbenchModel.mjs";

const ROOT = rootKey("workstream", "01JSCENARIOWORKSTREAM000000");
const keyOf = (tab) => `${ROOT}|${tabId(tab)}`;
const disk = (text) => ({ text, hash: `h(${text})`, editable: true, truncated: false, binary: false, size: text.length });

test("an untitled document that left the screen still asks before it closes, and keeps what was typed", () => {
  const untitled = { kind: "untitled", seq: 1 };
  const other = { kind: "file", path: "src/main.ts" };

  // ⌘N, a thought typed into it.
  let tabs = openTab(emptyWorkbench(), ROOT, untitled);
  let buffers = new Map([[keyOf(untitled), edited(untitledBuffer(untitled), "a thought worth keeping", 1000)]]);

  // Another file opens on top: the untitled editor unmounts. Nothing about the buffer changes.
  tabs = openTab(tabs, ROOT, other);
  buffers.set(keyOf(other), loaded(emptyBuffer(other.path), disk("export {};\n")));
  assert.deepEqual(unsavedKeys(buffers), [keyOf(untitled)], "off screen, and still unsaved");
  assert.equal(buffers.get(keyOf(untitled)).text, "a thought worth keeping");

  // Its ✕: the guard names it, and says saving asks for a name.
  const closing = tabsFor(tabs, ROOT).filter((t) => isUnsaved(buffers.get(keyOf(t))));
  const words = guardWords(closing.map((t) => ({ label: tabLabel(t), untitled: t.kind === "untitled" })));
  assert.equal(words.title, `Save changes to ${tabLabel(untitled)}?`);
  assert.match(words.note, /no name yet — saving asks for one/);

  // *Don't save*: the tab closes, and that — nothing else — is what forgets the buffer.
  const after = closeTab(tabs, ROOT, tabId(untitled));
  assert.deepEqual(closedTabs(tabs, after), [[ROOT, tabId(untitled)]]);
  for (const [root, id] of closedTabs(tabs, after)) buffers.delete(`${root}|${id}`);
  assert.deepEqual(unsavedKeys(buffers), []);
  assert.ok(buffers.has(keyOf(other)), "the file beside it is untouched");
});

test("a file edited, left, and come back to is as it was typed — and says so if the disk moved meanwhile", () => {
  const file = { kind: "file", path: "README.md" };
  const typed = edited(loaded(emptyBuffer(file.path), disk("# Title\n")), "# Title\n\nA line.\n", 1000);
  // Back on screen: the buffer the tab kept, then the read.
  const back = remounted(typed, file.path);
  assert.equal(back.text, "# Title\n\nA line.\n");
  assert.equal(changedOnDisk(back, disk("# Title\n")).status, "dirty", "the disk did not move: just unsaved");
  assert.equal(changedOnDisk(back, disk("# Title, by an agent\n")).status, "conflict", "it did: the three-way affordance, the typed text kept");
});

test("saving an untitled document under a name, and a rename on disk, lose nothing", () => {
  const untitled = { kind: "untitled", seq: 1 };
  const tabs = openTab(emptyWorkbench(), ROOT, untitled);
  // Saved as a file: the untitled tab is replaced, so its buffer goes — the file's is read from the disk it was just written to.
  const named = replaceTab(tabs, ROOT, tabId(untitled), { kind: "file", path: "notes/idea.md" });
  assert.deepEqual(closedTabs(tabs, named), [[ROOT, tabId(untitled)]]);

  // The folder is renamed on disk: the tab follows, and the move says which buffer follows it.
  const { state: renamed, moved } = retargetTabs(named, ROOT, "notes", "docs");
  assert.deepEqual(moved, [["notes/idea.md", "docs/idea.md"]]);
  assert.deepEqual(tabsFor(renamed, ROOT).map(tabId), ["file:docs/idea.md"]);

  // A tab that only moved pane, or a root whose tabs were just restored, closes nothing.
  assert.deepEqual(closedTabs(named, named), []);
  assert.deepEqual(closedTabs(emptyWorkbench(), named), []);
});
