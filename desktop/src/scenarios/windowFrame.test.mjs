/**
 * The window is the frame (DESIGN.md › Layout): the document never scrolls —
 * the top bar and the footer stay where they are — and every screen owns the
 * region that does. Two things used to break it. An absolutely positioned
 * line with no positioned ancestor (a screen-reader-only one is enough) was
 * placed against the page, past `main`'s clipping, and made the page taller
 * than the window: a wheel turn over anything that does not scroll then moved
 * the whole app, footer included. And a thread whose host was a block, not a
 * flex column, took its content's height: nothing in it scrolled, and focus
 * dragged the screen instead.
 * Source-text facts across the files. Run with
 * `node --test desktop/src/scenarios/windowFrame.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync, statSync } from "node:fs";

const SRC = new URL("../", import.meta.url);
const read = (rel) => readFileSync(new URL(rel, SRC), "utf8");

/** Every `.tsx` under a folder of `src/`, as paths from `src/`. */
function screens(dir) {
  const out = [];
  for (const name of readdirSync(new URL(`${dir}/`, SRC))) {
    const rel = `${dir}/${name}`;
    if (statSync(new URL(rel, SRC)).isDirectory()) out.push(...screens(rel));
    else if (name.endsWith(".tsx")) out.push(rel);
  }
  return out;
}

test("the document itself never scrolls: the page's root clips, whatever a screen lets escape", () => {
  const css = read("styles.css");
  const frame = /html,\s*body\s*\{([^}]*)\}/.exec(css);
  assert.ok(frame && /overflow:\s*hidden/.test(frame[1]), "html and body clip — the window is no scrollport");
  assert.ok(/html,\s*body,\s*#root\s*\{[^}]*height:\s*100%/.test(css), "and the frame is still the window's height");
});

test("the screen's region and the side pane anchor what is positioned inside them", () => {
  const app = read("App.tsx");
  assert.ok(app.includes('<main className="relative min-w-0 flex-1 overflow-hidden"'), "main is the containing block of its screen, so its clipping holds for a positioned descendant");
  assert.ok(read("shell/AuxPane.tsx").includes('className="relative min-h-0 flex-1 overflow-auto"'), "the side pane's slot likewise — a form portaled into it keeps its hidden lines inside");
});

test("a scroll box that holds a screen-reader-only line is that line's anchor", () => {
  assert.ok(read("views/_studio/Chat.tsx").includes('className="relative min-h-0 flex-1 overflow-y-auto pb-2"'), "the thread's live line stays in the thread");
  assert.ok(read("views/WorkflowDesigner.tsx").includes('className="relative min-h-0 flex-1 overflow-y-auto"'), "the designer's Properties form — its inputs' alert line");
});

test("a goal's Conversation and Workflow tabs fill their column by flex, so the thread scrolls itself", () => {
  const goal = read("views/GoalDetail.tsx");
  for (const pane of ["<GoalConversationPane", "<GoalWorkflowTab"]) {
    const at = goal.indexOf(pane);
    assert.ok(at > 0, `${pane} is drawn`);
    const host = goal.slice(goal.lastIndexOf("<div className=", at), at);
    assert.ok(host.startsWith('<div className="flex min-h-0 flex-1 flex-col">'), `${pane}'s host is a flex column: ${host.slice(0, 60)}`);
  }
});

test("bringing the your-move band into view moves no more than it must", () => {
  const band = read("views/_goal/YourMoveBand.tsx");
  assert.ok(band.includes('block: "nearest"') && !band.includes('block: "start"'), "nearest: an ancestor that need not move stays");
});

test("no screen sizes itself by the window: a screen fills the region it is given", () => {
  for (const file of [...screens("views"), ...screens("shell")]) {
    const src = read(file);
    for (const banned of ["h-screen", "min-h-screen", "100vh", "100dvh"]) {
      assert.ok(!new RegExp(`(^|[\\s"'\`:])${banned}([\\s"'\`]|$)`, "m").test(src), `${file}: ${banned}`);
    }
  }
});
