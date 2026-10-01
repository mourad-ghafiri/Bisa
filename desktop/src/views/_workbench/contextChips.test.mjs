import test from "node:test";
import assert from "node:assert/strict";
import { annotationChip, attach, chipLabel, contextBytes, fileChip, filePage, fitsBudget, frameChips, hunkChip, MAX_CONTEXT_BYTES, pageWords, sameChip, selectionChip, terminalChip, urlPage } from "./contextChips.mjs";

test("chips are labelled by what a person would call them", () => {
  assert.equal(chipLabel(fileChip("src/main.rs")), "main.rs");
  assert.equal(chipLabel(selectionChip("src/main.rs", 3, 4, "a\nb")), "main.rs:3–4");
  assert.equal(chipLabel(selectionChip("src/main.rs", 3, 3, "a")), "main.rs:3");
  assert.equal(chipLabel(hunkChip("a.txt", true, "@@", "h1")), "a.txt · staged hunk");
  assert.equal(chipLabel(terminalChip("t3", ["x"])), "terminal t3");
  assert.equal(chipLabel({ kind: "commit", id: "abcdef0123" }), "commit abcdef0");
  assert.equal(chipLabel(annotationChip(filePage("www/index.html"), "body > main > button:nth-of-type(2)", "<button>", "make it blue")), "index.html · button:nth-of-type(2)", "the file and the element's own segment");
  assert.equal(chipLabel(annotationChip(filePage("www/index.html"), "#save", "<button>", "x")), "index.html · #save");
  assert.equal(chipLabel(annotationChip(urlPage("http://localhost:5173/shop/pricing"), "#cta", "<a>", "x")), "pricing · #cta", "a page the browser showed: its last path segment");
  assert.equal(chipLabel(annotationChip(urlPage("http://localhost:5173/"), "#cta", "<a>", "x")), "localhost:5173 · #cta", "the root: its host");
  assert.equal(pageWords(filePage("a.html")), "a.html");
  assert.equal(pageWords(urlPage("http://x/")), "http://x/");
});

test("an annotated element is one chip however often it is pointed at: the newer note replaces the older", () => {
  let refs = attach([], annotationChip(filePage("index.html"), "#save", "<button>Save</button>", "make it blue"));
  refs = attach(refs, annotationChip(filePage("index.html"), "#cancel", "<button>Cancel</button>", "hide it"));
  refs = attach(refs, annotationChip(filePage("index.html"), "#save", "<button>Save</button>", "make it green"));
  assert.deepEqual(refs.map((r) => [r.selector, r.note]), [["#cancel", "hide it"], ["#save", "make it green"]]);
  assert.ok(sameChip(annotationChip(filePage("a.html"), "#x", "<i>", "1"), annotationChip(filePage("a.html"), "#x", "<i>", "2")));
  assert.equal(sameChip(annotationChip(filePage("a.html"), "#x", "<i>", "1"), annotationChip(filePage("b.html"), "#x", "<i>", "1")), false, "the same locator in another page is another element");
  assert.equal(sameChip(annotationChip(filePage("a.html"), "#x", "<i>", "1"), annotationChip(urlPage("http://localhost:5173/a.html"), "#x", "<i>", "1")), false, "a file and a served page are two pages");
  assert.ok(sameChip(annotationChip(urlPage("http://localhost:5173/"), "#x", "<i>", "1"), annotationChip(urlPage("http://localhost:5173/"), "#x", "<i>", "2")));
});

test("a terminal chip keeps the last lines and drops trailing blanks", () => {
  const lines = ["one", "two", "three", "", "  "];
  assert.equal(terminalChip("t1", lines).tail, "one\ntwo\nthree");
  assert.equal(terminalChip("t1", lines, 2).tail, "two\nthree");
  assert.equal(terminalChip("t1", []).tail, "");
});

test("a long selection is cut and says so", () => {
  const text = Array.from({ length: 10 }, (_, i) => `l${i + 1}`).join("\n");
  const chip = selectionChip("f.rs", 1, 10, text, 3);
  assert.ok(chip.text.endsWith("… (cut)"));
  assert.deepEqual(chip.range, { start: 1, end: 3 });
  assert.equal(selectionChip("f.rs", 0, 0, "x").range.start, 1, "ranges are 1-based");
});

test("attaching replaces an earlier chip for the same thing", () => {
  let refs = attach([], fileChip("a.rs"));
  refs = attach(refs, fileChip("b.rs"));
  refs = attach(refs, fileChip("a.rs"));
  assert.deepEqual(refs.map((r) => r.path), ["b.rs", "a.rs"], "moved to the end, not duplicated");
  refs = attach(refs, terminalChip("t1", ["old"]));
  refs = attach(refs, terminalChip("t1", ["new"]));
  assert.equal(refs.filter((r) => r.kind === "terminal").length, 1);
  assert.equal(refs.at(-1).tail, "new");
});

test("the wire bound is checked before sending", () => {
  assert.ok(fitsBudget([fileChip("a.rs")]));
  const big = terminalChip("t1", ["x".repeat(MAX_CONTEXT_BYTES)]);
  assert.ok(!fitsBudget([big]));
  assert.ok(contextBytes([big]) > MAX_CONTEXT_BYTES);
});

test("the frame states the placement: the project, then its goals — a project on no goal is the project alone", () => {
  assert.deepEqual(frameChips(null, []), []);
  assert.deepEqual(frameChips({ name: "Web app", slug: "web-app" }, []), [{ kind: "project", label: "Web app" }], "nothing says standalone");
  const f = frameChips({ name: "", slug: "web-app" }, [{ id: "g1", label: "Dark mode" }]);
  assert.deepEqual(f, [
    { kind: "project", label: "web-app" },
    { kind: "goal", label: "Dark mode", id: "g1" },
  ]);
});

test("sameChip is the identity attach() deduplicates by: the thing, not the bytes", () => {
  assert.ok(sameChip(fileChip("a.ts"), fileChip("a.ts")));
  assert.equal(sameChip(fileChip("a.ts"), fileChip("b.ts")), false);
  assert.ok(sameChip(terminalChip("t1", ["old"]), terminalChip("t1", ["new"])), "a fresher tail of the same terminal is the same chip");
  assert.equal(sameChip(fileChip("a.ts"), selectionChip("a.ts", 1, 2, "x")), false, "a file and a range of it are two things");
});

test("a capture chip is labelled by its device and its rectangle, and the same spot of the same picture is one chip (ide/19)", () => {
  const shot = { sha256: "ab".repeat(32), name: "mobile-A-1.png", mime: "image/png", size: 10 };
  const marked = { kind: "capture", device: "AAAA-1", label: "iPhone 16", shot, mark: { x: 1, y: 2, width: 30, height: 40 }, note: "blue" };
  const whole = { kind: "capture", device: "AAAA-1", label: "iPhone 16", shot, mark: null, note: "dark" };
  assert.equal(chipLabel(marked), "iPhone 16 · 30×40");
  assert.equal(chipLabel(whole), "iPhone 16 · screen");
  assert.ok(sameChip(marked, { ...marked, note: "green" }));
  assert.ok(!sameChip(marked, whole));
  assert.equal(attach([marked], { ...marked, note: "green" }).length, 1, "replaced, not stacked");
  assert.equal(attach([marked], whole).length, 2);
});
