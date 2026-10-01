/**
 * A picture pasted from the clipboard, and its name. Run with
 * `node --test desktop/src/ui/pastedImageModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { extensionOf, imageNameError, pastedImageName, pasteIntake, suggestedName, withExtension } from "./pastedImageModel.mjs";

test("a clipboard picture is offered as pasted-image-<stamp>.png, the stamp to the second", () => {
  assert.equal(pastedImageName(new Date("2026-09-15T14:30:12.345Z")), "pasted-image-20260915T143012Z.png");
  assert.match(pastedImageName(), /^pasted-image-\d{8}T\d{6}Z\.png$/, "now, by default");
});

test("a picture with a file behind it is offered under its own name; the engine's placeholder gets the stamped one", () => {
  const at = new Date("2026-09-15T14:30:12Z");
  assert.equal(suggestedName("photo.jpg", at), "photo.jpg");
  assert.equal(suggestedName("image.png", at), "pasted-image-20260915T143012Z.png", "`image.png` is what the engine calls a screenshot");
  assert.equal(suggestedName("Image.jpeg", at), "pasted-image-20260915T143012Z.jpeg", "the placeholder keeps its kind");
  assert.equal(suggestedName("", at), "pasted-image-20260915T143012Z.png");
});

test("a typed name keeps its own extension and is given the picture's when it has none", () => {
  assert.equal(withExtension("login-bug", "png"), "login-bug.png");
  assert.equal(withExtension("  login-bug  ", "png"), "login-bug.png", "trimmed");
  assert.equal(withExtension("login-bug.jpeg", "png"), "login-bug.jpeg", "the person's extension stands");
  assert.equal(withExtension("photo", "jpg"), "photo.jpg", "a pasted file's own kind");
  assert.equal(withExtension("", "png"), "", "nothing typed is nothing — the validator's word, not a .png");
  assert.equal(extensionOf("shot.PNG"), "png");
  assert.equal(extensionOf("Makefile"), "");
  assert.equal(extensionOf(".env"), "", "a dot-file has no extension");
});

test("a picture's name follows the tree's rule and refuses a dot-file", () => {
  assert.equal(imageNameError("login-bug.png", ["a.png"]), null);
  assert.equal(imageNameError("   ", []), "A name is needed.");
  assert.match(imageNameError("a/b.png", []), /no slash/);
  assert.equal(imageNameError("a.png", ["a.png"]), "a.png is already here.");
  assert.equal(imageNameError(".hidden.png", []), "A picture's name does not start with a dot.");
});

test("a paste sorts into pictures to name, files to take, and — empty of both files and text — a question for the shell", () => {
  const shot = { name: "image.png", type: "image/png" };
  const photo = { name: "photo.jpg", type: "image/jpeg" };
  const notes = { name: "notes.txt", type: "text/plain" };
  assert.deepEqual(pasteIntake([shot, notes, photo], false), { pictures: [shot, photo], files: [notes], askShell: false });
  assert.deepEqual(pasteIntake([], true), { pictures: [], files: [], askShell: false }, "a text paste is the field's own");
  assert.deepEqual(pasteIntake([], false), { pictures: [], files: [], askShell: true }, "nothing the engine hands over: the shell may hold a picture");
  assert.equal(pasteIntake([{ name: "x", type: "IMAGE/PNG" }], false).pictures.length, 1, "the type's case does not matter");
});
