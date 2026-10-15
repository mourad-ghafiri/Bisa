/**
 * The pure half of three built-ins — tic-tac-toe's rules and machine, the
 * sticky note's sanitiser, the media player's sources and pictures — each a
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
