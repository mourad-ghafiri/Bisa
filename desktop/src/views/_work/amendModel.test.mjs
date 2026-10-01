/**
 * Amend's words and the switch's effect on the draft. Run with
 * `node --test desktop/src/views/_work/amendModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { amendDraft, amendPlaceholder, amendWords, headMessage } from "./amendModel.mjs";

test("the confirmation names the commit, says what folds in, and warns only when HEAD is already on its upstream", () => {
  const local = amendWords({ short: "abc1234", subject: "fix the header", staged: 2, upstream: "origin/topic", ahead: 1 });
  assert.equal(local.title, "Amend abc1234 — fix the header?");
  assert.match(local.body, /2 files — folds into it/);
  assert.equal(local.warning, null, "one commit ahead: nothing published is rewritten");
  assert.equal(local.danger, false);
  assert.equal(local.confirm, "Amend");
  assert.equal(local.kind, "commit");
  const reword = amendWords({ short: "abc1234", subject: "fix the header", staged: 0, upstream: null, ahead: 0 });
  assert.equal(reword.body, "Only the message changes; the commit's id changes.");
  assert.equal(reword.warning, null, "no upstream: nothing is published");
  const pushed = amendWords({ short: "abc1234", subject: "fix the header", staged: 1, upstream: "origin/topic", ahead: 0 });
  assert.match(pushed.warning, /abc1234 is already on origin\/topic/);
  assert.match(pushed.warning, /Force push with lease…/);
  assert.equal(pushed.danger, true, "rewriting published history wears the danger tone");
  assert.match(pushed.body, /1 file — folds/);
  assert.equal(amendWords({ short: "abc1234", staged: 0, ahead: 0 }).title, "Amend abc1234?", "no subject read yet: the id alone");
});

test("the switch fills an empty box with HEAD's message and empties a box that still holds it, and never touches what was typed", () => {
  const head = "fix the header\n\nand the footer";
  assert.equal(amendDraft(true, "", head), head);
  assert.equal(amendDraft(true, "   ", head), head, "blank is empty");
  assert.equal(amendDraft(true, "my words", head), "my words");
  assert.equal(amendDraft(false, head, head), "", "off again: the borrowed message goes");
  assert.equal(amendDraft(false, "my words", head), "my words");
  assert.equal(amendDraft(false, "fix the header, edited", head), "fix the header, edited", "an edited message is the person's");
  assert.equal(amendDraft(true, "", null), "", "HEAD not read yet: nothing to fill with");
});

test("HEAD's message and the placeholder read as the box shows them", () => {
  assert.equal(headMessage("subject", "body line"), "subject\n\nbody line");
  assert.equal(headMessage("subject", ""), "subject");
  assert.equal(headMessage(" subject ", null), "subject");
  assert.equal(amendPlaceholder("abc1234", "fix the header"), "Amend abc1234 — fix the header");
  assert.equal(amendPlaceholder("abc1234", null), "Amend abc1234");
});
