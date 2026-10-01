/**
 * `@file` in the composer. Run with `node --test desktop/src/ui/fileMentionModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { fileSuggestions, fileTokenPresent, insertFileMention, syncFileMentions, wantsFiles } from "./fileMentionModel.mjs";

const PATHS = ["src/main.rs", "src/lib.rs", "docs/guide.md", "README.md", "Cargo.toml"];

test("a run with a slash or a dot asks for files; so does one nobody's name matched", () => {
  assert.equal(wantsFiles("src/", 3), true);
  assert.equal(wantsFiles("main.rs", 0), true);
  assert.equal(wantsFiles("ada", 1), false, "a name that matched somebody is a mention");
  assert.equal(wantsFiles("zzz", 0), true, "nothing matched: a path is all it could be");
  assert.equal(wantsFiles("", 0), false, "an empty run offers people, not every file");
  assert.equal(wantsFiles("   ", 0), false);
});

test("file suggestions come from the palette's scorer, best first, capped", () => {
  assert.deepEqual(fileSuggestions("main", PATHS), ["src/main.rs"]);
  const src = fileSuggestions("src/", PATHS);
  assert.deepEqual(new Set(src), new Set(["src/main.rs", "src/lib.rs"]));
  assert.equal(fileSuggestions("rs", PATHS, 1).length, 1, "the cap holds");
  assert.deepEqual(fileSuggestions("", PATHS), []);
  assert.deepEqual(fileSuggestions("main", []), []);
});

test("picking a file splices its path over the run and puts the caret after it", () => {
  const out = insertFileMention("look at @src/ma and tell me", { at: 8, end: 15 }, "src/main.rs");
  assert.equal(out.text, "look at @src/main.rs  and tell me");
  assert.equal(out.caret, 8 + "@src/main.rs ".length);
});

test("a file token is present only as a whole word", () => {
  assert.equal(fileTokenPresent("see @src/main.rs please", "src/main.rs"), true);
  assert.equal(fileTokenPresent("@src/main.rs", "src/main.rs"), true, "at the very start");
  assert.equal(fileTokenPresent("see @src/main.rsx", "src/main.rs"), false, "a longer path is another file");
  assert.equal(fileTokenPresent("see x@src/main.rs", "src/main.rs"), false, "mid-word is not a token");
  assert.equal(fileTokenPresent("see src/main.rs", "src/main.rs"), false, "without the @ it is prose");
  assert.equal(fileTokenPresent("see @a+b.(x) here", "a+b.(x)"), true, "regex characters in a path are literal");
});

test("editing a token out of the body drops its chip and keeps the others", () => {
  const picked = ["src/main.rs", "docs/guide.md"];
  assert.deepEqual(syncFileMentions("fix @src/main.rs using @docs/guide.md", picked), { kept: picked, dropped: [] });
  assert.deepEqual(syncFileMentions("fix @src/main.rs using the guide", picked), { kept: ["src/main.rs"], dropped: ["docs/guide.md"] });
  assert.deepEqual(syncFileMentions("", picked), { kept: [], dropped: picked });
  assert.deepEqual(syncFileMentions("anything", []), { kept: [], dropped: [] });
});
