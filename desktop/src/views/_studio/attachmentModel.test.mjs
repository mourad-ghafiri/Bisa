/**
 * `attachmentModel.mjs` — how a file in a conversation is described.
 *
 * Run with `npm test` from `desktop/`.
 */

import test from "node:test";
import assert from "node:assert/strict";

import { describe, formatBytes, glyphFor, isRenderableImage } from "./attachmentModel.mjs";

const file = (over = {}) => ({
  sha256: "a".repeat(64),
  name: "shot.png",
  mime: "image/png",
  size: 421337,
  ...over,
});

test("a size reads the way a person would say it", () => {
  assert.equal(formatBytes(0), "0 B");
  assert.equal(formatBytes(512), "512 B");
  assert.equal(formatBytes(1024), "1.0 KB");
  assert.equal(formatBytes(421337), "411 KB");
  assert.equal(formatBytes(4 * 1024 * 1024), "4.0 MB");
  assert.equal(formatBytes(25 * 1024 * 1024), "25 MB");
  // Nothing sensible to say, so nothing is said.
  assert.equal(formatBytes(-1), "");
  assert.equal(formatBytes(NaN), "");
});

test("only the formats the byte route serves inline are attempted", () => {
  // Kept in step with `image_type` in bisa-node/src/attachments.rs.
  for (const mime of ["image/png", "image/jpeg", "image/gif", "image/webp"]) {
    assert.ok(isRenderableImage(file({ mime }), true), mime);
  }
  // An image the route will not serve inline. Trying would get an
  // octet-stream back and render a broken picture.
  assert.equal(isRenderableImage(file({ mime: "image/svg+xml" }), true), false);
  assert.equal(isRenderableImage(file({ mime: "image/tiff" }), true), false);
  assert.equal(isRenderableImage(file({ mime: "application/pdf" }), true), false);
  assert.ok(isRenderableImage(file({ mime: "IMAGE/PNG" }), true), "case is not meaning");
});

test("bytes that have not arrived are never rendered", () => {
  // The case this guards: a peer's photo syncs as a descriptor, and an <img>
  // pointed at bytes we do not have is a broken icon with no explanation.
  assert.equal(isRenderableImage(file(), false), false);
});

test("a glyph is chosen by media type, which the domain icon map cannot answer", () => {
  assert.equal(glyphFor("image/png"), "image");
  assert.equal(glyphFor("application/pdf"), "document");
  assert.equal(glyphFor("text/markdown"), "document");
  assert.equal(glyphFor("application/json"), "document");
  assert.equal(glyphFor("audio/mpeg"), "audio");
  assert.equal(glyphFor("video/mp4"), "video");
  assert.equal(glyphFor("application/zip"), "archive");
  assert.equal(glyphFor("application/x-tar"), "archive");
  assert.equal(glyphFor("application/octet-stream"), "file");
  assert.equal(glyphFor(""), "file");
  assert.equal(glyphFor(undefined), "file");
});

test("an absent file says so, because it is a state and not a failure", () => {
  assert.equal(describe(file(), true), "411 KB");
  assert.equal(describe(file(), false), "411 KB · not on this machine");
});
