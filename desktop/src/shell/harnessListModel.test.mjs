/**
 * When the installed-harness list is read again on its own.
 * Run with `node --test desktop/src/shell/harnessListModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { CATALOG_RETRY_MS, catalogIncomplete, nextCatalogRead } from "./harnessListModel.mjs";

const row = (installed, launch = {}) => ({ id: "h", label: "H", installed, launch });

test("a list that could not be read, or names no harness a person could launch, is incomplete; one installed and launchable settles it", () => {
  assert.equal(catalogIncomplete(null), true, "not read");
  assert.equal(catalogIncomplete([]), true, "nothing at all");
  assert.equal(catalogIncomplete([row(false)]), true, "a probe that said no — or timed out");
  assert.equal(catalogIncomplete([row(true, null)]), true, "installed, but nothing a person could open");
  assert.equal(catalogIncomplete([row(false), row(true)]), false);
});

test("an incomplete list is read again at 10 s, 30 s, then 60 s, and then left to a reconnect or a Refresh; a complete one is left alone", () => {
  assert.deepEqual([...CATALOG_RETRY_MS], [10_000, 30_000, 60_000]);
  assert.deepEqual([0, 1, 2, 3, 9].map((n) => nextCatalogRead(null, n)), [10_000, 30_000, 60_000, null, null]);
  assert.deepEqual([0, 1].map((n) => nextCatalogRead([row(false)], n)), [10_000, 30_000]);
  assert.equal(nextCatalogRead([row(true)], 0), null);
});

test("the list store reads again when the bus comes back, on the model's backoff, and when Refresh is pressed", () => {
  const store = readFileSync(new URL("./useHarnesses.ts", import.meta.url), "utf8");
  assert.ok(store.includes("reloadOnReconnect(watchConnection, "), "a page that opened while the node was down reads the list when it comes up");
  assert.ok(store.includes("nextCatalogRead(cache, attempts)"), "the backoff is the model's");
  assert.ok(store.includes("export function reloadHarnesses()"), "Refresh re-reads which harnesses are installed");
  const line = readFileSync(new URL("./HarnessUsageLine.tsx", import.meta.url), "utf8");
  const picker = readFileSync(new URL("./UsagePicker.tsx", import.meta.url), "utf8");
  for (const [name, src] of [["RefreshUsage", line], ["UsagePicker", picker]]) {
    assert.ok(src.indexOf("reloadHarnesses();") < src.indexOf("refreshHarnessUsage("), `${name}: the list first, then the usage`);
  }
});
