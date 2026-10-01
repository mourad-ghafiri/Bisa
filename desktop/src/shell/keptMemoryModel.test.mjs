/**
 * A memory by place and name: bounded, versioned, one workspace's, quiet,
 * written on a beat — and never the reason a screen fails.
 * Run with `node --test desktop/src/shell/keptMemoryModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { KeptMemory, WRITE_AT_MOST_MS, WRITE_BEAT_MS, settleLeft } from "./keptMemoryModel.mjs";

/** A storage in memory, with a switch to refuse every call and a count of the writes it took. */
function storage(refuse = false) {
  const map = new Map();
  const s = {
    map,
    writes: 0,
    refuse,
    getItem: (k) => {
      if (s.refuse) throw new Error("denied");
      return map.has(k) ? map.get(k) : null;
    },
    setItem: (k, v) => {
      if (s.refuse) throw new Error("denied");
      s.writes += 1;
      map.set(k, String(v));
    },
    removeItem: (k) => {
      if (s.refuse) throw new Error("denied");
      s.writes += 1;
      map.delete(k);
    },
  };
  return s;
}

/** A clock a test moves, and the timers set against it. */
function clock() {
  const c = {
    at: 1_000,
    timers: new Map(),
    next: 1,
    now: () => c.at,
    later: (fn, ms) => {
      const id = c.next++;
      c.timers.set(id, { fn, due: c.at + ms });
      return id;
    },
    cancel: (id) => void c.timers.delete(id),
    /** Move the clock on, running what came due, in order. */
    pass: (ms) => {
      const until = c.at + ms;
      for (;;) {
        const due = [...c.timers].filter(([, t]) => t.due <= until).sort((a, b) => a[1].due - b[1].due)[0];
        if (!due) break;
        c.timers.delete(due[0]);
        c.at = due[1].due;
        due[1].fn();
      }
      c.at = until;
    },
  };
  return c;
}

const CAPS = { places: 3, valueBytes: 64, totalBytes: 400 };

function memory({ store = storage(), time = clock(), caps = CAPS, version = 1 } = {}) {
  const kept = new KeptMemory({ key: "bisa.view.test", version, caps, hands: { storage: () => store, now: time.now, later: time.later, cancel: time.cancel } });
  return { kept, store, time };
}

test("a value kept is read back by identity until it changes, and a name never kept is nothing", () => {
  const { kept } = memory();
  const rows = ["a", "b"];
  assert.equal(kept.keep("/goals", "opened", rows), true);
  assert.equal(kept.read("/goals", "opened"), rows, "the very array, so a snapshot is stable");
  assert.equal(kept.read("/goals", "opened"), kept.read("/goals", "opened"));
  assert.equal(kept.read("/goals", "other"), undefined);
  assert.equal(kept.read("/nowhere", "opened"), undefined);
  assert.equal(kept.knows("/goals"), true);
  assert.equal(kept.knows("/nowhere"), false);
});

test("the same value keeps nothing and tells nobody; another value tells the listeners of its own place and name only", () => {
  const { kept } = memory();
  const heard = [];
  const stop = kept.subscribe("/goals", "q", () => heard.push("goals.q"));
  kept.subscribe("/goals", "tags", () => heard.push("goals.tags"));
  kept.subscribe("/teams", "q", () => heard.push("teams.q"));
  assert.equal(kept.keep("/goals", "q", "ship"), true);
  assert.deepEqual(heard, ["goals.q"]);
  assert.equal(kept.keep("/goals", "q", "ship"), false, "the same text");
  assert.equal(kept.keep("/goals", "opened", ["a"]), true);
  assert.equal(kept.keep("/goals", "opened", ["a"]), false, "another array of the same text is the same value");
  assert.deepEqual(heard, ["goals.q"]);
  kept.keep("/goals", "q", "ships");
  assert.deepEqual(heard, ["goals.q", "goals.q"]);
  stop();
  kept.keep("/goals", "q", "shipped");
  assert.deepEqual(heard, ["goals.q", "goals.q"], "a listener that stopped hears nothing");
});

test("a scroll kept tells nobody, and is kept all the same", () => {
  const { kept } = memory();
  let heard = 0;
  kept.subscribe("/goals", "scroll:list", () => (heard += 1));
  assert.equal(kept.keepQuietly("/goals", "scroll:list", { top: 120, left: 0 }), true);
  assert.equal(heard, 0);
  assert.deepEqual(kept.read("/goals", "scroll:list"), { top: 120, left: 0 });
  assert.equal(kept.keepQuietly("/goals", "scroll:list", { top: 120, left: 0 }), false);
});

test("null and undefined forget a name, and a place with nothing left is no place", () => {
  const { kept } = memory();
  kept.keep("/goals", "q", "ship");
  kept.keep("/goals", "tags", ["ops"]);
  assert.equal(kept.keep("/goals", "q", null), true);
  assert.equal(kept.read("/goals", "q"), undefined);
  assert.equal(kept.knows("/goals"), true);
  assert.equal(kept.keep("/goals", "tags", undefined), true);
  assert.equal(kept.knows("/goals"), false);
  assert.equal(kept.keep("/goals", "tags", null), false, "nothing to forget");
});

test("the oldest place goes past the cap, and a place written to is the newest", () => {
  const { kept } = memory();
  let gone = 0;
  kept.subscribe("/b", "q", () => (gone += 1));
  kept.keep("/a", "q", "1");
  kept.keep("/b", "q", "2");
  kept.keep("/c", "q", "3");
  kept.keep("/a", "q", "again");
  kept.keep("/d", "q", "4");
  assert.deepEqual(kept.keptPlaces(), ["/c", "/a", "/d"], "`/b` was the oldest once `/a` was written to");
  assert.equal(kept.read("/b", "q"), undefined);
  assert.equal(gone, 2, "its listener heard it kept and heard it go");
});

test("a read moves nothing: only a write makes a place the newest", () => {
  const { kept } = memory();
  kept.keep("/a", "q", "1");
  kept.keep("/b", "q", "2");
  kept.read("/a", "q");
  assert.deepEqual(kept.keptPlaces(), ["/a", "/b"]);
});

test("writes wait the beat, the beat starts over with each change, and the longest wait ends it", () => {
  const { kept, store, time } = memory();
  kept.keep("/goals", "q", "s");
  assert.equal(kept.waiting(), true);
  time.pass(WRITE_BEAT_MS - 1);
  assert.equal(store.writes, 0, "still inside the beat");
  kept.keep("/goals", "q", "sh");
  time.pass(WRITE_BEAT_MS - 1);
  assert.equal(store.writes, 0, "the beat started over");
  time.pass(1);
  assert.equal(store.writes, 1);
  assert.equal(kept.waiting(), false);
  assert.deepEqual(JSON.parse(store.map.get("bisa.view.test")), { v: 1, owner: null, places: [["/goals", { q: "sh" }]] });

  // A person who never stops typing is written at the longest wait.
  let typed = "";
  const before = store.writes;
  for (let i = 0; i < 20; i++) {
    typed += "x";
    kept.keep("/goals", "q", typed);
    time.pass(WRITE_BEAT_MS / 2);
  }
  assert.ok(store.writes > before, "written before the typing ended");
  assert.ok(WRITE_AT_MOST_MS > WRITE_BEAT_MS);
});

test("a flush writes at once, the same text is no write, and nothing waiting writes nothing", () => {
  const { kept, store } = memory();
  assert.equal(kept.flush(), false, "nothing waiting");
  kept.keep("/goals", "q", "ship");
  assert.equal(kept.flush(), true);
  assert.equal(store.writes, 1);
  assert.ok(kept.writtenAt > 0);
  kept.keep("/goals", "q", "shipped");
  kept.keep("/goals", "q", "ship");
  assert.equal(kept.flush(), false, "back to what the storage holds");
  assert.equal(store.writes, 1);
});

test("a value over its bytes lives for the window and is not written", () => {
  const { kept, store } = memory();
  const long = "x".repeat(CAPS.valueBytes + 1);
  kept.keep("/goals", "q", long);
  kept.keep("/goals", "tags", ["ops"]);
  assert.equal(kept.read("/goals", "q"), long, "kept for the window");
  kept.flush();
  assert.deepEqual(JSON.parse(store.map.get("bisa.view.test")).places, [["/goals", { tags: ["ops"] }]]);
});

test("what is written stops at the total, the newest places first", () => {
  const { kept, store } = memory({ caps: { places: 10, valueBytes: 64, totalBytes: 60 } });
  kept.keep("/old", "q", "o".repeat(30));
  kept.keep("/mid", "q", "m".repeat(30));
  kept.keep("/new", "q", "n".repeat(30));
  kept.flush();
  assert.deepEqual(
    JSON.parse(store.map.get("bisa.view.test")).places.map(([place]) => place),
    ["/new"],
    "one place fits; it is the newest",
  );
  assert.equal(kept.read("/old", "q"), "o".repeat(30), "what did not fit is still the window's");
});

test("what was written is read back by the next window, in its order", () => {
  const store = storage();
  const first = memory({ store });
  first.kept.keep("/a", "q", "1");
  first.kept.keep("/b", "opened", ["x", "y"]);
  first.kept.keepQuietly("/b", "scroll:list", { top: 40, left: 0 });
  first.kept.flush();
  const next = memory({ store });
  assert.equal(next.kept.read("/a", "q"), "1");
  assert.deepEqual(next.kept.read("/b", "opened"), ["x", "y"]);
  assert.deepEqual(next.kept.read("/b", "scroll:list"), { top: 40, left: 0 });
  assert.deepEqual(next.kept.keptPlaces(), ["/a", "/b"]);
  assert.equal(next.kept.read("/b", "opened"), next.kept.read("/b", "opened"), "parsed once, read back by identity");
});

test("another version is refused, and so is text that is not this shape", () => {
  const store = storage();
  const first = memory({ store });
  first.kept.keep("/a", "q", "1");
  first.kept.flush();
  assert.equal(memory({ store, version: 2 }).kept.read("/a", "q"), undefined, "another version starts empty");
  for (const text of ["{not json", "[]", '"words"', '{"v":1}', '{"v":1,"places":{}}', '{"v":1,"places":[["/a"]]}', '{"v":1,"places":[[1,{}]]}', '{"v":1,"places":[["/a",[1]]]}']) {
    store.map.set("bisa.view.test", text);
    assert.deepEqual(memory({ store }).kept.keptPlaces(), [], text);
  }
});

test("more places stored than the cap keeps the newest", () => {
  const store = storage();
  store.map.set("bisa.view.test", JSON.stringify({ v: 1, owner: null, places: [["/a", { q: "1" }], ["/b", { q: "2" }], ["/c", { q: "3" }], ["/d", { q: "4" }]] }));
  assert.deepEqual(memory({ store }).kept.keptPlaces(), ["/b", "/c", "/d"]);
});

test("a storage that refuses keeps the memory for the window and throws nothing", () => {
  const store = storage(true);
  const { kept } = memory({ store });
  assert.equal(kept.keep("/goals", "q", "ship"), true);
  assert.equal(kept.flush(), false, "not taken");
  assert.equal(kept.read("/goals", "q"), "ship");
  assert.equal(kept.writtenAt, 0);
  // No storage at all is the same.
  const none = new KeptMemory({ key: "k", version: 1, caps: CAPS, hands: { storage: () => null, now: () => 1, later: () => 0, cancel: () => {} } });
  none.keep("/goals", "q", "ship");
  assert.equal(none.flush(), false);
  assert.equal(none.read("/goals", "q"), "ship");
});

test("a place is forgotten whole, and so is everything under a prefix", () => {
  const { kept } = memory({ caps: { places: 10, valueBytes: 64, totalBytes: 4000 } });
  let heard = 0;
  kept.subscribe("/goals/g1", "tab", () => (heard += 1));
  kept.keep("/goals/g1", "tab", "workflow");
  kept.keep("/goals/g2", "tab", "progress");
  kept.keep("/hosts/h1/channels/c1", "q", "1");
  kept.keep("/hosts/h1/messages/m1", "q", "2");
  kept.keep("/hosts/h2/channels/c1", "q", "3");
  assert.equal(kept.forget("/goals/g1"), true);
  assert.equal(kept.forget("/goals/g1"), false);
  assert.equal(heard, 2, "kept, then forgotten");
  assert.equal(kept.forgetUnder("/hosts/h1/"), 2);
  assert.deepEqual(kept.keptPlaces(), ["/goals/g2", "/hosts/h2/channels/c1"]);
  assert.equal(kept.forgetUnder("/nothing"), 0);
});

test("a place moved carries what it held", () => {
  const { kept } = memory();
  const view = { line: 12 };
  kept.keep("root|file:a.rs", "editor", view);
  assert.equal(kept.move("root|file:a.rs", "root|file:b.rs"), true);
  assert.equal(kept.read("root|file:b.rs", "editor"), view);
  assert.equal(kept.read("root|file:a.rs", "editor"), undefined);
  assert.equal(kept.move("root|file:gone.rs", "root|file:c.rs"), false, "nothing kept moves nothing");
  assert.equal(kept.move("root|file:b.rs", "root|file:b.rs"), false);
});

test("clear forgets everything and writes that at once", () => {
  const { kept, store } = memory();
  let heard = 0;
  kept.subscribe("/goals", "q", () => (heard += 1));
  kept.keep("/goals", "q", "ship");
  kept.flush();
  assert.ok(store.map.has("bisa.view.test"));
  kept.clear();
  assert.deepEqual(kept.keptPlaces(), []);
  assert.equal(heard, 2);
  assert.equal(store.map.has("bisa.view.test"), false, "a memory with nothing and nobody's is no key");
  assert.equal(kept.waiting(), false);
});

test("a memory sealed keeps nothing more and writes nothing more: what was forgotten cannot be handed back on the way out", () => {
  const { kept, store, time } = memory();
  let heard = 0;
  kept.subscribe("/settings", "tab", () => (heard += 1));
  kept.keep("/goals", "q", "ship");
  kept.keepQuietly("/settings", "scroll:main", { top: 240, left: 0 });
  // *Forget where I was*: everything forgotten, written at once — then the seal.
  kept.clear();
  kept.seal();
  assert.equal(kept.sealed, true);
  assert.equal(store.map.has("bisa.view.test"), false);
  const writes = store.writes;
  // A screen on its way out hands its last words over; a store still holding something writes it.
  assert.equal(kept.keepQuietly("/settings", "scroll:main", { top: 240, left: 0 }), false);
  assert.equal(kept.keep("/settings", "tab", "desktop"), false);
  assert.equal(kept.read("/settings", "tab"), undefined, "nothing is kept, not even for the window");
  assert.equal(heard, 0, "and nobody is told of a change that is none");
  assert.equal(kept.waiting(), false);
  // The window's own flush — put away, reloading — and the beat, had one been waiting.
  assert.equal(kept.flush(), false);
  time.pass(WRITE_AT_MOST_MS * 2);
  assert.equal(store.writes, writes, "the storage took nothing after the seal");
  assert.equal(store.map.has("bisa.view.test"), false);
  assert.deepEqual(kept.keptPlaces(), []);
  // Forgetting and moving are nothing to a memory that holds nothing, and schedule nothing.
  assert.equal(kept.forget("/settings"), false);
  assert.equal(kept.forgetUnder("/"), 0);
  assert.equal(kept.move("/a", "/b"), false);
  assert.equal(time.timers.size, 0);
  kept.seal();
  assert.equal(store.writes, writes, "sealed twice is sealed once");
});

test("a seal writes what waits before it closes: a memory sealed without being forgotten holds what it stood at", () => {
  const { kept, store } = memory();
  kept.keep("/goals", "q", "ship");
  assert.equal(kept.waiting(), true);
  kept.seal();
  assert.deepEqual(JSON.parse(store.map.get("bisa.view.test")).places, [["/goals", { q: "ship" }]]);
  assert.equal(kept.keep("/goals", "q", "ship it"), false);
  assert.equal(kept.read("/goals", "q"), "ship", "what it held is still read");
  assert.equal(kept.flush(), false);
  assert.deepEqual(JSON.parse(store.map.get("bisa.view.test")).places, [["/goals", { q: "ship" }]]);
  // The next window's memory is a new one: it reads what was written, and keeps again.
  const next = memory({ store });
  assert.equal(next.kept.sealed, false);
  assert.equal(next.kept.read("/goals", "q"), "ship");
  assert.equal(next.kept.keep("/goals", "q", "ship it"), true);
});

test("another workspace's memory is forgotten whole; the same workspace's is kept", () => {
  const store = storage();
  const first = memory({ store });
  assert.equal(first.kept.adopt("alice"), false, "a memory with no owner takes the first");
  first.kept.keep("/goals", "q", "ship");
  first.kept.flush();
  assert.equal(JSON.parse(store.map.get("bisa.view.test")).owner, "alice");

  const same = memory({ store });
  assert.equal(same.kept.adopt("alice"), false);
  assert.equal(same.kept.read("/goals", "q"), "ship");
  assert.equal(same.kept.adopt(""), false, "no owner known yet changes nothing");
  assert.equal(same.kept.adopt(null), false);

  const other = memory({ store });
  assert.equal(other.kept.adopt("bob"), true);
  assert.deepEqual(other.kept.keptPlaces(), []);
  assert.deepEqual(JSON.parse(store.map.get("bisa.view.test")), { v: 1, owner: "bob", places: [] });
});

test("a way out waits what is left of the settle, and nothing after an idle moment", () => {
  assert.equal(settleLeft(0, 5_000, 1_000), 0, "never written");
  assert.equal(settleLeft(4_000, 5_000, 1_000), 0, "written a second ago");
  assert.equal(settleLeft(4_700, 5_000, 1_000), 700);
  assert.equal(settleLeft(5_000, 5_000, 1_000), 1_000);
  assert.equal(settleLeft(6_000, 5_000, 1_000), 0, "a clock that went back waits nothing");
});
