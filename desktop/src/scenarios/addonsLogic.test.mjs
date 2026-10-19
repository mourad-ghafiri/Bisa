/**
 * The pure half of four built-ins — tic-tac-toe's rules and machine, the
 * sticky note's sanitiser, the media player's sources and pictures, Bug
 * hunt's insects, tools and marks — each a
 * `logic.js` the page loads before its `main.js`, run here alone in a `vm`
 * (the `addonSdk.test.mjs` recipe): the machine never loses, easy plays a
 * legal cell, a note keeps its formatting and nothing else, and a pasted
 * source is read as YouTube, a link or nothing with its reason.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

function logicOf(slug) {
  const source = readFileSync(new URL(`../../../library/addons/${slug}/logic.js`, import.meta.url), "utf8");
  const ctx = { Math, Array, Object, String, JSON, TextEncoder, Infinity, Number, decodeURIComponent };
  vm.runInNewContext(source, ctx);
  return ctx;
}
const plain = (v) => JSON.parse(JSON.stringify(v));

// --- tic-tac-toe -----------------------------------------------------------

const T = logicOf("tic-tac-toe").TicTacToe;
const board = (s) => s.split("").map((c) => (c === "." ? "" : c));

test("the rules: three in a row names the winner and the line, a full board is full", () => {
  assert.deepEqual(plain(T.winner(board("XXX.O.O.."))), { who: "X", line: [0, 1, 2] });
  assert.deepEqual(plain(T.winner(board("O.X.O.X.O"))), { who: "O", line: [0, 4, 8] });
  assert.equal(T.winner(board("XOXXOXOXO")), null);
  assert.equal(T.full(board("XOXXOXOXO")), true);
  assert.equal(T.full(board("XOX.OXOXO")), false);
  assert.deepEqual(plain(T.emptyCells(board("X...O...X"))), [1, 2, 3, 5, 6, 7]);
  assert.equal(T.other("X"), "O");
});

test("unbeatable takes a win, blocks a loss, answers a fork, and stays out of a finished game", () => {
  assert.equal(T.bestMove(board("OO.XX...."), "O", "unbeatable"), 2, "a win first");
  assert.equal(T.bestMove(board("XX..O...."), "O", "unbeatable"), 2, "then a block");
  // X holds two corners with the centre O's: the only answers that do not
  // lose are the edges — a corner hands X a fork.
  const answer = T.bestMove(board("X...O...X"), "O", "unbeatable");
  assert.ok([1, 3, 5, 7].includes(answer), `an edge, not a corner: ${answer}`);
  assert.equal(T.bestMove(board("XXX.O.O.."), "O", "unbeatable"), -1, "a won board");
  assert.equal(T.bestMove(board("XOXXOXOXO"), "O", "unbeatable"), -1, "a full board");
});

/** Every game against the machine at `level`, the human trying every move: the outcomes. */
function everyGame(machine, level, starter) {
  const outcomes = { human: 0, machine: 0, draw: 0 };
  const human = T.other(machine);
  const play = (cells, turn) => {
    const w = T.winner(cells);
    if (w) { outcomes[w.who === machine ? "machine" : "human"] += 1; return; }
    if (T.full(cells)) { outcomes.draw += 1; return; }
    if (turn === machine) {
      const move = T.bestMove(cells, machine, level);
      assert.ok(move >= 0 && !cells[move], "the machine's move is a free cell");
      const next = cells.slice(); next[move] = machine;
      play(next, human);
      return;
    }
    for (const i of T.emptyCells(cells)) { const next = cells.slice(); next[i] = human; play(next, machine); }
  };
  play(board("........."), starter);
  return outcomes;
}

test("unbeatable never loses — against every line of play, whoever starts", () => {
  const second = everyGame("O", "unbeatable", "X");
  assert.equal(second.human, 0, `X never wins: ${JSON.stringify(second)}`);
  assert.ok(second.machine > 0 && second.draw > 0);
  const first = everyGame("O", "unbeatable", "O");
  assert.equal(first.human, 0, `X never wins when O starts: ${JSON.stringify(first)}`);
});

test("easy plays a legal cell from the random it is handed, and nothing on a finished board", () => {
  const cells = board("X.O.X....");
  const free = T.emptyCells(cells);
  assert.equal(T.bestMove(cells, "O", "easy", () => 0), free[0]);
  assert.equal(T.bestMove(cells, "O", "easy", () => 0.999), free[free.length - 1]);
  for (let i = 0; i < 20; i++) {
    const move = T.bestMove(cells, "O", "easy", () => i / 20);
    assert.ok(free.includes(move), `a free cell: ${move}`);
  }
  assert.equal(T.bestMove(board("XXX.O.O.."), "O", "easy", () => 0), -1);
  assert.deepEqual(plain(cells), plain(board("X.O.X....")), "the board given is never changed");
});

// --- sticky note -------------------------------------------------------------

const N = logicOf("sticky-note").StickyNote;

test("the sanitiser keeps basic formatting and lists, and nothing else", () => {
  assert.equal(N.sanitize("<b>bold</b> and <i>italic</i>, <u>under</u>, <s>struck</s>"), "<b>bold</b> and <i>italic</i>, <u>under</u>, <s>struck</s>");
  assert.equal(N.sanitize("<strong>b</strong><em>i</em><strike>s</strike>"), "<b>b</b><i>i</i><s>s</s>", "strong/em/strike fold into b/i/s");
  assert.equal(N.sanitize("<ul><li>one</li><li>two</li></ul><ol><li>1</li></ol>"), "<ul><li>one</li><li>two</li></ul><ol><li>1</li></ol>");
  assert.equal(N.sanitize("line<br>next<div>para</div><p>p</p>"), "line<br>next<div>para</div><p>p</p>");
});

test("the sanitiser drops a script and its body, every attribute, unknown tags, comments, and closes what was left open", () => {
  assert.equal(N.sanitize('hi<script>alert(1)</script> there'), "hi there");
  assert.equal(N.sanitize('<style>b{color:red}</style>x'), "x");
  assert.equal(N.sanitize('<b onclick="x()" style="color:red">b</b>'), "<b>b</b>");
  assert.equal(N.sanitize('<a href="https://x">link</a> <img src=x onerror=y> <span>s</span>'), "link  s");
  assert.equal(N.sanitize("<b><i>open"), "<b><i>open</i></b>", "unclosed tags are closed");
  assert.equal(N.sanitize("stray</b></i> close"), "stray close", "a close with no open goes");
  assert.equal(N.sanitize("<b>a<i>b</b>c</i>"), "<b>a<i>b</i></b>c", "a close closes what it must");
  assert.equal(N.sanitize("<!-- note -->text"), "text");
  assert.equal(N.sanitize("a < b && c > d &amp; &lt;"), "a &lt; b &amp;&amp; c &gt; d &amp; &lt;", "text is escaped where it needs to be");
  assert.equal(N.sanitize("<svg onload=x><b>in</b></svg>after"), "after");
  assert.equal(N.sanitize(null), "");
  assert.equal(N.sanitize("<br/>"), "<br>");
});

test("the plain text reads as the note does, the papers are six, and the cap is in bytes under the platform's", () => {
  assert.equal(N.plainText("<b>Milk</b><br><ul><li>eggs</li><li>bread &amp; jam</li></ul>"), "Milk\n• eggs\n• bread & jam");
  assert.deepEqual(plain(N.COLORS), ["yellow", "rose", "mint", "sky", "lilac", "stone"]);
  assert.equal(N.isColor("rose"), true);
  assert.equal(N.isColor("red"), false);
  assert.equal(N.MAX_HTML_BYTES, 12000);
  assert.ok(N.MAX_HTML_BYTES < 16 * 1024);
  assert.equal(N.fits("x".repeat(12000)), true);
  assert.equal(N.fits("x".repeat(12001)), false);
  assert.equal(N.fits("é".repeat(6001)), false, "bytes, not characters");
  assert.equal(N.byteLength("a😀"), 5);
});

// --- media player ------------------------------------------------------------

const M = logicOf("media-player").MediaPlayer;

test("a pasted source is a YouTube video in every spelling, an https link, or nothing with its reason", () => {
  for (const url of [
    "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
    "https://youtube.com/watch?feature=share&v=dQw4w9WgXcQ&t=10",
    "https://m.youtube.com/watch?v=dQw4w9WgXcQ",
    "https://music.youtube.com/watch?v=dQw4w9WgXcQ&list=RD",
    "https://youtu.be/dQw4w9WgXcQ?t=4",
    "https://www.youtube.com/shorts/dQw4w9WgXcQ",
    "https://www.youtube.com/embed/dQw4w9WgXcQ",
    "https://www.youtube.com/live/dQw4w9WgXcQ",
    "youtube.com/watch?v=dQw4w9WgXcQ",
    "  https://youtu.be/dQw4w9WgXcQ  ",
  ]) {
    assert.deepEqual(plain(M.parseSource(url)), { kind: "youtube", video: "dQw4w9WgXcQ" }, url);
  }
  assert.equal(M.parseSource("https://www.youtube.com/watch?v=short").kind, "link", "not an id: an ordinary link to YouTube");
  assert.deepEqual(plain(M.parseSource("https://cdn.example/music/Track%20One.mp3?x=1")), { kind: "link", url: "https://cdn.example/music/Track%20One.mp3?x=1", name: "Track One.mp3" });
  assert.equal(M.parseSource("https://cdn.example/").name, "cdn.example", "a bare host is its own name");
  assert.deepEqual(plain(M.parseSource("")), { kind: "none", reason: "empty" });
  assert.deepEqual(plain(M.parseSource("http://cdn.example/a.mp3")), { kind: "none", reason: "http" });
  assert.deepEqual(plain(M.parseSource("a song I like")), { kind: "none", reason: "not-a-link" });
  assert.deepEqual(plain(M.parseSource("https://cdn.example/a b.mp3")), { kind: "none", reason: "not-a-link" }, "a space is not a link");
  assert.deepEqual(plain(M.entryOf(M.parseSource("https://youtu.be/dQw4w9WgXcQ"))), { kind: "youtube", key: "yt:dQw4w9WgXcQ", label: "YouTube · dQw4w9WgXcQ", video: "dQw4w9WgXcQ" });
  assert.deepEqual(plain(M.entryOf({ kind: "file", name: "a.mp3" })), { kind: "file", key: "file:a.mp3", label: "a.mp3" });
});

test("time reads as minutes and seconds, the ramp runs cold to warm, and the rows are logarithmic", () => {
  assert.equal(M.timeWords(0), "0:00");
  assert.equal(M.timeWords(65.9), "1:05");
  assert.equal(M.timeWords(3600 + 61), "1:01:01");
  assert.equal(M.timeWords(NaN), "");
  assert.equal(M.timeWords(Infinity), "", "a live stream has no length");
  assert.equal(M.timeWords(-3), "");
  assert.deepEqual(plain(M.ramp(0)), [21, 20, 26], "the stage");
  assert.equal(M.FLOOR, 0.1);
  assert.deepEqual(plain(M.ramp(0.08)), [21, 20, 26], "the hiss is the floor, not a glow");
  assert.deepEqual(plain(M.ramp(0.08, [14, 13, 18])), [14, 13, 18], "the stage the page read");
  assert.deepEqual(plain(M.ramp(1)), [255, 244, 214], "warm white");
  assert.deepEqual(plain(M.ramp(2)), [255, 244, 214], "clamped");
  assert.deepEqual(plain(M.ramp(NaN)), [21, 20, 26]);
  const mid = M.ramp(0.55);
  assert.ok(mid[2] > mid[0], "the middle is cold");
  const warm = M.ramp(0.85);
  assert.ok(warm[0] > warm[2], "near the top is warm");
  const low = M.ramp(0.2, [14, 13, 18]);
  assert.ok(low[2] > low[0] && low[2] < 107, "just past the floor the stage warms to indigo");
  assert.deepEqual(plain(M.hexToRgb("#0e0d12")), [14, 13, 18]);
  assert.deepEqual(plain(M.hexToRgb(" 15141A ")), [21, 20, 26]);
  assert.deepEqual(plain(M.hexToRgb("rgb(1,2,3)")), [21, 20, 26], "not a hex: the stage");
  assert.equal(M.FILL_SECONDS, 12);
  assert.equal(M.columnWidth(848, 30), 2, "a 440 window at 2× fills in twelve seconds");
  assert.equal(M.columnWidth(1600, 30), 4);
  assert.equal(M.columnWidth(300, 30), 1, "never under a pixel");
  assert.equal(M.columnWidth(848, 4), 18, "the slow sweep under reduced motion takes wider steps");
  assert.equal(M.columnWidth(0, 30), 1);
  assert.equal(M.binFor(0, 100, 1024), 1);
  assert.equal(M.binFor(99, 100, 1024), 1023);
  assert.ok(M.binFor(50, 100, 1024) < 100, "half the rows are the bass");
  assert.equal(M.binFor(5, 1, 1024), 0);
  const spans = M.barSpans(8, 1024);
  assert.equal(spans.length, 8);
  for (const [from, to] of spans) assert.ok(to > from && from >= 0 && to <= 1024, `${from}..${to}`);
  assert.ok(spans[7][1] - spans[7][0] > spans[0][1] - spans[0][0], "the top bar spans more bins");
});

test("the recent list keeps twenty, moves a repeat to the front, and reads the store only as far as its shape allows", () => {
  let list = [];
  for (let i = 0; i < 25; i++) list = M.pushQueue(list, { kind: "link", key: `link:${i}`, label: `${i}`, url: `https://x/${i}` });
  assert.equal(list.length, 20);
  assert.equal(list[0].key, "link:24");
  assert.equal(list[19].key, "link:5");
  const again = M.pushQueue(list, { kind: "link", key: "link:7", label: "7", url: "https://x/7" });
  assert.equal(again.length, 20);
  assert.equal(again[0].key, "link:7");
  assert.equal(again.filter((e) => e.key === "link:7").length, 1, "never doubled");
  assert.deepEqual(plain(M.pushQueue(null, { kind: "file", key: "file:a", label: "a" }, 3)), [{ kind: "file", key: "file:a", label: "a" }]);
  assert.deepEqual(plain(M.readQueue("nope")), []);
  assert.deepEqual(plain(M.readQueue(JSON.stringify({ a: 1 }))), []);
  assert.deepEqual(
    plain(M.readQueue(JSON.stringify([{ kind: "youtube", key: "yt:dQw4w9WgXcQ", label: "A", video: "dQw4w9WgXcQ", extra: 1 }, { kind: "link", key: "link:x", label: "x", url: "https://x" }, { kind: "file", key: "file:a", label: "a" }, { kind: "link", key: "link:bad" }, 7, null, { kind: "other", key: "k", label: "l" }]))),
    [
      { kind: "youtube", key: "yt:dQw4w9WgXcQ", label: "A", video: "dQw4w9WgXcQ" },
      { kind: "link", key: "link:x", label: "x", url: "https://x" },
      { kind: "file", key: "file:a", label: "a" },
    ],
  );
  assert.equal(M.RECENT_CAP, 20);
});

test("the YouTube box is the stage less an inset, or nothing under the player's floor; seeks and volume stay in bounds", () => {
  assert.deepEqual(plain(M.youtubeBox({ left: 8.4, top: 6.6, width: 424, height: 246 })), { x: 12, y: 11, width: 416, height: 238 });
  assert.equal(M.youtubeBox({ left: 0, top: 0, width: 424, height: 207 }), null, "under 200 tall");
  assert.equal(M.youtubeBox({ left: 0, top: 0, width: 207, height: 300 }), null, "under 200 wide");
  assert.deepEqual(plain(M.youtubeBox({ left: 0, top: 0, width: 220, height: 220 }, 10)), { x: 10, y: 10, width: 200, height: 200 });
  assert.equal(M.MIN_PLAYER_SIDE, 200);
  assert.equal(M.seekBy(10, -15, 100), 0);
  assert.equal(M.seekBy(98, 5, 100), 100);
  assert.equal(M.seekBy(98, 5, NaN), 103, "no known length: no ceiling");
  assert.equal(M.seekBy(undefined, 5, Infinity), 5);
  assert.equal(M.volumeStep(98, 5), 100);
  assert.equal(M.volumeStep(3, -5), 0);
  assert.equal(M.volumeStep(50, 5), 55);
  assert.deepEqual(plain(M.SPEEDS), [1, 1.25, 1.5, 2, 0.75]);
  assert.equal(M.nextSpeed(1), 1.25);
  assert.equal(M.nextSpeed(0.75), 1, "round the list");
  assert.equal(M.nextSpeed(9), 1, "a speed not on the list starts it");
  assert.equal(M.speedWords(1), "1×");
  assert.equal(M.speedWords(1.25), "1.25×");
  for (const code of [1, 2, 3, 4, "http", "not-a-link", "unavailable", "youtube-not-allowed", "too-small", "zzz"]) {
    const words = M.errorWords(code);
    assert.ok(words.length > 12 && /[.]$/.test(words), `${code}: ${words}`);
  }
  assert.notEqual(M.errorWords(3), M.errorWords(4));
});

// --- bug hunt ------------------------------------------------------------------

const H = logicOf("bug-hunt").BugHunt;
const glass = { width: 800, height: 500 };
const hunt = (r, over = {}) => ({ bounds: glass, obstacles: [], pointer: null, rng: r, now: 0, scale: 1, ...over });

test("the same seed walks the same path, every species has its facts, and no insect leaves the glass or walks into a tab", () => {
  const walk = (seed) => {
    const r = H.rng(seed);
    let i = H.spawn("beetle", glass, [], r, 0, 1);
    const path = [];
    for (let k = 0; k < 600; k++) {
      i = H.step(i, 1 / 30, hunt(r, { now: k / 30 }));
      path.push([Math.round(i.x), Math.round(i.y)]);
    }
    return path;
  };
  assert.deepEqual(walk(42), walk(42));
  assert.notDeepEqual(walk(42), walk(43));
  for (const name of H.SPECIES_NAMES) {
    const sp = H.SPECIES[name];
    assert.ok(sp.size > 0 && sp.speed[0] <= sp.speed[1] && sp.hp >= 1 && sp.points > 0 && ["walk", "fly", "hover", "jump"].includes(sp.gait), name);
  }
  const tab = { x: 300, y: 100, width: 200, height: 200 };
  const r = H.rng(5);
  for (const name of H.SPECIES_NAMES) {
    let i = H.spawn(name, glass, [tab], r, 0, 1);
    const half = H.SPECIES[name].size / 2;
    let entered = false;
    for (let k = 1; k <= 900; k++) {
      i = H.step(i, 1 / 30, hunt(r, { obstacles: [tab], now: k / 30, pointer: k % 200 < 20 ? { x: 400, y: 400 } : null }));
      if (i.state === "dead") break;
      entered = entered || i.entered;
      if (i.entered && !i.leaving) {
        assert.ok(i.x >= half - 1 && i.x <= glass.width - half + 1 && i.y >= half - 1 && i.y <= glass.height - half + 1, `${name} left the glass at tick ${k}: ${i.x},${i.y} (${i.state})`);
      }
      assert.ok(!(i.x > tab.x + 2 && i.x < tab.x + tab.width - 2 && i.y > tab.y + 2 && i.y < tab.y + tab.height - 2), `${name} walked into the tab at tick ${k} (${i.state})`);
    }
    assert.ok(entered, `${name} never came onto the glass`);
  }
  // A cricket's hop rises and lands.
  const cr = H.rng(11);
  let c = { ...H.spawn("cricket", glass, [], cr, 0, 1), x: 400, y: 250, entered: true, state: "walk", until: -1 };
  let rose = false, landed = false;
  for (let k = 0; k < 400 && !landed; k++) {
    c = H.step(c, 1 / 30, hunt(cr, { now: k / 30 }));
    if (c.state === "jump" && c.z > 10) rose = true;
    if (rose && c.state !== "jump" && c.z === 0) landed = true;
  }
  assert.ok(rose && landed, "a hop rose and came down");
  // A cockroach near the pointer darts away from it.
  const rr = H.rng(2);
  let roach = { ...H.spawn("cockroach", glass, [], rr, 0, 1), x: 400, y: 250, entered: true, state: "walk", until: 99 };
  const before = H.step(roach, 1 / 30, hunt(rr, { now: 0.1, pointer: { x: 430, y: 250 } }));
  assert.equal(before.state, "dart");
  const after = H.step(before, 1 / 30, hunt(rr, { now: 0.2, pointer: { x: 430, y: 250 } }));
  assert.ok(after.x < before.x, "away from a pointer on its right");
  // An ant follows its leader's heels.
  const ar = H.rng(3);
  const leader = { ...H.spawn("ant", glass, [], ar, 0, 1), x: 200, y: 200, heading: 0, entered: true, state: "walk", until: 99 };
  let follower = { ...H.spawn("ant", glass, [], ar, 0, 2), x: 100, y: 260, heading: Math.PI, entered: true, state: "walk", until: 99, leader: 1 };
  for (let k = 0; k < 60; k++) follower = H.step(follower, 1 / 30, hunt(ar, { now: k / 30, leaderOf: (id) => (id === 1 ? leader : null) }));
  assert.ok(Math.abs(follower.y - 200) < 30 && follower.x > 120 && follower.x < 200, `fell in behind the leader: ${follower.x},${follower.y}`);
  assert.equal(H.offstage({ ...leader, x: -100, entered: true }, glass), true);
  assert.equal(H.offstage({ ...leader, x: -100, entered: false }, glass), false, "one still walking in is not gone");
});

test("each tool reaches what its shape covers and nothing beside; a whip needs a stroke; the spray slows before it kills; a beetle takes two", () => {
  const at = (species, x, y, id) => ({ ...H.spawn(species, glass, [], H.rng(1), 0, id), x, y, state: "walk", entered: true });
  const bugs = [at("ant", 100, 100, 1), at("ant", 160, 100, 2), at("ant", 100, 300, 3)];
  assert.deepEqual(plain(H.strike("swatter", { x: 100, y: 100, bounds: glass }, bugs, 1).hits), [1]);
  assert.deepEqual(plain(H.strike("swatter", { x: 130, y: 100, bounds: glass }, bugs, 1).hits), [1, 2], "a square of 56 plus an ant's 8 reaches both");
  assert.deepEqual(plain(H.strike("slipper", { x: 100, y: 130, bounds: glass }, bugs, 1).hits), [1], "26 plus 8 reaches 34");
  const miss = H.strike("slipper", { x: 100, y: 140, bounds: glass }, bugs, 1);
  assert.deepEqual(plain(miss.hits), []);
  assert.equal(miss.marks[0].kind, "crack");
  assert.equal(miss.sound, "thud");
  assert.equal(H.strike("whip", { from: { x: 0, y: 100 }, to: { x: 200, y: 100 }, x: 100, y: 100, length: 200, bounds: glass }, bugs, 1).hits.length, 2);
  assert.equal(H.strike("whip", { from: { x: 0, y: 100 }, to: { x: 10, y: 100 }, x: 5, y: 100, length: 10, bounds: glass }, bugs, 1), null, "too short to crack");
  assert.equal(H.strike("whip", { from: { x: 0, y: 400 }, to: { x: 200, y: 400 }, x: 100, y: 400, length: 200, bounds: glass }, bugs, 1).marks[0].kind, "scratch");
  assert.deepEqual(plain(H.strike("newspaper", { x: 100, y: 150, angle: Math.PI / 2, bounds: glass }, bugs, 1).hits), [1], "held upright, the paper reaches 60 up and down: the ant 50 above, not the one 150 below");
  assert.deepEqual(plain(H.strike("newspaper", { x: 100, y: 150, angle: 0, bounds: glass }, bugs, 1).hits), [], "held flat, only 20 up and down");
  assert.deepEqual(plain(H.strike("newspaper", { x: 150, y: 100, angle: 0, bounds: glass }, bugs, 1).hits), [1, 2], "held flat, 60 to each side: both ants on the line");
  const spray = H.strike("spray", { x: 100, y: 160, angle: -Math.PI / 2, bounds: glass }, bugs, 1);
  assert.deepEqual(plain(spray.hits), [1], "the sector points up: the ant above, not the one below");
  assert.equal(spray.slow, 1.2);
  assert.equal(spray.marks[0].kind, "mist");
  const r = H.rng(3);
  const sprayed = { ...bugs[0], slow: 1.2, sprayed: true, until: 99 };
  const slowed = H.step(sprayed, 0.5, hunt(r, { now: 0.5 }));
  assert.ok(Math.abs(slowed.x - sprayed.x) < 15 && slowed.state !== "dead", "slowed, alive");
  let dying = slowed;
  for (let k = 0; k < 4; k++) dying = H.step(dying, 0.5, hunt(r, { now: 1 + k * 0.5 }));
  assert.equal(dying.state, "dead");
  assert.equal(dying.cause, "spray");
  assert.deepEqual(plain(H.strike("swatter", { x: 100, y: 100, bounds: glass }, [{ ...bugs[0], state: "dead" }], 1).hits), [], "a dead one is never hit again");
  const squeegee = H.strike("squeegee", { x: 100, y: 100, bounds: glass }, bugs, 1);
  assert.deepEqual(plain(squeegee.hits), []);
  assert.equal(squeegee.wipes, true);
  assert.equal(H.strike("laser", { x: 0, y: 0 }, bugs, 1), null);
  assert.equal(H.SPECIES.beetle.hp, 2);
  assert.ok(H.isTool("whip") && !H.isTool("laser") && !H.isTool(7));
  assert.deepEqual(plain(H.TOOL_NAMES), ["whip", "swatter", "slipper", "newspaper", "spray", "squeegee"]);
});

test("cracks stay on the glass and off a tab, scratches follow the stroke, a splat wears its species' colour, marks fade, and a squeegee wipes what it crosses", () => {
  const c = H.crack(5, 5, 9, glass, 0);
  assert.ok(c.rays.length >= 6 && c.rings.length >= 1 && c.core > 0, "rays, at least one ring, a crushed core");
  assert.ok(c.polylines.length >= c.rays.length + c.rings.length, "every ray, fork and ring is a line to clip");
  for (const ray of c.rays) {
    assert.equal(ray.widths.length, ray.points.length - 1, "a width per segment");
    for (let i = 1; i < ray.widths.length; i++) assert.ok(ray.widths[i] <= ray.widths[i - 1] + 0.01, "a ray thins toward its tip");
  }
  for (const line of c.polylines) for (const p of line) assert.ok(p.x >= 0 && p.x <= 800 && p.y >= 0 && p.y <= 500, `${p.x},${p.y}`);
  assert.deepEqual(plain(H.crack(5, 5, 9, glass, 0).polylines), plain(c.polylines), "the same seed cracks the same way");
  assert.deepEqual(plain(H.clipOutside([[{ x: 0, y: 0 }, { x: 50, y: 0 }, { x: 150, y: 0 }, { x: 250, y: 0 }]], [{ x: 100, y: -10, width: 100, height: 20 }])), [[{ x: 0, y: 0 }, { x: 50, y: 0 }]], "the run before the tab stays; the lone point after it goes");
  assert.deepEqual(plain(H.clipOutside([[{ x: 0, y: 0 }, { x: 1, y: 1 }]], [])), [[{ x: 0, y: 0 }, { x: 1, y: 1 }]]);
  const s = H.scratch({ x: 0, y: 200 }, { x: 100, y: 200 }, 4, 0);
  assert.ok(s.polylines.length >= 2 && s.polylines.every((l) => l.every((p) => Math.abs(p.y - 200) < 14 && p.x >= -1 && p.x <= 101)));
  assert.ok(s.widths.every((w) => w[0] < w[Math.floor(w.length / 2)] && w[w.length - 1] < w[Math.floor(w.length / 2)]), "a scratch is thin at both ends and widest in the middle");
  assert.ok(s.chips.length >= 2 && s.chips.every((ch) => ch.x > 20 && ch.x < 80), "a few chips where the lash bit hardest");
  const thrown = H.splat(50, 50, "cockroach", 2, 0, 0);
  const forward = thrown.drops.filter((d) => d.dx > 0).length;
  assert.ok(forward >= thrown.drops.length / 2, "most droplets fly the way the blow went");
  const struck = H.step({ ...H.spawn("beetle", glass, [], H.rng(1), 0, 9), x: 300, y: 300, entered: true, state: "hit", until: 0.1, t: 0 }, 0.2, hunt(H.rng(1), { now: 0.2 }));
  assert.equal(struck.state, "walk", "a struck beetle staggers, then walks on");
  assert.ok(thrown.streaks.length >= 1 && thrown.legs.length >= 1);
  const sm = H.smear([{ x: 0, y: 0 }, { x: 60, y: 10 }], 60, 1, 0);
  assert.equal(sm.kind, "smear");
  assert.equal(sm.ttl, 5);
  assert.deepEqual(H.aliveMarks([sm], 6), []);
  const sp = H.splat(50, 50, "cockroach", 2, 0);
  assert.equal(sp.color, H.SPECIES.cockroach.splat);
  assert.equal(sp.ttl, null);
  assert.ok(sp.drops.length >= 6);
  const marks = [c, s, sp, H.mist(0, 0, 0, H.TOOLS.spray.shape, 1, 0)];
  assert.equal(H.aliveMarks(marks, 10).length, 3, "the mist is gone after eight seconds");
  assert.equal(H.aliveMarks(marks, 25).length, 1, "only the splat stays until wiped");
  assert.equal(H.markOpacity(c, 0), 1);
  assert.equal(H.markOpacity(c, 18), 0.5);
  assert.equal(H.markOpacity(c, 21), 0);
  assert.equal(H.markOpacity(sp, 999), 1);
  assert.deepEqual(plain(H.wipe(marks, [{ x: 40, y: 40 }, { x: 60, y: 60 }], 60).map((m) => m.kind)), ["crack", "scratch", "mist"], "the splat under the stroke went");
  assert.equal(H.wipe(marks, [], 60).length, 4);
});

test("the streak multiplies within two seconds and a miss ends it; arrivals quicken with work and slow under reduced motion; the strip moves off a tab; the store is read as far as its shape allows", () => {
  let sc = { score: 0, streak: 0, best: 0, lastHitAt: -100 };
  sc = H.scoreHit(sc, { species: "ant" }, 10);
  assert.deepEqual([sc.score, sc.streak, sc.lastPoints], [5, 1, 5]);
  sc = H.scoreHit(sc, { species: "ant" }, 11);
  assert.deepEqual([sc.score, sc.streak], [15, 2]);
  sc = H.scoreHit(sc, { species: "beetle" }, 14);
  assert.deepEqual([sc.score, sc.streak, sc.best], [45, 1, 45], "three seconds later the streak starts over");
  for (let k = 0; k < 7; k++) sc = H.scoreHit(sc, { species: "ant" }, 14 + (k + 1) * 0.5);
  assert.equal(sc.lastPoints, 25, "five times at most");
  const missed = H.scoreMiss(sc);
  assert.equal(missed.streak, 0);
  assert.equal(missed.score, sc.score);
  assert.equal(missed.best, sc.best);
  assert.equal(H.maxInsects({ difficulty: "calm" }), 4);
  assert.equal(H.maxInsects({ difficulty: "swarm", reduced: true }), 7);
  assert.equal(H.maxInsects({ difficulty: "x" }), 8, "an unknown difficulty is busy");
  assert.ok(H.spawnInterval({ difficulty: "busy", working: 0 }) > H.spawnInterval({ difficulty: "busy", working: 3 }));
  assert.equal(H.spawnInterval({ difficulty: "swarm", working: 99 }), 0.8, "work past eight counts as eight, and the floor holds");
  assert.ok(H.spawnInterval({ difficulty: "swarm", working: 8, reduced: true }) > H.spawnInterval({ difficulty: "swarm", working: 8 }));
  assert.equal(H.speedScale({ reduced: true }), 0.5);
  assert.equal(H.speedScale({}), 1);
  const vp = { width: 1024, height: 660 }, size = { width: 56, height: 320 };
  assert.deepEqual(plain(H.stripPlacement(vp, [], size)), { x: 960, y: 170, width: 56, height: 320 });
  assert.equal(H.stripPlacement(vp, [{ x: 700, y: 0, width: 324, height: 660 }], size).x, 8, "a tab on the right sends it left");
  const bottom = H.stripPlacement(vp, [{ x: 700, y: 0, width: 324, height: 660 }, { x: 0, y: 0, width: 300, height: 660 }], size);
  assert.deepEqual([bottom.x, bottom.y], [484, 332], "tabs on both sides send it to the bottom");
  assert.equal(H.stripPlacement({ width: 400, height: 200 }, [], size).height, 184, "a short glass shortens the strip");
  assert.deepEqual(plain(H.solidRects({ x: 960.4, y: 170, width: 56, height: 320 })), [{ x: 960, y: 170, width: 56, height: 320, radius: 10 }]);
  assert.deepEqual(plain(H.readSaved("junk")), { best: 0, muted: false, tool: "swatter", difficulty: "busy" });
  assert.deepEqual(plain(H.readSaved(JSON.stringify({ best: 120.7, muted: true, tool: "whip", difficulty: "swarm" }))), { best: 120, muted: true, tool: "whip", difficulty: "swarm" });
  assert.deepEqual(plain(H.readSaved(JSON.stringify({ best: -5, muted: "yes", tool: "laser", difficulty: "insane" }))), { best: 0, muted: false, tool: "swatter", difficulty: "busy" });
  assert.deepEqual(plain(H.readSaved(JSON.stringify([1]))), { best: 0, muted: false, tool: "swatter", difficulty: "busy" });
  assert.equal(H.waitWords(0), "Nothing is running — hunt anyway.");
  assert.equal(H.waitWords(1), "One thing is running — hunt while you wait.");
  assert.equal(H.waitWords(3), "3 things are running — hunt while you wait.");
  assert.deepEqual(plain(H.DIFFICULTY_NAMES), ["calm", "busy", "swarm"]);
});
