/**
 * Suggesting and resolving are two questions over one directory.
 *
 * The trap this file exists to nail down: hiding an entry from the picker by
 * removing it from the directory also removes it from resolution, and the
 * result is a message that posts with no mentions, wakes nobody, and reports
 * nothing wrong. There is no assertion a component test could make about that
 * — the symptom is silence — so it is asserted here instead.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import {
  SUGGESTION_LIMIT,
  escapeRe,
  insertMention,
  isSuggestable,
  mentionQuery,
  mentionsIn,
  suggestions,
} from "./mentionModel.mjs";

const CORE = { id: "core-pubkey", name: "General Agent", kind: "agent", suggest: false };
const ADA = { id: "ada-pubkey", name: "Ada", kind: "agent" };
const CHANNEL = { id: "01CHANNEL", name: "engineering", kind: "channel" };
const DIRECTORY = [CHANNEL, ADA, CORE];

test("a hidden entry is offered by no query, including one that names it exactly", () => {
  assert.deepEqual(suggestions(DIRECTORY, ""), [CHANNEL, ADA]);
  assert.deepEqual(suggestions(DIRECTORY, "bisa"), []);
  assert.deepEqual(suggestions(DIRECTORY, "core-pubkey"), []);
});

test("a hidden entry still resolves when its name is typed", () => {
  assert.deepEqual(mentionsIn(DIRECTORY, "@General Agent can you look?"), ["core-pubkey"]);
});

test("an entry with no suggest flag is offered, because absent means yes", () => {
  assert.equal(isSuggestable(ADA), true);
  assert.equal(isSuggestable(CORE), false);
  assert.deepEqual(suggestions(DIRECTORY, "ada"), [ADA]);
});

test("a body naming nobody resolves to nobody — that is what triage keys on", () => {
  assert.deepEqual(mentionsIn(DIRECTORY, "just thinking out loud"), []);
  // An address-like fragment that matches no name is still nobody.
  assert.deepEqual(mentionsIn(DIRECTORY, "@nobody at all"), []);
});

test("a name that is a regex is matched literally rather than throwing", () => {
  const awkward = [
    { id: "cpp", name: "C++ helper", kind: "agent" },
    { id: "ops", name: "Ops (EU)", kind: "agent" },
    { id: "anon", name: "ab12cd34…", kind: "human" },
  ];
  assert.deepEqual(mentionsIn(awkward, "@C++ helper please"), ["cpp"]);
  assert.deepEqual(mentionsIn(awkward, "ping @Ops (EU)"), ["ops"]);
  // `\b` never matches after "…", which is why the guard is a lookahead.
  assert.deepEqual(mentionsIn(awkward, "@ab12cd34… hello"), ["anon"]);
  assert.equal(escapeRe("C++ (x)"), "C\\+\\+ \\(x\\)");
});

test("a name that is a prefix of another is not matched by the longer one", () => {
  const pair = [
    { id: "a", name: "Ada", kind: "agent" },
    { id: "b", name: "Adalind", kind: "agent" },
  ];
  assert.deepEqual(mentionsIn(pair, "@Adalind hello"), ["b"]);
  assert.deepEqual(mentionsIn(pair, "@Ada hello"), ["a"]);
});

test("the same name twice is one mention, and the picker stays a screenful", () => {
  assert.deepEqual(mentionsIn(DIRECTORY, "@Ada and also @Ada"), ["ada-pubkey"]);
  const many = Array.from({ length: 20 }, (_, i) => ({ id: `a${i}`, name: `Agent ${i}` }));
  assert.equal(suggestions(many, "agent").length, SUGGESTION_LIMIT);
});

test("an absent directory is empty rather than a crash", () => {
  assert.deepEqual(suggestions(undefined, "x"), []);
  assert.deepEqual(mentionsIn(null, "@Ada"), []);
  assert.deepEqual(mentionsIn(DIRECTORY, undefined), []);
});

const LOVELACE = { id: "ada-pubkey", name: "Ada Lovelace", kind: "agent" };
const TWO_WORD = [LOVELACE, CORE];

test("a two-word name does not close the picker halfway through typing it", () => {
  // The defect: the run after `@` was cut at the first space, so the dropdown
  // shut on the space in "Ada Lovelace" — the one name the message was about
  // to address, and one `mentionsIn` resolves perfectly happily.
  const text = "@Ada L";
  assert.deepEqual(mentionQuery(text, text.length, TWO_WORD), {
    at: 0,
    end: 6,
    query: "Ada L",
  });
  assert.deepEqual(suggestions(TWO_WORD, "Ada L"), [LOVELACE]);
  assert.deepEqual(mentionsIn(TWO_WORD, "@Ada Lovelace hello"), ["ada-pubkey"]);
});

test("a run with a space closes as soon as no name could still complete it", () => {
  // Otherwise every `@` in a paragraph holds the dropdown open to the end of
  // the line.
  const text = "@Ada is right about";
  assert.equal(mentionQuery(text, text.length, TWO_WORD), null);
  // A completed name plus its trailing space is finished, not still a query.
  assert.equal(mentionQuery("@Ada Lovelace ", 14, TWO_WORD), null);
});

test("a hidden entry cannot hold the picker open on a name it will not offer", () => {
  const text = "@Bisa A";
  assert.equal(mentionQuery(text, text.length, TWO_WORD), null);
});

test("the picker opens at a bare @ and at the start of a word, never mid-word", () => {
  assert.deepEqual(mentionQuery("@", 1, TWO_WORD), { at: 0, end: 1, query: "" });
  assert.deepEqual(mentionQuery("hi @a", 5, TWO_WORD), { at: 3, end: 5, query: "a" });
  // An email address is not the start of a mention.
  assert.equal(mentionQuery("ada@example.com", 15, TWO_WORD), null);
  assert.equal(mentionQuery("nothing here", 12, TWO_WORD), null);
});

test("the query is what sits between the @ and the caret, not the rest of the line", () => {
  const text = "@Ada and then some";
  assert.deepEqual(mentionQuery(text, 4, TWO_WORD), { at: 0, end: 4, query: "Ada" });
});

test("inserting a mention keeps what was after the caret", () => {
  // The defect: the insertion rebuilt the text as everything-before-the-@ plus
  // the name, so completing a mention you had gone back to add ate the rest
  // of the sentence.
  const text = "@Ada L, what do you think?";
  const span = mentionQuery(text, 6, TWO_WORD);
  assert.deepEqual(insertMention(text, span, "Ada Lovelace"), {
    text: "@Ada Lovelace , what do you think?",
    caret: 14,
  });
});

test("inserting mid-sentence leaves the caret after the name it inserted", () => {
  const text = "please ask @ad about it";
  const span = mentionQuery(text, 14, TWO_WORD);
  const next = insertMention(text, span, "Ada Lovelace");
  assert.equal(next.text, "please ask @Ada Lovelace  about it");
  assert.equal(next.text.slice(0, next.caret), "please ask @Ada Lovelace ");
});
