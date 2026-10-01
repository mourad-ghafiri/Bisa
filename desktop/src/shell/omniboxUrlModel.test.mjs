/**
 * The palette's address row. Run with `node --test desktop/src/shell/omniboxUrlModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { urlRow } from "./omniboxUrlModel.mjs";

test("an address typed into the palette is one row that opens it; a word is a search", () => {
  assert.deepEqual(urlRow("example.com").url, "http://example.com/");
  assert.equal(urlRow("example.com").label, "Open example.com in the browser");
  assert.equal(urlRow("https://example.com/docs?x=1").url, "https://example.com/docs?x=1");
  assert.equal(urlRow("localhost:5173/admin").url, "http://localhost:5173/admin", "a bare host with a port is a page on this machine");
  assert.equal(urlRow("127.0.0.1:3000").url, "http://127.0.0.1:3000/");
  assert.equal(urlRow("  example.com  ").url, "http://example.com/", "trimmed");
  assert.equal(urlRow("pricing"), null, "a word with no dot, no port and no scheme is a search");
  assert.equal(urlRow("fix the login page"), null, "words with spaces are never an address");
  assert.equal(urlRow("ftp://files.example.com"), null, "another scheme is refused, as the bar refuses it");
  assert.equal(urlRow(""), null);
  assert.match(urlRow("example.com").hint, /beside this screen/);
});
