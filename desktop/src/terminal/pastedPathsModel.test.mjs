import test from "node:test";
import assert from "node:assert/strict";
import { pasteAsksShell, pasteChoice, shellQuoted, typedPaths } from "./pastedPathsModel.mjs";

test("a plain path is typed as it is", () => {
  assert.equal(shellQuoted("/Users/ada/Desktop/shot.png"), "/Users/ada/Desktop/shot.png");
  assert.equal(shellQuoted("/tmp/a_b-c.d,e:f+g@h%i=j"), "/tmp/a_b-c.d,e:f+g@h%i=j", "punctuation no shell reads stays bare");
});

test("every character a shell reads goes behind a backslash, as Terminal.app types a dropped file", () => {
  assert.equal(shellQuoted("/Users/ada/Screen Shot 2026-10-04 at 15.02.11.png"), "/Users/ada/Screen\\ Shot\\ 2026-10-04\\ at\\ 15.02.11.png");
  assert.equal(shellQuoted(`/a/it's "x"`), `/a/it\\'s\\ \\"x\\"`);
  assert.equal(shellQuoted("/a/$HOME&(b);c|d<e>f"), "/a/\\$HOME\\&\\(b\\)\\;c\\|d\\<e\\>f");
  assert.equal(shellQuoted("/a/*?[x]{y}!#~^`\\"), "/a/\\*\\?\\[x\\]\\{y\\}\\!\\#\\~\\^\\`\\\\");
});

test("a letter beyond ASCII is kept; a control character puts the path in single quotes", () => {
  assert.equal(shellQuoted("/Users/ada/Café/Résumé — été.png"), "/Users/ada/Café/Résumé\\ —\\ été.png");
  assert.equal(shellQuoted("/a/line\nbreak.png"), "'/a/line\nbreak.png'");
  assert.equal(shellQuoted("/a/it's\tx"), "'/a/it'\\''s\tx'", "a quote inside single quotes is closed, escaped and reopened");
});

test("several paths are typed one space apart; none types nothing", () => {
  assert.equal(typedPaths(["/a/one.png", "/b/two three.png"]), "/a/one.png /b/two\\ three.png");
  assert.equal(typedPaths([]), "");
  assert.equal(typedPaths(undefined), "");
});

test("only a paste carrying files, or no text, asks the shell — plain text is the terminal's as it always was", () => {
  assert.equal(pasteAsksShell({ types: ["Files"], text: "" }), true, "a copy in the file manager");
  assert.equal(pasteAsksShell({ types: ["text/plain", "Files"], text: "shot.png" }), true, "the engine's name beside the files");
  assert.equal(pasteAsksShell({ types: [], text: "" }), true, "a picture the engine holds back");
  assert.equal(pasteAsksShell({ types: ["text/plain"], text: "ls -la" }), false);
  assert.equal(pasteAsksShell({ types: undefined, text: "x" }), false);
});

test("what the paste types: the files' paths, else its text, else the picture, else its own empty text", () => {
  assert.deepEqual(pasteChoice({ paths: ["/a/shot.png"], image: true }, "shot.png"), { kind: "paths", paths: ["/a/shot.png"] }, "a copied image file is its path, never its icon");
  assert.deepEqual(pasteChoice({ paths: [], image: true }, "https://example.org/x.png"), { kind: "text", text: "https://example.org/x.png" }, "text on the clipboard is pasted as text");
  assert.deepEqual(pasteChoice({ paths: [], image: true }, ""), { kind: "picture" }, "a screenshot");
  assert.deepEqual(pasteChoice({ paths: [], image: false }, ""), { kind: "text", text: "" }, "nothing: what xterm would have pasted");
  assert.deepEqual(pasteChoice(null, "x"), { kind: "text", text: "x" });
  assert.deepEqual(pasteChoice({ paths: ["relative.png", 7], image: false }, ""), { kind: "text", text: "" }, "only an absolute path is a path");
});
