import test from "node:test";
import assert from "node:assert/strict";
import { emptyQueue, head, onFrame, remaining, remove, seed, skip } from "./committerPromptModel.mjs";

const needed = (project, extra = {}) => ({
  type: "committer_needed",
  project,
  slug: `p-${project}`,
  workstream: project,
  reason: "created",
  origin: { origin: "workspace" },
  ...extra,
});

test("an ask is queued once per project; a repeat refreshes its reason and keeps the offered pair", () => {
  let q = onFrame(emptyQueue(), needed("A", { global: { name: "G", email: "g@x.y" } }));
  q = onFrame(q, needed("A", { reason: "settlement_refused", global: null }));
  assert.equal(q.asks.length, 1);
  assert.equal(head(q).reason, "settlement_refused");
  assert.deepEqual(head(q).global, { name: "G", email: "g@x.y" }, "a later frame without the pair does not lose it");
  q = onFrame(q, needed("B"));
  assert.equal(q.asks.length, 2);
  assert.equal(head(q).project, "A", "oldest first");
  assert.equal(remaining(q), 1);
});

test("the answer removes the ask, whichever door it came through", () => {
  let q = onFrame(onFrame(emptyQueue(), needed("A")), needed("B"));
  q = onFrame(q, { type: "committer_set", project: "A", workstream: "A", identity: { name: "Ada", email: "a@b.c" } });
  assert.equal(head(q).project, "B");
  q = remove(q, "B");
  assert.equal(head(q), null);
  assert.equal(remaining(q), 0);
  assert.equal(remove(q, "nobody"), q, "removing what is not there changes nothing");
});

test("not now skips a project for this session only: a later ask for it is dropped, a fresh queue is not", () => {
  let q = onFrame(emptyQueue(), needed("A"));
  q = skip(q, "A");
  assert.equal(head(q), null);
  q = onFrame(q, needed("A", { reason: "commit_refused" }));
  assert.equal(head(q), null, "asked again in the same session: still skipped");
  assert.deepEqual(q.skipped, ["A"]);
  assert.equal(head(onFrame(emptyQueue(), needed("A"))).project, "A", "a new session asks again");
  // Skipping twice does not grow the list.
  assert.deepEqual(skip(q, "A").skipped, ["A"]);
});

test("seeding from the overview fills the queue with what the engine was already asking", () => {
  const pending = [
    { project: "A", slug: "web", workstream: "A", reason: "created" },
    { project: "B", slug: "api", workstream: "B", reason: "settlement_refused" },
  ];
  const q = seed(emptyQueue(), pending, { name: "G", email: "g@x.y" });
  assert.deepEqual(
    q.asks.map((a) => [a.project, a.slug, a.reason, a.global?.name ?? null, a.origin]),
    [
      ["A", "web", "created", "G", null],
      ["B", "api", "settlement_refused", "G", null],
    ],
  );
  assert.equal(seed(q, pending).asks.length, 2, "seeding again is idempotent");
  assert.equal(seed(emptyQueue(), null).asks.length, 0);
  assert.equal(seed(skip(emptyQueue(), "A"), pending).asks.length, 1, "a skipped project is not re-seeded");
});

test("frames about other things leave the queue alone", () => {
  const q = onFrame(emptyQueue(), needed("A"));
  assert.equal(onFrame(q, { type: "project_created", project: "Z", slug: "z", origin: { origin: "workspace" } }), q);
  assert.equal(onFrame(q, null), q);
});

test("a seeded row keeps the origin and the global pair it carries, and takes the overview's pair only without one", () => {
  const pending = [
    { project: "A", slug: "web", workstream: "A", reason: "unresolved", origin: { origin: "goal", goal: "g" }, global: { name: "Ada", email: "ada@x.y" } },
    { project: "B", slug: "api", workstream: "B", reason: "created", origin: { origin: "workspace" }, global: null },
  ];
  const q = seed(emptyQueue(), pending, { name: "G", email: "g@x.y" });
  assert.deepEqual(q.asks[0].origin, { origin: "goal", goal: "g" });
  assert.equal(q.asks[0].global.name, "Ada", "its own pair wins");
  assert.equal(q.asks[1].global.name, "G", "the overview's pair fills a row without one");
  assert.equal(q.asks[0].reason, "unresolved");
});
