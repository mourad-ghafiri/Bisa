/**
 * Update from the You menu: the item stands before *About Bisa*, the dialog
 * reads the node's one route and opens every door through `openExternal`,
 * the desktop's tag rule is the release scripts' own, and the node answers
 * facts without a comparison of its own.
 *
 * Run with `node --test --import ./src/i18n/preload.mjs src/scenarios/update.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { tagFor as desktopTagFor } from "../shell/updateModel.mjs";
import { tagFor as releaseTagFor } from "../../../scripts/release/releaseModel.mjs";

const src = join(dirname(fileURLToPath(import.meta.url)), "..");
const repo = join(src, "..", "..");
const read = (rel) => readFileSync(join(src, rel), "utf8");
const readRepo = (rel) => readFileSync(join(repo, rel), "utf8");

test("the You menu lists Update before About Bisa, past the separator that parts the person's items from the app's", () => {
  const menu = read("shell/ProfileMenu.tsx");
  const update = menu.indexOf('t("shell-profile-menu-update")');
  const about = menu.indexOf('t("shell-profile-menu-about-bisa")');
  const settings = menu.indexOf('t("shell-keymap-settings")');
  assert.ok(settings >= 0 && update > settings && about > update, "Identity · Settings · Update · About Bisa");
  assert.match(menu.slice(update, about), /separatorBefore: true/, "the separator opens the app's two");
  assert.doesNotMatch(menu.slice(about, about + 160), /separatorBefore/, "About no longer parts itself from Update");
  assert.match(menu, /<UpdateDialog open=\{update\} onClose=/);
});

test("the dialog reads GET /updates, opens every door through openExternal, and renders the notes through the kit's Markdown", () => {
  const dialog = read("shell/UpdateDialog.tsx");
  assert.match(dialog, /api\.updateCheck\(again > 0, s\)/, "one read, refreshed on Check again");
  assert.match(dialog, /releaseLinks\(state, __APP_REPOSITORY__\)/, "the doors come from the model and the baked-in repository");
  assert.match(dialog, /onClick=\{\(\) => void openExternal\(l\.url\)\}/, "a door is a button through openExternal");
  assert.doesNotMatch(dialog, /<a /, "never a link that could navigate the window");
  assert.doesNotMatch(dialog, /target="_blank"/);
  assert.match(dialog, /<Markdown text=\{state\.notes\} \/>/, "the notes render through the kit, sanitised");
  assert.match(dialog, /variant=\{l\.id === "release" \? "primary" : "ghost"\}/, "the one primary is the release page");
  assert.match(dialog, /<ErrorNote error=\{words\.line\} retry=\{checkAgain\} \/>/, "a failure is a note with Retry");
  assert.doesNotMatch(dialog, /\bnavigator\b|\bwindow\./, "nothing read off the window");
});

test("the desktop's tag rule is the release scripts' rule, and the changelog door names CHANGELOG.md at the tag", () => {
  assert.equal(desktopTagFor("0.3.0"), releaseTagFor("0.3.0"));
  const model = read("shell/updateModel.mjs");
  assert.match(model, /\/blob\/\$\{tag\}\/CHANGELOG\.md/);
  assert.ok(readRepo("CHANGELOG.md").startsWith("# Changelog"));
});

test("the node answers facts: the route is documented, the engine compares no versions, and the type names its states", () => {
  const docs = readRepo("crates/bisa-node/src/route_docs.rs");
  assert.match(docs, /path: "\/updates"/);
  const engine = readRepo("crates/bisa-engine/src/updates.rs");
  assert.doesNotMatch(engine, /fn\s+\w*(compare|newer|older)\w*\s*\(|semver::|\.cmp\(|PartialOrd/, "the comparison is the desktop's (`updateModel.mjs`)");
  assert.match(engine, /releases\/latest/, "GitHub's own *latest* — never a draft, never a prerelease");
  const types = read("types.gen.ts");
  for (const state of ['state: "latest"', 'state: "no_release"', 'state: "off"', 'state: "failed"']) assert.ok(types.includes(state), state);
  for (const kind of ['kind: "unreachable"', 'kind: "rate_limited"', 'kind: "unexpected"']) assert.ok(types.includes(kind), kind);
});
