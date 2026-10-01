/**
 * `reactionModel.mjs` — folding a message's reactions.
 *
 * Run with `npm test` from `desktop/`.
 */

import test from "node:test";
import assert from "node:assert/strict";

import { QUICK_EMOJI, TAKEN_MARK, groupReactions } from "./reactionModel.mjs";

const OWNER = "owner-pubkey";
const DEVELOPER = "developer-pubkey";
const MSG = "msg-1";

const reaction = (over = {}) => ({
  id: "r1",
  target_id: MSG,
  author: OWNER,
  emoji: "👍",
  retracted: false,
  ...over,
});

test("a group names every author, not just how many there were", () => {
  const [group] = groupReactions(
    [
      reaction({ id: "r1", author: DEVELOPER, emoji: TAKEN_MARK }),
      reaction({ id: "r2", author: OWNER, emoji: TAKEN_MARK }),
    ],
    MSG,
    OWNER,
  );
  assert.equal(group.emoji, TAKEN_MARK);
  assert.equal(group.count, 2);
  assert.deepEqual(group.authors, [DEVELOPER, OWNER], "in the order they reacted");
});

test("an agent's mark and your own on the same emoji are two, and only yours is retractable", () => {
  // The case the engine creates: the Developer marks your message, and you
  // react with the same emoji. Clicking the pill must add or remove *yours*
  // and never touch the agent's — which is what `mine` carrying only your own
  // event id enforces.
  const [group] = groupReactions(
    [
      reaction({ id: "agent", author: DEVELOPER, emoji: TAKEN_MARK }),
      reaction({ id: "yours", author: OWNER, emoji: TAKEN_MARK }),
    ],
    MSG,
    OWNER,
  );
  assert.equal(group.count, 2);
  assert.equal(group.mine, "yours");
});

test("a mark drawn only by an agent is not yours to undo", () => {
  const [group] = groupReactions(
    [reaction({ id: "agent", author: DEVELOPER, emoji: TAKEN_MARK })],
    MSG,
    OWNER,
  );
  assert.equal(group.mine, undefined, "clicking adds your own rather than removing theirs");
  assert.deepEqual(group.authors, [DEVELOPER]);
});

test("retracted reactions and other messages' reactions are not this message's", () => {
  const groups = groupReactions(
    [
      reaction({ id: "r1", retracted: true }),
      reaction({ id: "r2", target_id: "another-message" }),
    ],
    MSG,
    OWNER,
  );
  assert.deepEqual(groups, [], "a reaction nobody stands behind is not a fact");
});

test("one group per emoji, in first-reacted order", () => {
  const groups = groupReactions(
    [
      reaction({ id: "r1", emoji: "🎉" }),
      reaction({ id: "r2", author: DEVELOPER, emoji: TAKEN_MARK }),
      reaction({ id: "r3", author: DEVELOPER, emoji: "🎉" }),
    ],
    MSG,
    OWNER,
  );
  assert.deepEqual(
    groups.map((g) => [g.emoji, g.count]),
    [
      ["🎉", 2],
      [TAKEN_MARK, 1],
    ],
  );
});

test("the mark an agent draws is one the picker already offers", () => {
  // So it renders through the pill the timeline already draws, and a person
  // can add or remove their own of the same kind.
  assert.ok(QUICK_EMOJI.includes(TAKEN_MARK));
});
