import { strict as assert } from "node:assert";
import { test } from "node:test";
import { GROUP_WINDOW_SECONDS, contentOf, isCompact, threads } from "./timelineModel.mjs";
import { has } from "../../i18n/l10n.mjs";

const at = (id, reply_to = null) => ({ id, reply_to });

test("a platform-authored post is said in the reader's language from its `said` message; a person's or an agent's words, and a message this catalog lacks, are the content", () => {
  // The engine's catalog is the window's too (`i18n/catalog.ts`, `testing.mjs`): a `said` from `engine.ftl` renders here.
  assert.ok(has("guided-say-cycle-failed") && has("engine-conversation-reply-cut"), "the crates' said messages are in the installed catalog");
  const said = { content: "I could not finish this cycle. Ask for the design again.", said: { id: "guided-say-cycle-failed" } };
  assert.equal(contentOf(said), "I could not finish this cycle. Ask for the design again.");
  const withArgs = { content: "english", said: { id: "guided-say-proposed-design-adopted", args: { name: "Ship it", steps: "- build" } } };
  assert.equal(contentOf(withArgs), "I designed **Ship it** and started it.\n- build", "the arguments ride into the sentence");
  assert.equal(contentOf({ content: "hello", said: null }), "hello", "a person's post");
  assert.equal(contentOf({ content: "hello" }), "hello", "no said at all");
  assert.equal(contentOf({ content: "the English kept", said: { id: "engine-nobody-wrote-this" } }), "the English kept", "a message this catalog lacks falls back to the content, never to an id");
  assert.equal(contentOf({ content: "x", said: { id: 7 } }), "x", "a said with no id is no said");
  assert.equal(contentOf(null), "");
});

test("threads: roots in time order, replies under them one level deep, an orphan reply a root of its own", () => {
  const page = [at("a"), at("b", "a"), at("c"), at("d", "b"), at("e", "gone"), at("f", "a")];
  const out = threads(page);
  assert.deepEqual(
    out.map((t) => [t.root.id, t.replies.map((r) => r.id)]),
    [
      ["a", ["b", "f"]],
      ["c", []],
      ["d", []],
      ["e", []],
    ],
    "a reply to a reply is not nested; a reply whose parent is off the page is shown, not hidden",
  );
  assert.deepEqual(threads([]), []);
  assert.deepEqual(threads(null), []);
});

test("a message is compact under the same author within the window, on the same day, after a standing message, and never at the first unread", () => {
  const day = 1_700_000_000;
  const prev = { author: "ada", created_at: day, retracted: false };
  assert.ok(isCompact({ author: "ada", created_at: day + 10, id: "m2" }, prev, null));
  assert.ok(!isCompact({ author: "ada", created_at: day + 10, id: "m2" }, undefined, null), "nothing before it");
  assert.ok(!isCompact({ author: "bob", created_at: day + 10, id: "m2" }, prev, null), "another author");
  assert.ok(!isCompact({ author: "ada", created_at: day + GROUP_WINDOW_SECONDS, id: "m2" }, prev, null), "at the window's edge, a new turn");
  assert.ok(isCompact({ author: "ada", created_at: day + GROUP_WINDOW_SECONDS - 1, id: "m2" }, prev, null));
  assert.ok(!isCompact({ author: "ada", created_at: day + 10, id: "m2" }, { ...prev, retracted: true }, null), "a retracted message heads nothing");
  assert.ok(!isCompact({ author: "ada", created_at: day + 10, id: "m2" }, prev, "m2"), "the first unread stands alone");
  assert.ok(!isCompact({ author: "ada", created_at: day + 86_400, id: "m2" }, prev, null), "another day");
});
