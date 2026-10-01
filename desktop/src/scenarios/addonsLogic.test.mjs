/**
 * The pure half of two built-ins — tic-tac-toe's rules and machine, the
 * sticky note's sanitiser — each a `logic.js` the page loads before its
 * `main.js`, run here alone in a `vm` (the `addonSdk.test.mjs` recipe): the
 * machine never loses, easy plays a legal cell, and a note keeps its
 * formatting and nothing else.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

function logicOf(slug) {
  const source = readFileSync(new URL(`../../../library/addons/${slug}/logic.js`, import.meta.url), "utf8");
  const ctx = { Math, Array, Object, String, JSON, TextEncoder, Infinity };
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
