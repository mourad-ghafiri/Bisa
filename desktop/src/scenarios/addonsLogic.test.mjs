/**
 * The pure half of seven built-ins — tic-tac-toe's rules and machine, the
 * sticky note's sanitiser, the media player's sources and pictures, Bug
 * hunt's insects, tools and marks, and the three terminal games' rules
 * (Tetrominal's well, Linefeed's worm, Linerun's file) — each a
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

test("each tool reaches what its shape covers and nothing beside; a whip bites at its tip and along its lie; the spray slows before it kills; a beetle takes two", () => {
  const at = (species, x, y, id) => ({ ...H.spawn(species, glass, [], H.rng(1), 0, id), x, y, state: "walk", entered: true });
  const bugs = [at("ant", 100, 100, 1), at("ant", 160, 100, 2), at("ant", 100, 300, 3)];
  assert.deepEqual(plain(H.strike("swatter", { x: 100, y: 100, bounds: glass }, bugs, 1).hits), [1]);
  assert.deepEqual(plain(H.strike("swatter", { x: 130, y: 100, bounds: glass }, bugs, 1).hits), [1, 2], "a square of 56 plus an ant's 8 reaches both");
  assert.deepEqual(plain(H.strike("slipper", { x: 100, y: 130, bounds: glass }, bugs, 1).hits), [1], "26 plus 8 reaches 34");
  const miss = H.strike("slipper", { x: 100, y: 140, bounds: glass }, bugs, 1);
  assert.deepEqual(plain(miss.hits), []);
  assert.equal(miss.marks[0].kind, "crack");
  assert.equal(miss.sound, "thud");
  // The whip's gesture: the tip on the pointer, the tail 56 px back along the lash's lie, the angle the way the lash flew.
  const lash = (x, y, lie) => ({ x, y, from: { x, y }, to: { x: x + Math.cos(lie) * 56, y: y + Math.sin(lie) * 56 }, angle: lie + Math.PI, bounds: glass });
  assert.deepEqual(plain(H.TOOLS.whip.shape), { kind: "lash", tip: 14, width: 10, reach: 56 });
  assert.equal(H.TOOLS.whip.minStroke, undefined, "a click is a crack: no stroke to measure");
  const cracked = H.strike("whip", lash(100, 100, 0), bugs, 1);
  assert.deepEqual(plain(cracked.hits), [1, 2], "the ant under the tip and the one 60 along the lie, 4 off its line");
  assert.equal(cracked.sound, "whip");
  assert.ok(cracked.marks.every((m) => m.kind === "splat" && m.streaks.every((st) => st.x2 < st.x1)), "a splat is thrown the way the lash flew: tail to tip, leftward here");
  assert.deepEqual(plain(H.strike("whip", lash(100, 100, Math.PI), bugs, 1).hits), [1], "the lash lies behind the tip, never ahead of it");
  assert.deepEqual(plain(H.strike("whip", lash(100, 121, Math.PI), bugs, 1).hits), [1], "the tip's 14 plus an ant's 8 reaches 22");
  const scratched = H.strike("whip", lash(100, 123, 0), bugs, 1);
  assert.deepEqual(plain(scratched.hits), [], "23 is past the tip, and the lash lies the other way");
  assert.equal(scratched.marks[0].kind, "scratch");
  assert.ok(scratched.marks[0].chips.every((ch) => ch.x > 100 && ch.x < 124 && Math.abs(ch.y - 123) <= 3), "the chips sit near the tip, in the first third of the lash");
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

test("cracks stay on the glass and off a tab, scratches run from the tip back along the lash, a splat wears its species' colour, marks fade, and a squeegee wipes what it crosses", () => {
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
  assert.ok(s.chips.length >= 2 && s.chips.every((ch) => ch.x > 2 && ch.x < 38), "a few chips near the tip, where the lash bit hardest");
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

// --- tetrominal ------------------------------------------------------------------

const Tm = logicOf("tetrominal").Tetrominal;
const tetro = (seed = 7, opts) => Tm.newGame(seed, opts);
/** The well with `rows` rows filled solid from the floor up, but for the named columns. */
const filledWell = (game, rows, holes = []) => {
  const cells = game.cells.map((r) => r.slice());
  for (let i = 0; i < rows; i++) {
    const y = Tm.ROWS_ALL - 1 - i;
    for (let x = 0; x < Tm.COLS; x++) cells[y][x] = holes.includes(x) ? 0 : "J";
  }
  return { ...game, cells };
};
const withPiece = (game, piece) => ({ ...game, piece });

test("tetrominal: the same seed and the same keys replay the same game, and the piece always fits while the build stands", () => {
  const script = Tm.ACTION_NAMES.filter((a) => a !== "pause" && a !== "resume");
  const run = (seed) => {
    let g = tetro(seed);
    for (let i = 0; i < 500 && !g.over; i++) g = Tm.tick(Tm.apply(g, script[i % script.length]), 16);
    return g;
  };
  assert.deepEqual(plain(run(7)), plain(run(7)));
  assert.notDeepEqual(plain([run(7).piece, run(7).queue]), plain([run(8).piece, run(8).queue]));
  for (const seed of [1, 2, 3]) {
    let g = tetro(seed);
    for (let i = 0; i < 200; i++) {
      assert.equal(g.cells.length, Tm.ROWS_ALL);
      g.cells.forEach((r) => assert.equal(r.length, Tm.COLS));
      if (!g.over) assert.ok(Tm.fits(g.cells, g.piece), `seed ${seed} step ${i}`);
      g = Tm.tick(Tm.apply(g, script[(i * 7 + seed) % script.length]), 50);
    }
  }
});

test("tetrominal: every bag of seven holds each piece once, the first seven dealt too", () => {
  const sorted = plain(Tm.PIECE_NAMES).slice().sort();
  let state = 99;
  for (let i = 0; i < 10; i++) {
    const r = Tm.shuffledBag(state);
    state = r.state;
    assert.deepEqual(plain(r.bag).slice().sort(), sorted);
  }
  const g = tetro(3);
  assert.deepEqual([g.piece.name, ...plain(g.queue)].slice(0, 7).sort(), sorted);
  assert.ok(g.queue.length >= 3);
});

test("tetrominal: every piece has four turns of four cells inside its box, O turns into itself, a fifth turn is the first", () => {
  for (const name of Tm.PIECE_NAMES) {
    const p = Tm.PIECES[name];
    assert.equal(p.shapes.length, 4);
    for (const s of p.shapes) {
      assert.equal(s.length, 4);
      for (const [x, y] of s) assert.ok(x >= 0 && x < p.box && y >= 0 && y < p.box, name);
    }
  }
  assert.deepEqual(plain(Tm.shapeOf("O", 1)), plain(Tm.shapeOf("O", 0)));
  assert.deepEqual(plain(Tm.shapeOf("T", 4)), plain(Tm.shapeOf("T", 0)));
  assert.deepEqual(plain(Tm.shapeOf("T", -1)), plain(Tm.shapeOf("T", 3)));
});

test("tetrominal: a turn against the wall is nudged in, the long piece by two, and a boxed-in piece neither turns nor moves", () => {
  const g = tetro(1);
  const t = Tm.apply(withPiece(g, { name: "T", rot: 1, x: -1, y: 5 }), "rotateCw");
  assert.deepEqual(plain(t.piece), { name: "T", rot: 2, x: 0, y: 5 });
  const i = Tm.apply(withPiece(g, { name: "I", rot: 1, x: -2, y: 5 }), "rotateCw");
  assert.deepEqual(plain(i.piece), { name: "I", rot: 2, x: 0, y: 5 });
  const full = g.cells.map((r) => r.map(() => "J"));
  const piece = { name: "T", rot: 0, x: 3, y: 10 };
  for (const [x, y] of plain(Tm.cellsOf(piece))) full[y][x] = 0;
  const stuck = { ...g, cells: full, piece };
  assert.equal(Tm.apply(stuck, "rotateCw"), stuck);
  assert.equal(Tm.apply(stuck, "rotateCcw"), stuck);
  assert.equal(Tm.apply(stuck, "left"), stuck);
  assert.equal(Tm.apply(stuck, "softDrop"), stuck);
});

test("tetrominal: a clear is worth its count by level with a combo on top, and a single clear end to end logs its line", () => {
  assert.equal(Tm.clearScore(1, 1, false, 0), 100);
  assert.equal(Tm.clearScore(2, 1, false, 0), 300);
  assert.equal(Tm.clearScore(3, 1, false, 0), 500);
  assert.equal(Tm.clearScore(4, 1, false, 0), 800);
  assert.equal(Tm.clearScore(4, 3, false, 0), 2400);
  assert.equal(Tm.clearScore(1, 2, false, 0), 200);
  assert.equal(Tm.clearScore(1, 1, false, 2), 200);
  const g = withPiece(filledWell(tetro(2), 1, [4, 5]), { name: "O", rot: 0, x: 3, y: 1 });
  const h = Tm.apply(g, "hardDrop");
  assert.equal(h.lines, 1);
  assert.equal(h.score, 100 + 2 * 19);
  assert.equal(h.combo, 0);
  assert.deepEqual(plain(h.log[h.log.length - 1]), { kind: "clear", text: "✓ 1 line compiled" });
  assert.equal(h.cells[Tm.ROWS_ALL - 1][4], "O");
  assert.equal(h.cells[Tm.ROWS_ALL - 1][0], 0);
  assert.equal(h.piece.name, g.queue[0]);
});

test("tetrominal: a second quad in a row is half again, only for quads, and the next clear breaks it", () => {
  assert.equal(Tm.clearScore(4, 1, true, 0), 1200);
  assert.equal(Tm.clearScore(4, 1, true, 1), 1250);
  assert.equal(Tm.clearScore(1, 1, true, 0), 100);
  const g = withPiece(filledWell(tetro(4), 4, [0]), { name: "I", rot: 1, x: -2, y: 1 });
  const q = Tm.apply(g, "hardDrop");
  assert.equal(q.lines, 4);
  assert.equal(q.lastQuad, true);
  assert.equal(q.score, 800 + 2 * 17);
  assert.equal(q.log[q.log.length - 1].text, "✓ 4 lines compiled — quad");
  const s = withPiece(filledWell(q, 1, [4, 5]), { name: "O", rot: 0, x: 3, y: 1 });
  const after = Tm.apply(s, "hardDrop");
  assert.equal(after.lastQuad, false);
  assert.equal(after.combo, 1);
  assert.equal(after.score, q.score + 100 + 50 + 2 * 19);
  assert.equal(after.log[after.log.length - 1].text, "  combo ×2");
  const again = withPiece(filledWell(q, 4, [0]), { name: "I", rot: 1, x: -2, y: 1 });
  assert.equal(Tm.apply(again, "hardDrop").log.slice(-2)[0].text, "✓ 4 lines compiled — back-to-back quad");
});

test("tetrominal: hold keeps the piece and brings the next, refuses a second hold until a lock, then swaps", () => {
  const g = tetro(5);
  const h = Tm.apply(g, "hold");
  assert.equal(h.hold, g.piece.name);
  assert.equal(h.canHold, false);
  assert.deepEqual(plain(h.piece), { name: g.queue[0], rot: 0, x: 3, y: 1 });
  assert.equal(Tm.apply(h, "hold"), h);
  const d = Tm.apply(h, "hardDrop");
  assert.equal(d.canHold, true);
  const s = Tm.apply(d, "hold");
  assert.equal(s.piece.name, h.hold);
  assert.equal(s.hold, d.piece.name);
  assert.equal(s.queue.length, d.queue.length, "a swap takes nothing from the queue");
});

test("tetrominal: the ghost sits where a hard drop lands, which pays two a row", () => {
  const g = tetro(6);
  const y = Tm.ghostRow(g);
  const h = Tm.apply(g, "hardDrop");
  const landed = plain(Tm.cellsOf({ ...plain(g.piece), y }));
  for (const [x, yy] of landed) assert.equal(h.cells[yy][x], g.piece.name);
  assert.equal(h.score, 2 * (y - g.piece.y));
  assert.equal(h.pieces, 1);
  const ghosts = plain(Tm.view(g)).rows.flatMap((r, row) => r.cells.map((c, x) => ({ c, x, row })).filter((o) => o.c.cls === "ghost"));
  assert.equal(ghosts.length, 4);
  for (const o of ghosts) assert.equal(o.c.ch, "░░");
  assert.deepEqual(ghosts.map((o) => `${o.x},${o.row}`).sort(), landed.map(([x, yy]) => `${x},${yy - Tm.HIDDEN}`).sort());
});

test("tetrominal: a piece that cannot enter ends the build, a piece locked out of sight too, and nothing moves after", () => {
  const g = withPiece(filledWell(tetro(8), Tm.ROWS_ALL - 3, [0]), { name: "T", rot: 0, x: 3, y: 1 });
  const o = Tm.apply(g, "hardDrop");
  assert.equal(o.over, true);
  assert.equal(o.log[o.log.length - 1].kind, "over");
  assert.equal(Tm.apply(o, "left"), o);
  assert.equal(Tm.apply(o, "hardDrop"), o);
  assert.equal(Tm.tick(o, 1000), o);
  const lockOut = withPiece(filledWell(tetro(8), Tm.ROWS_ALL - Tm.HIDDEN), { name: "I", rot: 0, x: 3, y: 0 });
  const lo = Tm.apply(lockOut, "hardDrop");
  assert.equal(lo.over, true);
  assert.equal(lo.pieces, 1);
  assert.equal(lo.cells[1][3], "I");
});

test("tetrominal: the view is twenty rows of ten cells numbered like an editor, the ghost only when asked, the gutter warned by a tall stack", () => {
  const g = tetro(9);
  const v = plain(Tm.view(g));
  assert.equal(v.rows.length, Tm.ROWS);
  v.rows.forEach((r) => assert.equal(r.cells.length, Tm.COLS));
  assert.equal(v.rows[0].line, 1);
  assert.equal(v.rows[19].line, 20);
  assert.equal(v.rows[0].warn, false);
  const active = v.rows.flatMap((r) => r.cells).filter((c) => c.cls === Tm.ROLES[g.piece.name] && c.ch === "██");
  assert.ok(active.length >= 1 && active.length <= 4, "the spawn straddles the hidden rows");
  assert.equal(plain(Tm.view(g, { ghost: false })).rows.flatMap((r) => r.cells).filter((c) => c.cls === "ghost").length, 0);
  const tall = filledWell(g, 16, [0]);
  assert.equal(Tm.danger(tall), true);
  assert.equal(Tm.view(tall).rows[5].warn, true);
  assert.equal(Tm.stackHeight(g.cells), 0);
  assert.equal(Tm.stackHeight(tall.cells), 16);
  for (const name of Tm.PIECE_NAMES) assert.ok(["kw", "str", "num", "fn", "type", "cmt", "op"].includes(Tm.ROLES[name]));
  assert.equal(new Set(Object.values(plain(Tm.ROLES))).size, 7);
  assert.deepEqual(plain(Tm.preview("I")).map((r) => r.map((c) => c.ch).join("")), ["        ", "████████"]);
  assert.deepEqual(plain(Tm.preview("O")).map((r) => r.map((c) => c.ch).join("")), ["  ████  ", "  ████  "]);
});

test("tetrominal: the store is read as far as its shape allows", () => {
  assert.deepEqual(plain(Tm.readSaved("junk")), { best: 0, muted: false });
  assert.deepEqual(plain(Tm.readSaved('{"best":1234.7,"muted":true}')), { best: 1234, muted: true });
  assert.deepEqual(plain(Tm.readSaved('{"best":-5,"muted":"yes"}')), { best: 0, muted: false });
  assert.deepEqual(plain(Tm.readSaved("[1]")), { best: 0, muted: false });
  assert.deepEqual(plain(Tm.readSaved(null)), { best: 0, muted: false });
});

test("tetrominal: a pause freezes time and keys until resume, and the speeds fall by level to a floor", () => {
  const g = tetro(10);
  const p = Tm.apply(g, "pause");
  assert.equal(p.paused, true);
  assert.equal(Tm.tick(p, 5000), p);
  assert.equal(Tm.apply(p, "left"), p);
  assert.equal(Tm.apply(p, "resume").paused, false);
  assert.equal(Tm.apply(g, "resume"), g);
  assert.equal(Tm.apply(g, "jump"), g);
  assert.equal(Tm.SPEEDS[0], 1000);
  for (let i = 1; i < Tm.SPEEDS.length; i++) assert.ok(Tm.SPEEDS[i] <= Tm.SPEEDS[i - 1] && Number.isInteger(Tm.SPEEDS[i]) && Tm.SPEEDS[i] >= 30);
  assert.equal(Tm.speedOf(99), 30);
  assert.equal(Tm.speedOf(0), 1000);
  assert.equal(Tm.speedOf(2), 800);
  assert.deepEqual(plain(Tm.ACTION_NAMES), ["left", "right", "rotateCw", "rotateCcw", "softDrop", "hardDrop", "hold", "pause", "resume"]);
  assert.deepEqual(plain(Tm.LOG_KINDS), ["start", "clear", "quad", "combo", "level", "over"]);
  assert.equal(g.log[0].text, "$ tetrominal --seed 10");
});

test("tetrominal: gravity is one row per level beat with the remainder kept, the lock lands at half a second and resets fifteen times at most", () => {
  const g = tetro(10);
  assert.equal(Tm.tick(g, 1000).piece.y, g.piece.y + 1);
  const two = Tm.tick(Tm.tick(g, 700), 700);
  assert.equal(two.piece.y, g.piece.y + 1);
  assert.equal(two.fallMs, 400);
  assert.equal(Tm.tick(g, 5000).piece.y, g.piece.y + 1, "a slow frame moves one row at most");
  assert.equal(Tm.tick(g, 0), g);
  const landed = withPiece(g, { ...plain(g.piece), y: Tm.ghostRow(g) });
  assert.equal(Tm.tick(landed, 499).pieces, 0);
  assert.equal(Tm.tick(Tm.tick(landed, 499), 1).pieces, 1);
  let r = landed;
  for (let i = 0; i < Tm.LOCK_RESETS; i++) {
    r = Tm.apply(Tm.tick(r, 499), i % 2 ? "right" : "left");
    assert.equal(r.pieces, 0, `reset ${i}`);
    assert.equal(r.lockMs, 0);
  }
  r = Tm.apply(Tm.tick(r, 499), "right");
  assert.equal(r.lockMs, 499, "the sixteenth move keeps the lock running");
  assert.equal(Tm.tick(r, 1).pieces, 1);
  const half = Tm.tick(landed, 200);
  assert.equal(Tm.apply(half, "softDrop"), half, "a soft drop on the floor is nothing");
  const air = Tm.apply(g, "softDrop");
  assert.equal(air.piece.y, g.piece.y + 1);
  assert.equal(air.score, 1);
  const turned = Tm.apply(Tm.tick(landed, 300), "rotateCw");
  assert.equal(turned.lockMs, 0);
  assert.equal(turned.lockResets, 1);
});

test("tetrominal: clearing keeps the letters above and leaves its input alone, the level climbs every ten lines, the log keeps six lines", () => {
  const g = tetro(11);
  const cells = g.cells.map((r) => r.slice());
  for (let x = 0; x < Tm.COLS; x++) cells[Tm.ROWS_ALL - 1][x] = "S";
  cells[Tm.ROWS_ALL - 2][0] = "T";
  const before = plain(cells);
  const r = plain(Tm.clearLines(cells));
  assert.equal(r.count, 1);
  assert.equal(r.cells[Tm.ROWS_ALL - 1][0], "T");
  assert.equal(r.cells[Tm.ROWS_ALL - 1][1], 0);
  assert.deepEqual(r.cells[0], new Array(Tm.COLS).fill(0));
  assert.deepEqual(plain(cells), before);
  assert.equal(Tm.levelOf(10, 1), 2);
  assert.equal(Tm.levelOf(9, 1), 1);
  assert.equal(Tm.levelOf(10, 5), 6);
  let w = g;
  for (let i = 0; i < 12; i++) w = Tm.apply(withPiece(filledWell(w, 1, [4, 5]), { name: "O", rot: 0, x: 3, y: 1 }), "hardDrop");
  assert.equal(w.lines, 12);
  assert.equal(w.level, 2);
  assert.ok(w.log.length <= Tm.LOG_MAX);
  assert.ok(w.log.some((l) => l.kind === "level" && l.text === "→ level 2 · 800 ms"));
  assert.equal(tetro(3, { level: 5 }).level, 5);
});

// --- linefeed --------------------------------------------------------------------

const Lf = logicOf("linefeed").Linefeed;
const worm = (seed = 7, opts) => Lf.newGame(seed, opts);
const seg = (x, y, ch = "-", cls = "op") => ({ x, y, ch, cls });
const withBody = (game, body, dir = game.dir) => ({ ...game, body, dir, queue: [] });
const foodAt = (x, y, kind) => ({ ...plain(Lf.tokenOf(kind)), x, y });

test("linefeed: the same seed and the same turns replay the same game, and food is always on the grid", () => {
  const script = { 20: "up", 40: "right", 55: "down", 80: "left", 120: "up", 160: "right", 200: "down", 240: "left" };
  const run = (seed) => {
    let g = worm(seed);
    const trail = [];
    for (let i = 0; i < 300 && g.alive; i++) {
      if (script[i]) g = Lf.apply(g, script[i]);
      g = Lf.step(g);
      for (const f of g.food) assert.ok(f.x >= 0 && f.x < Lf.COLS && f.y >= 0 && f.y < Lf.ROWS);
      trail.push([g.body[0].x, g.body[0].y, g.score]);
    }
    return trail;
  };
  assert.deepEqual(run(7), run(7));
  assert.notDeepEqual(plain(worm(7).food), plain(worm(8).food));
});

test("linefeed: a turn the worm already goes, or its opposite, is refused; two wait; a third is dropped; an unknown key is nothing", () => {
  const g = worm(1);
  assert.equal(Lf.apply(g, "left"), g);
  assert.equal(Lf.apply(g, "right"), g);
  assert.deepEqual(plain(Lf.apply(Lf.apply(g, "up"), "down").queue), ["up"]);
  const two = Lf.apply(Lf.apply(g, "up"), "left");
  assert.deepEqual(plain(two.queue), ["up", "left"]);
  assert.equal(Lf.apply(two, "down"), two);
  assert.equal(Lf.step(two).dir, "up");
  assert.deepEqual(plain(Lf.step(two).queue), ["left"]);
  assert.equal(Lf.apply(g, "jump"), g);
});

test("linefeed: a bite grows the neck with the token's glyph and class, keeps the tail, pays, and opens a bracket", () => {
  const g = { ...worm(2), food: [foodAt(13, 8, "braceOpen")] };
  const s = Lf.step(g);
  assert.equal(s.body.length, 4);
  assert.deepEqual(plain(s.body[1]), { x: 12, y: 8, ch: "{", cls: "kw" });
  assert.deepEqual(plain(s.body[0]), { x: 13, y: 8, ch: "█", cls: "head" });
  assert.deepEqual([s.body[3].x, s.body[3].y], [g.body[2].x, g.body[2].y]);
  assert.equal(s.score, 2);
  assert.equal(s.food.length, Lf.FOOD);
  assert.deepEqual(plain(s.open), ["{"]);
  assert.equal(s.interval, Lf.intervalFor(4));
  assert.equal(s.ticks, 1);
});

test("linefeed: a plain move shifts every segment one place and keeps the glyphs with their segments", () => {
  const g = { ...worm(3), food: [] };
  const s = Lf.step(g);
  assert.equal(s.body.length, g.body.length);
  assert.deepEqual([s.body[0].x, s.body[0].y], [13, 8]);
  for (let i = 1; i < s.body.length; i++) assert.deepEqual([s.body[i].x, s.body[i].y], [g.body[i - 1].x, g.body[i - 1].y]);
  assert.deepEqual(s.body.map((b) => b.ch), g.body.map((b) => b.ch));
  assert.equal(s.ticks, 1);
});

test("linefeed: the wall kills and the body stays; nothing moves after death", () => {
  const g = withBody(worm(4), [seg(23, 8, "█", "head"), seg(22, 8), seg(21, 8)], "right");
  const dead = Lf.step(g);
  assert.equal(dead.alive, false);
  assert.equal(dead.cause, "wall");
  assert.deepEqual(plain(dead.body), plain(g.body));
  assert.equal(Lf.step(dead), dead);
  assert.equal(Lf.apply(dead, "up"), dead);
  assert.equal(Lf.tick(dead, 1000), dead);
  assert.equal(Lf.words(dead), "hit the wall");
});

test("linefeed: the body kills, but the tail that is leaving does not", () => {
  const bitten = Lf.step(withBody(worm(5), [seg(5, 5, "█", "head"), seg(6, 5), seg(6, 6), seg(5, 6), seg(4, 6)], "down"));
  assert.equal(bitten.alive, false);
  assert.equal(bitten.cause, "self");
  assert.equal(Lf.words(bitten), "bit yourself");
  const free = Lf.step({ ...withBody(worm(5), [seg(5, 5, "█", "head"), seg(6, 5), seg(6, 6), seg(5, 6)], "down"), food: [] });
  assert.equal(free.alive, true);
  assert.deepEqual([free.body[0].x, free.body[0].y], [5, 6]);
});

test("linefeed: food never lands on the worm or on food, the free cells are counted, a full grid spawns none", () => {
  let g = worm(6);
  for (let i = 0; i < 300 && g.alive; i++) {
    g = Lf.step(Lf.apply(g, ["up", "right", "down", "left"][Math.floor(i / 7) % 4]));
    for (const f of g.food) {
      assert.equal(g.body.some((b) => b.x === f.x && b.y === f.y), false);
      assert.equal(g.food.filter((o) => o.x === f.x && o.y === f.y).length, 1);
    }
  }
  const fresh = worm(6);
  assert.equal(Lf.freeCells(fresh).length, Lf.COLS * Lf.ROWS - fresh.body.length - fresh.food.length);
  const all = [];
  for (let y = 0; y < Lf.ROWS; y++) for (let x = 0; x < Lf.COLS; x++) all.push(seg(x, y));
  all[0] = seg(0, 0, "█", "head");
  const packed = { ...fresh, body: all, food: [] };
  assert.deepEqual(plain(Lf.spawnFood(packed).food), []);
  const oneFree = { ...fresh, body: all.slice(1), food: [] };
  const spawned = Lf.spawnFood(oneFree);
  assert.equal(spawned.food.length, 1);
  assert.deepEqual([spawned.food[0].x, spawned.food[0].y], [0, 0]);
});

test("linefeed: a closer that matches the latest opener balances for ten; a lone closer, or the wrong one, is nothing", () => {
  const g = worm(7);
  const a = Lf.eat(g, Lf.tokenOf("braceOpen"));
  assert.deepEqual(plain(a.open), ["{"]);
  assert.equal(a.bonus, null);
  const b = Lf.eat(a, Lf.tokenOf("braceClose"));
  assert.deepEqual(plain(b.open), []);
  assert.deepEqual(plain(b.bonus), { kind: "balanced", points: 10, at: 0 });
  assert.equal(b.score, 2 + 4 + 10);
  assert.equal(Lf.words(b), "balanced +10");
  const lone = Lf.eat(g, Lf.tokenOf("braceClose"));
  assert.deepEqual(plain(lone.open), []);
  assert.equal(lone.bonus, null);
  assert.equal(lone.score, 2);
  const wrong = Lf.eat(Lf.eat(g, Lf.tokenOf("parenOpen")), Lf.tokenOf("braceClose"));
  assert.deepEqual(plain(wrong.open), ["("]);
  assert.equal(wrong.bonus, null);
});

test("linefeed: a semicolon banks two a segment since the last one, and a second semicolon banks nothing", () => {
  let g = worm(8);
  for (let i = 0; i < 3; i++) g = Lf.eat(g, Lf.tokenOf("assign"));
  assert.equal(g.sinceSemi, 3);
  const s = Lf.eat(g, Lf.tokenOf("semi"));
  assert.deepEqual(plain(s.bonus), { kind: "statement", points: 6, at: 0 });
  assert.equal(s.sinceSemi, 0);
  assert.equal(Lf.words(s), "statement +6");
  assert.equal(Lf.eat(s, Lf.tokenOf("semi")).bonus, null);
  const braced = Lf.eat(Lf.eat(g, Lf.tokenOf("braceOpen")), Lf.tokenOf("semi"));
  assert.deepEqual(plain(braced.open), ["{"], "a brace outlives a statement");
});

test("linefeed: a streak grows by class and multiplies to four; a comment ends it", () => {
  let g = worm(9);
  const h = Lf.eat(g, Lf.tokenOf("hash"));
  assert.equal(h.streak, 0);
  assert.equal(h.score, 1);
  assert.equal(Lf.eat(h, Lf.tokenOf("assign")).streak, 1);
  let k = g;
  const scores = [];
  for (let i = 0; i < 5; i++) { k = Lf.eat(k, Lf.tokenOf(i % 2 ? "braceClose" : "braceOpen")); scores.push(k.score); }
  assert.deepEqual(scores, [2, 2 + 4 + 10, 16 + 6, 22 + 8 + 10, 40 + 8]);
  assert.equal(k.streak, 5);
  assert.equal(Lf.words(k), "streak ×4");
});

test("linefeed: the step shortens with length down to a floor, and the interval follows a bite", () => {
  assert.equal(Lf.intervalFor(3), 240);
  assert.equal(Lf.intervalFor(8), 200);
  assert.equal(Lf.intervalFor(36), 90);
  assert.equal(Lf.intervalFor(400), 90);
  for (let n = 1; n < 400; n++) assert.ok(Lf.intervalFor(n) <= Lf.intervalFor(n - 1));
  assert.equal(Lf.SPEEDS[Lf.SPEEDS.length - 1].ms, 90);
  let g = { ...worm(10), food: [foodAt(13, 8, "assign")] };
  for (let i = 0; i < 6; i++) g = Lf.step({ ...g, food: [foodAt(g.body[0].x + 1, 8, "assign")] });
  assert.equal(g.body.length, 9);
  assert.equal(g.interval, Lf.intervalFor(9));
});

test("linefeed: time accumulates into steps, two at most a tick, never while paused or dead", () => {
  const g = worm(11);
  const a = Lf.tick(g, 100);
  assert.equal(a.ticks, 0);
  assert.equal(a.acc, 100);
  assert.equal(Lf.tick(g, 240).ticks, 1);
  assert.equal(Lf.tick(g, 5000).ticks, 1, "a slow frame is cut to a quarter second");
  assert.equal(Lf.tick({ ...g, acc: 240 }, 240).ticks, 2);
  assert.equal(Lf.tick({ ...g, acc: 480 }, 480).ticks, 2, "never more than two");
  const p = Lf.apply(g, "pause");
  assert.equal(p.paused, true);
  assert.equal(Lf.tick(p, 1000), p);
  assert.equal(Lf.apply(p, "pause"), p);
  assert.equal(Lf.apply(p, "resume").paused, false);
  assert.equal(Lf.apply(g, "resume"), g);
  assert.equal(Lf.tick(g, 0), g);
});

test("linefeed: the view is the grid with the head, the dimmed body, bright food and blanks", () => {
  const g = worm(12);
  const v = plain(Lf.view(g));
  assert.equal(v.length, Lf.ROWS);
  assert.equal(v[0].length, Lf.COLS);
  assert.deepEqual(v[8][12], { ch: "█", cls: "head" });
  assert.deepEqual(v[8][11], { ch: ">", cls: "body cmt" });
  const f = g.food[0];
  assert.deepEqual(v[f.y][f.x], { ch: f.ch, cls: f.cls });
  const blank = v.flat().find((c) => c.ch === " ");
  assert.deepEqual(blank, { ch: " ", cls: "" });
  const dead = { ...g, alive: false, cause: "wall" };
  assert.equal(plain(Lf.view(dead))[8][12].cls, "head dead");
  assert.equal(Lf.words(g), "");
  assert.equal(Lf.words({ ...g, alive: false, cause: "full" }), "the grid is full — a win");
});

test("linefeed: the store is read as far as its shape allows, and the tables are pinned", () => {
  assert.deepEqual(plain(Lf.readSaved("junk")), { best: 0, muted: false });
  assert.deepEqual(plain(Lf.readSaved('{"best":12.9,"muted":true}')), { best: 12, muted: true });
  assert.deepEqual(plain(Lf.readSaved('{"best":-5,"muted":"yes"}')), { best: 0, muted: false });
  assert.deepEqual(plain(Lf.readSaved("[1]")), { best: 0, muted: false });
  assert.equal(worm(1, { best: 40 }).best, 40);
  assert.deepEqual(plain(Lf.TOKEN_NAMES), ["lambda", "semi", "braceOpen", "braceClose", "parenOpen", "parenClose", "bracketOpen", "bracketClose", "assign", "digit", "quote", "hash"]);
  assert.deepEqual(plain(Lf.ACTION_NAMES), ["up", "down", "left", "right", "pause", "resume"]);
  for (const name of Lf.TOKEN_NAMES) {
    const t = Lf.TOKENS[name];
    assert.ok((t.ch && t.ch.length === 1) || (t.glyphs && t.glyphs.length > 1), name);
    assert.ok(["kw", "str", "num", "fn", "type", "cmt", "op"].includes(t.cls), name);
    assert.ok(t.points >= 1 && t.weight >= 1, name);
  }
  assert.deepEqual(plain(Lf.tokenOf("digit", "7")), { ch: "7", cls: "num", points: 3, kind: "digit" });
  assert.equal(Lf.COLS, 24);
  assert.equal(Lf.ROWS, 16);
});

// --- linerun ---------------------------------------------------------------------

const Rn = logicOf("linerun").Linerun;
const run = (seed = 7, saved) => Rn.newGame(seed, saved);
/** A run over a fixed file: these entities, no chunks ever laid down. */
const withWorld = (game, entities) => ({ ...game, entities: entities.map((e, i) => ({ id: i + 1, done: false, near: false, ...e })), cursor: 1e6 });
const advance = (game, cells) => {
  let g = game;
  for (let i = 0; i < 4000 && g.alive && g.distance < cells; i++) g = Rn.tick(g, 16);
  return g;
};
const bugAt = (x, lane = 1, kind = "bug") => ({ kind, x, lane });

test("linerun: the same seed and the same keys replay the same run, and seeds differ", () => {
  const script = { 30: "jump", 100: "up", 160: "down", 400: "jump", 700: "up", 1200: "down", 1500: "jump" };
  const play = (seed) => {
    let g = run(seed);
    for (let i = 0; i < 2000 && g.alive; i++) {
      if (script[i]) g = Rn.apply(g, script[i]);
      g = Rn.tick(g, 16);
    }
    return g;
  };
  assert.deepEqual(plain(play(7)), plain(play(7)));
  assert.notDeepEqual(plain(run(7).entities), plain(run(8).entities));
  assert.equal(play(7).seed, 7);
});

test("linerun: every chunk can be run by the game's own rules, taking every token, with a gap a hop can land in", () => {
  for (const name of Rn.CHUNK_NAMES) {
    const chunk = Rn.CHUNKS[name];
    assert.ok(Rn.pathThrough(chunk) !== null, `${name} has a way through`);
    assert.ok(chunk.gap >= Rn.JUMP + 1, name);
    assert.ok([1, 2, 3].includes(chunk.tier), name);
    for (const c of chunk.cells) assert.ok(Rn.KIND_NAMES.includes(c.kind) && c.lane >= 0 && c.lane + Rn.KINDS[c.kind].span <= Rn.LANES, name);
  }
  assert.equal(Rn.pathThrough({ tier: 1, gap: 8, cells: [{ dx: 0, lane: 0, kind: "todo" }, { dx: 0, lane: 1, kind: "todo" }, { dx: 0, lane: 2, kind: "todo" }] }), null, "three walls are no way");
  assert.equal(Rn.pathThrough({ tier: 1, gap: 8, cells: [{ dx: 0, lane: 0, kind: "wall" }, { dx: 0, lane: 2, kind: "coin" }, { dx: 0, lane: 1, kind: "coin" }] }), null, "a token under a wall is never taken");
});

test("linerun: a generated file never blocks every lane with what cannot be hopped, puts one token a column, gates its tiers by distance and spaces its chunks", () => {
  for (let seed = 1; seed <= 20; seed++) {
    const g = Rn.generate(run(seed), 3000);
    const byCol = new Map();
    for (const e of g.entities) {
      const k = Rn.KINDS[e.kind];
      for (let x = e.x; x < e.x + k.width; x++) {
        const col = byCol.get(x) ?? { solid: 0, tokens: new Set() };
        for (let l = e.lane; l < e.lane + k.span; l++) if (k.blocks && !k.jumpable) col.solid += 1;
        if (k.collect) col.tokens.add(e.lane);
        byCol.set(x, col);
      }
      if (e.kind === "nan") assert.ok(e.x >= 400, `seed ${seed}: NaN before 400`);
      if (Rn.CHUNKS["wall-top"].cells.some((c) => c.kind === e.kind && e.kind === "wall")) assert.ok(e.x >= 150, `seed ${seed}: a wall before 150`);
    }
    for (const [x, col] of byCol) {
      assert.ok(col.solid <= 2, `seed ${seed}: column ${x} walled across`);
      assert.ok(col.tokens.size <= 1, `seed ${seed}: two tokens at column ${x}`);
    }
    const sorted = g.entities.slice().sort((a, b) => a.x - b.x);
    assert.ok(sorted[0].x >= Rn.RUNWAY);
  }
});

test("linerun: a bug on the lane crashes the run at its column; a hop clears it and landing right after it pays a near miss", () => {
  const g = withWorld(run(1), [bugAt(7)]);
  const hit = advance(g, 2);
  assert.equal(hit.alive, false);
  assert.deepEqual(plain(hit.crash), { kind: "bug", x: 7, lane: 1 });
  assert.equal(hit.distance, 1);
  assert.equal(hit.streak, 0);
  assert.equal(hit.log[hit.log.length - 1].cls, "err");
  assert.equal(Rn.statusLine(hit), "crash: ✗ at 1");
  assert.equal(Rn.apply(hit, "up"), hit);
  assert.equal(Rn.tick(hit, 100), hit);
  const hop = advance(Rn.apply(g, "jump"), 6);
  assert.equal(hop.alive, true);
  assert.equal(hop.runner.air, 0);
  assert.equal(hop.runner.landAt, 11);
  const late = advance(Rn.apply(withWorld(run(1), [bugAt(10)]), "jump"), 6);
  assert.equal(late.alive, true);
  assert.equal(late.score, 25);
  assert.equal(late.log[late.log.length - 1].text, "close! +25");
  assert.equal(Rn.apply(Rn.apply(g, "jump"), "jump").runner.air, Rn.JUMP, "a hop in the air is refused");
});

test("linerun: TODO cannot be hopped, a comment wall blocks two lanes and not the third", () => {
  const todo = advance(Rn.apply(withWorld(run(2), [bugAt(7, 1, "todo")]), "jump"), 6);
  assert.equal(todo.alive, false);
  assert.equal(todo.crash.kind, "todo");
  assert.equal(todo.distance, 1);
  const wallMid = advance(Rn.apply(withWorld(run(2), [bugAt(7, 0, "wall")]), "jump"), 6);
  assert.equal(wallMid.crash.kind, "wall");
  const below = advance(Rn.apply(withWorld(run(2), [bugAt(7, 0, "wall")]), "down"), 12);
  assert.equal(below.alive, true);
  assert.equal(below.runner.lane, 2);
  assert.equal(Rn.apply(below, "down"), below, "the bottom lane has no lane below");
});

test("linerun: tokens pay by the streak's multiplier, a token left behind ends the streak, a crash too", () => {
  const coins = [7, 8, 9, 10, 11, 12].map((x) => bugAt(x, 1, "coin"));
  const g = advance(withWorld(run(3), coins), 8);
  assert.equal(g.streak, 6);
  assert.equal(g.score, 5 * 10 + 20);
  assert.equal(Rn.multiplier(0), 1);
  assert.equal(Rn.multiplier(5), 2);
  assert.equal(Rn.multiplier(99), 5);
  const missed = advance(withWorld(run(3), [bugAt(7, 1, "coin"), bugAt(9, 0, "coin")]), 5);
  assert.equal(missed.streak, 0);
  assert.ok(missed.log.some((l) => l.text === "missed $"));
  assert.equal(missed.score, 10);
  const crashed = advance(withWorld(run(3), [bugAt(7, 1, "coin"), bugAt(8, 1, "coin"), bugAt(10)]), 6);
  assert.equal(crashed.alive, false);
  assert.equal(crashed.streak, 0);
  assert.equal(crashed.score, 20);
});

test("linerun: passing a bug on the lane beside pays a near miss, once", () => {
  const g = advance(withWorld(run(4), [bugAt(7, 0)]), 3);
  assert.equal(g.score, 25);
  assert.equal(g.streak, 0);
  assert.equal(g.entities[0].near, true);
  assert.equal(g.entities[0].done, false);
  const back = Rn.apply(Rn.apply(g, "down"), "up");
  assert.equal(back.score, 25);
  const into = Rn.apply(advance(withWorld(run(4), [bugAt(7, 0)]), 1), "up");
  assert.equal(into.alive, false, "a move into a bug is a crash");
});

test("linerun: a bang doubles the points and lifts the speed for forty cells, then wears off", () => {
  const g = advance(withWorld(run(5), [bugAt(7, 1, "bang"), bugAt(8, 1, "coin")]), 2);
  assert.equal(g.burst, Rn.BURST.cells - 1);
  assert.equal(g.score, 20);
  assert.equal(Rn.effectiveSpeed(g), g.speed * Rn.BURST.speed);
  assert.ok(g.log.some((l) => l.text.indexOf("bang!") === 0));
  assert.ok(Rn.statusLine(g).indexOf("· ! ") > 0);
  const later = advance(g, 50);
  assert.equal(later.burst, 0);
  assert.equal(Rn.effectiveSpeed(later), later.speed);
});

test("linerun: the speed climbs with the distance to a cap", () => {
  assert.equal(Rn.speedAt(0), 8);
  assert.equal(Rn.speedAt(500), 14);
  assert.equal(Rn.speedAt(1000), 20);
  assert.equal(Rn.speedAt(9999), 20);
  const far = advance(withWorld(run(6), []), 500);
  assert.equal(far.speed, 14);
  assert.ok(far.distance >= 500);
  assert.deepEqual(plain(Rn.tierWeights(0)), [1, 0, 0]);
  assert.deepEqual(plain(Rn.tierWeights(800)), [1, 1, 3]);
});

test("linerun: the view is three lines of the file with their numbers, the runner, an obstacle at its column", () => {
  const g = withWorld(run(7), [bugAt(7)]);
  const v = plain(Rn.view(g));
  assert.equal(v.length, Rn.LANES);
  v.forEach((row) => assert.equal(row.length, Rn.GUTTER + Rn.COLS));
  assert.deepEqual(v[1][Rn.GUTTER + Rn.RUNNER_X], { ch: "λ", cls: "fn" });
  assert.equal(plain(Rn.view(Rn.apply(g, "jump")))[1][Rn.GUTTER + Rn.RUNNER_X].ch, "Λ");
  assert.equal(v[0].slice(0, Rn.GUTTER).map((c) => c.ch).join(""), " 1 │ ");
  assert.equal(v[1][1].cls, "value");
  assert.equal(v[0][1].cls, "gutter");
  assert.deepEqual(v[1][Rn.GUTTER + 7], { ch: "✗", cls: "err" });
  for (const name of Rn.KIND_NAMES) for (const line of Rn.KINDS[name].text) assert.equal(line.length, Rn.KINDS[name].width, name);
  const dead = advance(g, 2);
  assert.equal(plain(Rn.view(dead))[1][Rn.GUTTER + Rn.RUNNER_X].cls, "err");
  const wall = plain(Rn.view(withWorld(run(7), [bugAt(8, 0, "wall")])));
  assert.equal(wall[0].slice(Rn.GUTTER + 8, Rn.GUTTER + 11).map((c) => c.ch).join(""), "/**");
  assert.equal(wall[1].slice(Rn.GUTTER + 8, Rn.GUTTER + 11).map((c) => c.ch).join(""), "**/");
});

test("linerun: the best is marked in the file when on screen, not beyond it, and passing it is logged once", () => {
  const saved = { best: { distance: 20, score: 0, seed: 1 }, muted: false };
  const g = withWorld(run(1, saved), []);
  const v = plain(Rn.view(g));
  for (let lane = 0; lane < Rn.LANES; lane++) assert.deepEqual(v[lane][Rn.GUTTER + Rn.RUNNER_X + 20], { ch: "┊", cls: "cmt" });
  const far = plain(Rn.view(withWorld(run(1, { best: { distance: 100, score: 0, seed: 1 }, muted: false }), [])));
  assert.ok(!far.flat().some((c) => c.ch === "┊"));
  const past = advance(g, 25);
  assert.equal(past.log.filter((l) => l.text === "past your best").length, 1);
  assert.deepEqual(plain(Rn.bestOf(past)), { distance: past.distance, score: 0, seed: 1 });
  assert.deepEqual(plain(Rn.bestOf(withWorld(run(2, saved), []))), { distance: 20, score: 0, seed: 1 });
});

test("linerun: the store is read as far as its shape allows, and the tables are pinned", () => {
  assert.deepEqual(plain(Rn.readSaved("junk")), { best: { distance: 0, score: 0, seed: 0 }, muted: false });
  assert.deepEqual(plain(Rn.readSaved('{"best":{"distance":120.7,"score":30,"seed":5},"muted":true}')), { best: { distance: 120, score: 30, seed: 5 }, muted: true });
  assert.deepEqual(plain(Rn.readSaved('{"best":{"distance":-1,"score":0,"seed":0}}')).best, { distance: 0, score: 0, seed: 0 });
  assert.deepEqual(plain(Rn.readSaved("[1]")), { best: { distance: 0, score: 0, seed: 0 }, muted: false });
  assert.deepEqual(plain(Rn.readSaved('{"best":"x"}')), { best: { distance: 0, score: 0, seed: 0 }, muted: false });
  assert.deepEqual(plain(Rn.KIND_NAMES), ["bug", "nan", "todo", "wall", "coin", "gem", "bang"]);
  assert.deepEqual(plain(Rn.ACTION_NAMES), ["up", "down", "jump", "pause", "resume"]);
  assert.equal(Rn.COLS, 48);
  assert.equal(Rn.LANES, 3);
  assert.equal(Rn.JUMP, 4);
  assert.equal(Rn.CHUNK_NAMES.length, 9);
});

test("linerun: a pause freezes the file, a hop in the air and a lane past the edge are nothing, a slow frame moves a tenth of a second at most", () => {
  const g = run(9);
  const p = Rn.apply(g, "pause");
  assert.equal(Rn.tick(p, 16), p);
  assert.equal(Rn.apply(p, "jump"), p);
  assert.equal(Rn.apply(p, "resume").paused, false);
  assert.equal(Rn.apply(g, "resume"), g);
  assert.equal(Rn.apply({ ...g, runner: { lane: 0, air: 0, landAt: -1 } }, "up").runner.lane, 0);
  const slow = Rn.tick(withWorld(g, []), 5000);
  assert.ok(slow.offset <= 0.1 * g.speed + 1e-9);
  assert.equal(Rn.tick(g, 0), g);
  assert.equal(Rn.apply(g, "dance"), g);
});
