/**
 * What a right-click on a tab offers. Run with `node --test desktop/src/views/_workbench/tabMenuModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { browserTabMenu, docTabMenu, terminalTabMenu } from "./tabMenuModel.mjs";

const doc = (over = {}) => docTabMenu({ pinned: false, preview: false, others: 2, right: 1, saved: 2, canSplit: true, inRoot: true, onDisk: true, desktop: true, reveal: "Reveal in Finder", ...over });
const ids = (items) => items.map((i) => i.id);

test("a file tab offers close verbs, pin and split, then its paths and reveals — the OS one only under the shell", () => {
  assert.deepEqual(ids(doc()), ["close", "close-others", "close-right", "close-saved", "close-all", "pin", "split-right", "split-down", "copy-path", "copy-absolute", "reveal-files", "reveal-os"]);
  assert.deepEqual(ids(doc({ desktop: false })).slice(-3), ["copy-path", "copy-absolute", "reveal-files"], "no file manager in a browser");
  assert.deepEqual(ids(doc({ inRoot: false, onDisk: false })), ["close", "close-others", "close-right", "close-saved", "close-all", "pin", "split-right", "split-down"], "Git, Diff and an untitled document have no path");
  // A loose file is on this machine and under no root: the absolute path and the file manager, nothing of Files.
  assert.deepEqual(ids(doc({ inRoot: false, onDisk: true })).slice(-2), ["copy-absolute", "reveal-os"]);
  assert.ok(doc({ inRoot: false, onDisk: true }).find((i) => i.id === "copy-absolute").separatorBefore, "the path group still starts with a rule");
  assert.deepEqual(ids(doc({ preview: true })).slice(4, 7), ["close-all", "keep", "pin"], "a preview offers Keep open, ahead of the pin");
  assert.ok(doc({ preview: true }).find((i) => i.id === "keep").separatorBefore && !doc({ preview: true }).find((i) => i.id === "pin").separatorBefore, "one rule before the keep-or-pin group");
  assert.equal(doc({ preview: true }).find((i) => i.id === "keep").command, "keep_tab");
  assert.ok(doc({ saved: 0 }).find((i) => i.id === "close-saved").disabled, "everything unsaved or pinned: nothing to close");
  assert.equal(doc().find((i) => i.id === "close-saved").command, "close_saved");
  assert.equal(doc().find((i) => i.id === "reveal-os").label, "Reveal in Finder");
  assert.equal(doc().find((i) => i.id === "close").command, "close_tab", "the menu names the keymap command, so it can show the chord");
  assert.equal(doc().find((i) => i.id === "reveal-files").command, "reveal_in_files");
  assert.ok(doc().find((i) => i.id === "pin").separatorBefore);
  assert.ok(doc().find((i) => i.id === "copy-path").separatorBefore);
});

test("a pinned tab cannot close from the menu and says why; nothing to the right disables that verb", () => {
  const pinned = doc({ pinned: true });
  assert.ok(pinned.find((i) => i.id === "close").disabled);
  assert.match(pinned.find((i) => i.id === "close").label, /unpin first/);
  assert.equal(pinned.find((i) => i.id === "pin").label, "Unpin");
  assert.equal(doc().find((i) => i.id === "pin").label, "Pin");
  assert.ok(doc({ right: 0 }).find((i) => i.id === "close-right").disabled);
  assert.ok(doc({ others: 0 }).find((i) => i.id === "close-others").disabled);
  assert.ok(doc({ others: 0, pinned: true }).find((i) => i.id === "close-all").disabled, "one pinned tab alone: close all would do nothing");
  assert.ok(!doc({ others: 0 }).find((i) => i.id === "close-all").disabled, "one unpinned tab: close all closes it");
  assert.ok(doc({ canSplit: false }).find((i) => i.id === "split-right").disabled);
});

test("a terminal tab: focus, new shell, restart only when exited, send to the agent, the Agent panel for a harness, then the closes", () => {
  const live = terminalTabMenu({ live: true, harness: false, others: 2, exited: 0 });
  assert.deepEqual(ids(live), ["focus", "new-shell", "send-to-agent", "close", "close-others", "close-exited"]);
  assert.ok(live.find((i) => i.id === "close").danger, "closing a live shell ends it: red");
  assert.match(live.find((i) => i.id === "close").label, /ends the shell/);
  assert.equal(live.find((i) => i.id === "close-others").label, "Close the other 2 tabs");
  assert.ok(live.find((i) => i.id === "close-exited").disabled);
  const harness = terminalTabMenu({ live: true, harness: true, others: 0, exited: 0 });
  assert.equal(harness.find((i) => i.id === "close").label, "Terminate", "a live harness is terminated: its session and its tab go together");
  assert.ok(harness.find((i) => i.id === "close").danger);
  const exited = terminalTabMenu({ live: false, harness: true, others: 1, exited: 1 });
  assert.equal(exited.find((i) => i.id === "close").label, "Close", "an exited harness is just closed");
  assert.deepEqual(ids(exited), ["focus", "new-shell", "restart", "send-to-agent", "show-agents", "close", "close-others", "close-exited"]);
  assert.ok(!exited.find((i) => i.id === "close").danger);
  assert.equal(exited.find((i) => i.id === "close-others").label, "Close the other tab");
  assert.equal(exited.find((i) => i.id === "close-exited").label, "Close 1 exited");
  assert.equal(exited.find((i) => i.id === "show-agents").command, "panel_agents");
  const alone = terminalTabMenu({ live: true, harness: false, others: 0, exited: 0 });
  assert.ok(alone.find((i) => i.id === "close-others").disabled);
  assert.equal(alone.find((i) => i.id === "close-others").label, "Close others");
});

test("a browser tab: reload, its URL and the machine's browser — off while blank — then the closes", () => {
  const loaded = browserTabMenu({ others: 2, blank: false, annotatable: false });
  assert.deepEqual(ids(loaded), ["reload", "copy-url", "open-outside", "close", "close-others"]);
  const served = browserTabMenu({ others: 2, blank: false, annotatable: true });
  assert.deepEqual(ids(served), ["reload", "copy-url", "open-outside", "annotate", "close", "close-others"], "a page an agent can edit offers its annotation");
  assert.equal(served.find((i) => i.id === "annotate").label, "Annotate the page for an agent…");
  assert.ok(served.find((i) => i.id === "annotate").separatorBefore && served.find((i) => i.id === "close").separatorBefore, "the annotation stands alone between the page's verbs and the closes");
  assert.ok(!ids(browserTabMenu({ others: 0, blank: true, annotatable: true })).includes("annotate"), "nothing loaded: nothing to annotate");
  assert.ok(!loaded.find((i) => i.id === "reload").disabled);
  assert.equal(loaded.find((i) => i.id === "close-others").label, "Close the other 2 browser tabs");
  assert.equal(loaded.find((i) => i.id === "close").command, "close_tab");
  const blank = browserTabMenu({ others: 0, blank: true, annotatable: false });
  assert.ok(blank.find((i) => i.id === "reload").disabled && blank.find((i) => i.id === "copy-url").disabled && blank.find((i) => i.id === "open-outside").disabled, "nothing loaded: nothing to reload, copy or open");
  assert.ok(blank.find((i) => i.id === "close-others").disabled);
  assert.equal(browserTabMenu({ others: 1, blank: false, annotatable: false }).find((i) => i.id === "close-others").label, "Close the other browser tab");
});
