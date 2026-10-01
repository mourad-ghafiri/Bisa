import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { looseRefusal } from "../../apiModel.mjs";

import { AUTOSAVE_FLOOR_MS, autosaveDue, breadcrumbsOf, changedOnDisk, clampAutosaveDelay, conflicted, edited, emptyBuffer, basenameOf, isDirty, isLoose, isUnsaved, isUntitled, keepMine, loaded, reformatted, remounted, saveFailed, savePathProblem, saveStarted, saved, sourceName, takeTheirs, unsavedKeys, untitledBuffer, DEFAULT_TAB_SIZE, EDITABLE_BYTES, formatOnSave, saveFailure, loadFailed, loadFailure, overBoundWords, refusalWords, sizeNote } from "./editorModel.mjs";

const file = (text, extra = {}) => ({ text, hash: `h(${text})`, editable: true, truncated: false, binary: false, size: text.length, ...extra });

test("a loaded buffer is clean and remembers the hash a save must carry", () => {
  const b = loaded(emptyBuffer("a.rs"), file("fn main() {}"));
  assert.equal(b.status, "clean");
  assert.equal(b.savedHash, "h(fn main() {})");
  assert.equal(isDirty(b), false);
});

test("editing makes it dirty from the first keystroke, and typing back the saved text makes it clean", () => {
  let b = loaded(emptyBuffer("a.rs"), file("x"));
  b = edited(b, "xy", 1000);
  assert.equal(b.status, "dirty");
  assert.equal(b.dirtySince, 1000);
  b = edited(b, "xyz", 1500);
  assert.equal(b.dirtySince, 1000, "the first unsaved keystroke is the clock");
  b = edited(b, "x", 2000);
  assert.equal(b.status, "clean");
  assert.equal(b.dirtySince, null);
});

test("a save records the new base; keystrokes during the save keep the buffer dirty", () => {
  let b = edited(loaded(emptyBuffer("a.rs"), file("x")), "xy", 1);
  b = saved(b, "xy", "h2");
  assert.equal(b.status, "clean");
  assert.equal(b.savedHash, "h2");
  b = edited(b, "xyz", 5);
  b = saved(b, "xy", "h2"); // the save that was in flight carried "xy"
  assert.equal(b.status, "dirty", "what was typed after the save went out is still unsaved");
  assert.equal(b.text, "xyz");
});

test("a save that formatted: the buffer nobody touched since takes what went to disk and is clean — never dirty against its own save", () => {
  // Typed, saved with format on save: the formatter answered other text than was typed.
  let b = saveStarted(edited(loaded(emptyBuffer("a.rs"), file("fn x(){}")), "fn  y( ){}", 1000));
  const typed = b.text;
  b = reformatted(b, typed, "fn y() {}\n");
  assert.equal(b.text, "fn y() {}\n", "the editor shows what the save writes");
  assert.equal(b.status, "saving", "still the same save");
  b = saved(b, "fn y() {}\n", "h2");
  assert.equal(b.status, "clean", "what is on disk is what is on screen");
  assert.equal(b.dirtySince, null);
  assert.equal(autosaveDue(b, "after_delay", 1000, 999_999), false, "nothing is due: an autosave that found the buffer dirty against its own formatted save wrote it again for ever");

  // The person typed while the formatter was out: what they typed stays, dirty against the text on disk.
  let moved = saveStarted(edited(loaded(emptyBuffer("a.rs"), file("fn x(){}")), "fn  y( ){}", 1000));
  moved = edited(moved, "fn  y( ){} // more", 1200);
  const kept = reformatted(moved, "fn  y( ){}", "fn y() {}\n");
  assert.equal(kept, moved, "the same buffer: a formatter never overwrites a keystroke");
  const after = saved(kept, "fn y() {}\n", "h2");
  assert.equal(after.status, "dirty");
  assert.equal(after.text, "fn  y( ){} // more");
  assert.equal(after.savedText, "fn y() {}\n");
  // A formatter that changed nothing changes nothing.
  assert.equal(reformatted(b, b.text, b.text), b);
  // The document's save puts the formatter's text on the buffer before it writes it.
  const doc = readFileSync(new URL("./EditorDoc.tsx", import.meta.url), "utf8");
  const save = doc.slice(doc.indexOf("const save = useCallback(async (): Promise<boolean> => {"), doc.indexOf("// One registration per key"));
  assert.ok(save.includes("setBuffer((cur) => reformatted(cur, b.text, formatted));"));
  assert.ok(save.indexOf("reformatted(cur, b.text, formatted)") < save.indexOf("await backend.write(text, b.savedHash)"));
});

test("a conflict keeps every typed character and offers both resolutions", () => {
  let b = edited(loaded(emptyBuffer("a.rs"), file("x")), "x mine", 1);
  b = conflicted(b, "x theirs", "ht");
  assert.equal(b.status, "conflict");
  assert.equal(b.text, "x mine");
  const mine = keepMine(b);
  assert.equal(mine.text, "x mine");
  assert.equal(mine.savedHash, "ht", "the next save is against what is on disk now");
  assert.equal(mine.status, "dirty");
  const theirs = takeTheirs(b);
  assert.equal(theirs.text, "x theirs");
  assert.equal(theirs.status, "clean");
});

test("a change on disk reloads a clean buffer silently and raises a conflict on a dirty one", () => {
  const clean = loaded(emptyBuffer("a.rs"), file("x"));
  const reloaded = changedOnDisk(clean, file("y"));
  assert.equal(reloaded.text, "y");
  assert.equal(reloaded.status, "clean");
  const dirty = edited(clean, "x mine", 1);
  const told = changedOnDisk(dirty, file("y"));
  assert.equal(told.status, "conflict");
  assert.equal(told.text, "x mine", "the typed text is untouched");
  assert.equal(told.conflict.theirs, "y");
  // The disk holds what this editor saved — its own announced write: no
  // change, no conflict, whether the buffer is clean or dirty since.
  assert.equal(changedOnDisk(clean, file("x")), clean, "the same hash is no change");
  assert.equal(changedOnDisk(dirty, file("x")), dirty, "a dirty buffer over its own save is not told");
});

test("read-only, binary and failed loads never become dirty", () => {
  const ro = loaded(emptyBuffer("big.txt"), file("a".repeat(10), { editable: false }));
  assert.equal(ro.status, "read_only");
  assert.equal(edited(ro, "b", 1).text, "a".repeat(10));
  const bin = loaded(emptyBuffer("x.png"), { text: null, hash: null, editable: false, truncated: false, binary: true, size: 9 });
  assert.equal(bin.status, "binary");
  assert.equal(edited(bin, "b", 1).status, "binary");
  const failed = saveFailed(edited(loaded(emptyBuffer("a"), file("x")), "xy", 1), new Error("boom"));
  assert.equal(failed.status, "dirty");
  assert.equal(failed.error, "Error: boom");
});

test("autosave fires after the delay from the first unsaved keystroke, and only in after_delay mode", () => {
  const b = edited(loaded(emptyBuffer("a"), file("x")), "xy", 1000);
  assert.equal(autosaveDue(b, "after_delay", 1000, 1500), false);
  assert.equal(autosaveDue(b, "after_delay", 1000, 2000), true);
  assert.equal(autosaveDue(b, "on_focus_change", 1000, 9000), false);
  assert.equal(autosaveDue(b, "off", 1000, 9000), false);
  assert.equal(autosaveDue(loaded(emptyBuffer("a"), file("x")), "after_delay", 1000, 9000), false, "a clean buffer has nothing to save");
});

test("the delay is clamped, and an unusable value is the default rather than the minimum", () => {
  assert.equal(clampAutosaveDelay(50), AUTOSAVE_FLOOR_MS);
  assert.equal(clampAutosaveDelay(99999), 10000);
  assert.equal(clampAutosaveDelay("2000"), 2000);
  assert.equal(clampAutosaveDelay(null), 1000);
  assert.equal(clampAutosaveDelay(""), 1000);
  assert.equal(clampAutosaveDelay([]), 1000);
});

test("breadcrumbs are every folder then the file, each naming the path up to it", () => {
  assert.deepEqual(breadcrumbsOf("src/ui/FileTree.tsx"), [
    { label: "src", path: "src", dir: true },
    { label: "ui", path: "src/ui", dir: true },
    { label: "FileTree.tsx", path: "src/ui/FileTree.tsx", dir: false },
  ]);
  assert.deepEqual(breadcrumbsOf("README.md"), [{ label: "README.md", path: "README.md", dir: false }]);
  assert.deepEqual(breadcrumbsOf(""), []);
});

test("an untitled document starts empty, clean and editable, with no hash so its first save is a create", () => {
  const source = { kind: "untitled", seq: 3 };
  assert.ok(isUntitled(source));
  assert.ok(!isUntitled({ kind: "file", path: "a.rs" }));
  assert.equal(sourceName(source), "Untitled-3");
  assert.equal(sourceName({ kind: "file", path: "src/a.rs" }), "src/a.rs");
  const b = untitledBuffer(source);
  assert.equal(b.status, "clean");
  assert.equal(b.text, "");
  assert.equal(b.savedHash, null);
  assert.ok(!b.readOnly);
  assert.ok(!isDirty(b));
  const typed = edited(b, "hello", 10);
  assert.equal(typed.status, "dirty");
  assert.ok(!isDirty(edited(typed, "", 11)), "typing back to nothing is nothing to save — the name is still asked for");
});

test("the path a new document is saved under is relative, forward-slashed, inside the root, and a file", () => {
  assert.equal(savePathProblem("notes/todo.md"), null);
  assert.equal(savePathProblem("  README  "), null, "trimmed");
  assert.equal(savePathProblem("a/b/c.txt"), null, "folders are made on the way");
  assert.equal(savePathProblem(""), "A path is needed.");
  assert.equal(savePathProblem("   "), "A path is needed.");
  assert.equal(savePathProblem(null), "A path is needed.");
  assert.equal(savePathProblem("src\\a.rs"), "Use forward slashes.");
  assert.equal(savePathProblem("/etc/hosts"), "A path is relative to the root.");
  assert.equal(savePathProblem("~/a.rs"), "A path is relative to the root.");
  assert.equal(savePathProblem("src/"), "A file, not a folder.");
  assert.equal(savePathProblem("../a.rs"), "That is not a path inside the root.");
  assert.equal(savePathProblem("src/../a.rs"), "That is not a path inside the root.");
  assert.equal(savePathProblem("./a.rs"), "That is not a path inside the root.");
  assert.equal(savePathProblem("src//a.rs"), "That is not a path inside the root.");
});

test("a loose source is named by its absolute path, and a name is a path's last segment", () => {
  const loose = { kind: "loose", path: "/Users/me/notes/todo.md" };
  assert.ok(isLoose(loose));
  assert.ok(!isLoose({ kind: "file", path: "todo.md" }));
  assert.ok(!isUntitled(loose));
  assert.equal(sourceName(loose), "/Users/me/notes/todo.md");
  assert.equal(basenameOf("/Users/me/notes/todo.md"), "todo.md");
  assert.equal(basenameOf("todo.md"), "todo.md");
  assert.equal(basenameOf(""), "");
});

test("the autosave settings are read from the resolved rows: a mode off the list is after_delay, the delay is clamped, no rows is the default", async () => {
  const { AUTOSAVE_CEILING_MS, AUTOSAVE_FLOOR_MS, autosaveFrom } = await import("./editorModel.mjs");
  assert.deepEqual(autosaveFrom([{ key: "editor.autosave.mode", value: "off" }, { key: "editor.autosave.delay_ms", value: 1500 }]), { mode: "off", delay: 1500 });
  assert.equal(autosaveFrom([{ key: "editor.autosave.mode", value: "sometimes" }]).mode, "after_delay", "an unknown word is the default");
  assert.equal(autosaveFrom([{ key: "editor.autosave.mode", value: 3 }]).mode, "after_delay", "a number is not a mode");
  assert.equal(autosaveFrom([{ key: "editor.autosave.delay_ms", value: 1 }]).delay, AUTOSAVE_FLOOR_MS);
  assert.equal(autosaveFrom([{ key: "editor.autosave.delay_ms", value: 10 ** 9 }]).delay, AUTOSAVE_CEILING_MS);
  assert.equal(autosaveFrom(null).mode, "after_delay");
  assert.equal(autosaveFrom(undefined).delay, autosaveFrom([]).delay, "no rows and no key agree");
});

test("unsaved is a fact of the buffer — of a document on screen or not — and an untitled one with text is unsaved", () => {
  const untitled = untitledBuffer({ kind: "untitled", seq: 1 });
  assert.equal(isUnsaved(untitled), false, "a ⌘N document nobody typed in closes without a question");
  const typed = edited(untitled, "a thought", 1000);
  assert.equal(isUnsaved(typed), true);
  assert.equal(isUnsaved(edited(typed, "", 2000)), false, "typed back to nothing: nothing to lose");

  const clean = loaded(emptyBuffer("a.txt"), file("one"));
  const dirty = edited(clean, "one!", 1000);
  const conflict = conflicted(clean, "theirs", "h(theirs)");
  assert.equal(isUnsaved(clean), false);
  assert.equal(isUnsaved(dirty), true);
  assert.equal(isUnsaved(conflict), true, "a conflict nobody settled is work a close would lose");
  assert.equal(isUnsaved(null), false);
  assert.equal(isUnsaved(undefined), false);

  const buffers = new Map([["r|file:a.txt", clean], ["r|untitled:1", typed], ["r|file:b.txt", dirty], ["r|file:c.txt", conflict]]);
  assert.deepEqual(unsavedKeys(buffers), ["r|untitled:1", "r|file:b.txt", "r|file:c.txt"]);
  assert.deepEqual(unsavedKeys(new Map()), []);
});

test("a tab that comes back on screen keeps its unsaved text, and the read that follows compares it with the disk", () => {
  const clean = loaded(emptyBuffer("a.txt"), file("one"));
  const dirty = edited(clean, "one, edited", 1000);

  // Unsaved work is kept exactly as typed; anything else is read afresh.
  assert.equal(remounted(dirty, "a.txt"), dirty);
  assert.deepEqual(remounted(clean, "a.txt"), emptyBuffer("a.txt"));
  assert.deepEqual(remounted(null, "a.txt"), emptyBuffer("a.txt"));
  assert.deepEqual(remounted(undefined, "a.txt"), emptyBuffer("a.txt"));

  // The disk did not move while it was off screen: still dirty, no conflict.
  const same = changedOnDisk(remounted(dirty, "a.txt"), file("one"));
  assert.equal(same.text, "one, edited");
  assert.equal(same.status, "dirty");
  // It did — an agent wrote the file meanwhile: the three-way affordance, and the typed text untouched.
  const moved = changedOnDisk(remounted(dirty, "a.txt"), file("one, by an agent"));
  assert.equal(moved.status, "conflict");
  assert.equal(moved.text, "one, edited");
  assert.equal(moved.conflict.theirs, "one, by an agent");

  // Unmounted mid-save: the save's answer never came back to this buffer, so it is dirty, not saving for ever.
  const midSave = { ...dirty, status: "saving" };
  assert.equal(remounted(midSave, "a.txt").status, "dirty");
  assert.equal(remounted(midSave, "a.txt").text, "one, edited");
});

test("a save formats only when the setting is exactly on and a server follows the document, with the editor's own options", () => {
  const on = [{ key: "editor.format_on_save", value: true }, { key: "editor.tab_size", value: 2 }, { key: "editor.insert_spaces", value: false }];
  assert.deepEqual(formatOnSave(on, "rust"), { tabSize: 2, insertSpaces: false });
  assert.equal(formatOnSave(on, null), null, "no server follows this document");
  assert.equal(formatOnSave(on, ""), null);
  for (const off of [false, "true", 1, null, undefined]) assert.equal(formatOnSave([{ key: "editor.format_on_save", value: off }], "rust"), null, `${JSON.stringify(off)} is not on`);
  assert.equal(formatOnSave(null, "rust"), null, "settings still out: a save never waits for them");
  assert.equal(formatOnSave([], "rust"), null);
  // A tab size nobody can use is the default; spaces unless exactly false.
  for (const size of [0, -2, 17, 2.5, "4", null, Number.NaN]) {
    assert.deepEqual(formatOnSave([{ key: "editor.format_on_save", value: true }, { key: "editor.tab_size", value: size }], "ts"), { tabSize: DEFAULT_TAB_SIZE, insertSpaces: true }, JSON.stringify(size));
  }
  assert.equal(formatOnSave([{ key: "editor.format_on_save", value: true }, { key: "editor.tab_size", value: 16 }], "ts").tabSize, 16);
});

test("a save that threw is a conflict when the file moved on disk, and a failure in its own words otherwise", () => {
  const moved = Object.assign(new Error("conflict"), { status: 409, body: { current_text: "theirs\n", current_hash: "h2" } });
  assert.deepEqual(saveFailure(moved), { kind: "conflict", text: "theirs\n", hash: "h2" });
  assert.deepEqual(saveFailure(Object.assign(new Error("conflict"), { status: 409 })), { kind: "conflict", text: "", hash: null }, "a conflict the node sent no body for is still one");
  assert.deepEqual(saveFailure(Object.assign(new Error("conflict"), { status: 409, body: { current_text: 7, current_hash: {} } })), { kind: "conflict", text: "", hash: null }, "a body nobody can read is read as nothing");
  assert.deepEqual(saveFailure(Object.assign(new Error("read-only file system"), { status: 500 })), { kind: "failed", message: "read-only file system" });
  assert.deepEqual(saveFailure(new TypeError("Failed to fetch")), { kind: "failed", message: "Failed to fetch" });
  assert.deepEqual(saveFailure("offline"), { kind: "failed", message: "offline" });
  assert.deepEqual(saveFailure(null), { kind: "failed", message: "null" });
  // The buffer after each: a conflict holds theirs beside mine, a failure keeps the text dirty.
  const dirty = edited(loaded(emptyBuffer("a.txt"), { text: "base\n", hash: "h1", editable: true }), "mine\n", 1);
  const c = saveFailure(moved);
  assert.equal(conflicted(dirty, c.text, c.hash).text, "mine\n", "what was typed is never lost to a conflict");
  assert.ok(isDirty(saveFailed(dirty, "disk full")), "a failed save is still unsaved work");
});


test("the size a file is drawn plain above is the editable size nobody set, byte for byte", () => {
  // *Editable up to* is this machine's to move (`editor.large_file.editable_mib`);
  // what it is when nobody moved it is the engine's (`EditorCaps::UNSET`, read by
  // the node where a file is read and saved), and the editor tokenises up to the
  // same number: a file within it is edited as it always was.
  const here = dirname(fileURLToPath(import.meta.url));
  const src = readFileSync(join(here, "../../../../crates/bisa-engine/src/ide/files.rs"), "utf8");
  const unset = src.match(/pub const UNSET: EditorCaps = EditorCaps \{\s*editable: (\d+) \* MIB,/);
  assert.ok(unset, "the engine declares the bound nobody set");
  const mib = src.match(/const MIB: u64 = ([0-9 *]+);/);
  assert.ok(mib, "and what a MiB is");
  const bytes = mib[1].split("*").reduce((acc, n) => acc * Number(n.trim()), 1);
  assert.equal(EDITABLE_BYTES, Number(unset[1]) * bytes);
  // The node's own constant is that bound, by name — never a second number.
  const node = readFileSync(join(here, "../../../../crates/bisa-node/src/ide.rs"), "utf8");
  assert.match(node, /pub const EDITABLE_BYTES: u64 = files::EditorCaps::UNSET\.editable;/);
});

/** An answer as the client raises it: a status, the node's sentence, what the body carried. */
const refused = (status, message, body) => Object.assign(new Error(message), { status, body });

test("a loose file over the shell's bound is refused as a root file's 413 is: the size and the limit the shell said, the reveal offered; a save over it the same sentence", () => {
  const MIB = 1024 * 1024;
  // The shell's tagged refusal (`LooseError::TooLarge`), as the loose backend raises it (`apiModel.looseRefusal`).
  const shell = looseRefusal({ kind: "too_large", message: "/Users/me/big.log is 25165824 bytes; the editor stops at 20971520", size: 24 * MIB, limit: 20 * MIB }, "the shell refused");
  const failure = loadFailure(refused(shell.status, shell.message, shell.body));
  assert.deepEqual(failure, { kind: "too_large", message: "/Users/me/big.log is 25165824 bytes; the editor stops at 20971520", size: 24 * MIB, limit: 20 * MIB });
  assert.match(refusalWords(failure), /^24\.0 MB — .*stops at 20\.0 MB .*Reveal the file/, "the size, the shell's limit, and the way out");
  const buffer = loadFailed(emptyBuffer("/Users/me/big.log"), failure);
  assert.equal(buffer.status, "error");
  assert.equal(buffer.refusal.kind, "too_large", "kept on the buffer, where the screen offers Reveal");
  const save = saveFailure(refused(shell.status, shell.message, shell.body));
  assert.equal(save.kind, "failed");
  assert.match(save.message, /the text is 24\.0 MB and this machine edits up to 20\.0 MB/);
  // A missing loose file and a conflict read as a root file's do.
  const gone = looseRefusal({ kind: "missing", message: "no such file" }, "the shell refused");
  assert.deepEqual(loadFailure(refused(gone.status, gone.message, gone.body)), { kind: "missing", message: "no such file" });
});
const MIB = 1024 * 1024;

test("whether a file may be edited is the node's word on it, at whatever size this machine set — never a size compared here", () => {
  // The bound lowered to 1 MiB: a file of a MiB and a half is the node's to call read-only.
  const lowered = loaded(emptyBuffer("a.log"), file("x", { size: 1.5 * MIB, editable: false }));
  assert.deepEqual([lowered.status, lowered.readOnly, lowered.readOnlyWhy, lowered.plain], ["read_only", true, "size", false]);
  assert.equal(sizeNote(lowered), "Read-only: above the editable size this machine set (Settings › Project IDE › Editor).", "the reason is said below the size the editor tokenises, too");
  // The bound raised to 8 MiB: a file of five is edited, drawn plain.
  const raised = loaded(emptyBuffer("a.log"), file("x", { size: 5 * MIB, editable: true }));
  assert.deepEqual([raised.status, raised.readOnly, raised.readOnlyWhy, raised.plain], ["clean", false, null, true]);
  assert.equal(sizeNote(raised), "Drawn plain: above the size the editor tokenises.");
  assert.equal(edited(raised, "xy", 1).status, "dirty", "and typed into");
  // Nobody moved the bound: above it, read-only and plain.
  const large = loaded(emptyBuffer("a.log"), file("x", { size: 3 * MIB, editable: false }));
  assert.equal(sizeNote(large), "Read-only: above the editable size, tokenisation is off.");
  // Cut where the read stops: read-only whatever the node says of its size.
  const cut = loaded(emptyBuffer("a.log"), file("x", { truncated: true }));
  assert.deepEqual([cut.readOnly, cut.readOnlyWhy], [true, "truncated"]);
  assert.equal(sizeNote(cut), "Read-only: the file was cut where the read stops.");
  assert.equal(sizeNote(loaded(emptyBuffer("a.rs"), file("x"))), null, "an ordinary file says nothing");
  // No number decides it: the model holds one size, the one a file is drawn plain above.
  const source = readFileSync(new URL("./editorModel.mjs", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  assert.deepEqual([...source.matchAll(/\b(\d+) \* 1024 \* 1024\b/g)].map((m) => m[0]), ["2 * 1024 * 1024"]);
  assert.ok(!/MiB/.test(source), "and spells no bound in its words");
});

test("a read refused for its size carries the size and the limit the node said; a file that is gone and a failure are told apart", () => {
  const big = loadFailure(refused(413, "big.log is 12582912 bytes; the editor stops at 8388608", { error: "…", size: 12 * MIB, limit: 8 * MIB }));
  assert.deepEqual(big, { kind: "too_large", message: "big.log is 12582912 bytes; the editor stops at 8388608", size: 12 * MIB, limit: 8 * MIB });
  assert.equal(refusalWords(big), "12.0 MB — the editor on this machine stops at 8.0 MB (Settings › Project IDE › Editor). Reveal the file and open it with another application.");
  // This machine raised the bound to its most: the words follow the node, not a number kept here.
  assert.match(refusalWords(loadFailure(refused(413, "x", { size: 70 * MIB, limit: 64 * MIB }))), /stops at 64\.0 MB/);
  // An answer that carried no sizes says so without inventing one.
  const bare = loadFailure(refused(413, "too large", undefined));
  assert.deepEqual([bare.size, bare.limit], [null, null]);
  assert.equal(refusalWords(bare), "Too large for the editor on this machine — Settings › Project IDE › Editor sets the size it stops at. Reveal the file and open it with another application.");
  assert.deepEqual(loadFailure(refused(413, "x", { size: "12", limit: -1 })).size, null, "a size that is no number is no size");
  assert.deepEqual(loadFailure(refused(404, "no such file: a.rs")), { kind: "missing", message: "no such file: a.rs" });
  assert.deepEqual(loadFailure(new Error("the node did not answer")), { kind: "failed", message: "the node did not answer" });
  assert.deepEqual(loadFailure("offline"), { kind: "failed", message: "offline" });
  assert.equal(refusalWords(loadFailure(refused(404, "gone"))), null, "the node's sentence says it all");
  assert.equal(refusalWords(null), null);
});

test("a read that fails shows its refusal on a document with nothing typed, and takes nothing typed off the screen", () => {
  const failure = loadFailure(refused(404, "no such file: a.rs"));
  const opening = loadFailed(emptyBuffer("a.rs"), failure);
  assert.deepEqual([opening.status, opening.error, opening.refusal], ["error", "no such file: a.rs", failure]);
  const clean = loadFailed(loaded(emptyBuffer("a.rs"), file("x")), failure);
  assert.equal(clean.status, "error", "a clean document whose file went says so");
  // Typed work: the screen stays the editor's, the failure is noted beside it.
  const typed = edited(loaded(emptyBuffer("a.rs"), file("x")), "x mine", 1);
  const kept = loadFailed(typed, loadFailure(new Error("the node did not answer")));
  assert.deepEqual([kept.status, kept.text, kept.error, kept.refusal], ["dirty", "x mine", "the node did not answer", null]);
  assert.ok(isUnsaved(kept));
  const inConflict = loadFailed(conflicted(typed, "x theirs", "ht"), failure);
  assert.equal(inConflict.status, "conflict", "a conflict nobody settled is work too");
  // The next read that answers mends a refusal, and leaves no word of it behind.
  const mended = changedOnDisk(opening, file("back"));
  assert.deepEqual([mended.status, mended.text, mended.error, mended.refusal], ["clean", "back", null, null]);
});

test("a save refused for its size says the limit the node said", () => {
  const over = saveFailure(refused(413, "a.log is 3145728 bytes; the editor stops at 1048576", { size: 3 * MIB, limit: MIB }));
  assert.deepEqual(over, { kind: "failed", message: "the text is 3.0 MB and this machine edits up to 1.0 MB (Settings › Project IDE › Editor)" });
  assert.deepEqual(saveFailure(refused(413, "too large", {})), { kind: "failed", message: "too large" }, "no limit said: the node's own sentence stands");
  assert.equal(overBoundWords(null, MIB), null);
  assert.equal(overBoundWords(2 * MIB, 8 * MIB), "the text is 2.0 MB and this machine edits up to 8.0 MB (Settings › Project IDE › Editor)");
  // The editor draws both through the model, and keeps no bound of its own in its words.
  const doc = readFileSync(new URL("./EditorDoc.tsx", import.meta.url), "utf8");
  assert.ok(doc.includes("loadFailed(b, failure)") && doc.includes("refusalWords(buffer.refusal)") && doc.includes("sizeNote(buffer)"));
  assert.ok(!/\b\d+ MiB\b/.test(doc.replace(/\/\*[\s\S]*?\*\//g, "").replace(/^\s*\/\/.*$/gm, "")), "EditorDoc.tsx says no size of its own");
});

test("the editor's reads land in order and never on a closed tab", () => {
  const doc = readFileSync(new URL("./EditorDoc.tsx", import.meta.url), "utf8");
  const load = doc.slice(doc.indexOf("const load = useCallback"), doc.indexOf("useEffect(() => {\n    if (!onDisk) return;\n    // Unsaved work"));
  assert.ok(load.includes("const ticket = reads.begin();"), "a read takes its ticket before it asks");
  assert.equal(load.split("if (!reads.lands(ticket)) return;").length - 1, 2, "the answer and the refusal are both held to it");
  assert.ok(load.indexOf("if (!reads.lands(ticket)) return;") < load.indexOf("setBuffer("), "before anything is written");
  assert.ok(doc.includes("return () => reads.close();"), "the tab leaving closes the order");
});
