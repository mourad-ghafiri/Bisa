// The game's rules, pure: who won, which cells are free, and the machine's
// move at either level. Loaded before `main.js`; exposed as one global so a
// Node test can run this file alone.
(function (root) {
  "use strict";
  var LINES = [[0, 1, 2], [3, 4, 5], [6, 7, 8], [0, 3, 6], [1, 4, 7], [2, 5, 8], [0, 4, 8], [2, 4, 6]];

  /** `{who, line}` for three in a row, else null. `cells` is nine marks or blanks. */
  function winner(cells) {
    for (var i = 0; i < LINES.length; i++) {
      var l = LINES[i];
      if (cells[l[0]] && cells[l[0]] === cells[l[1]] && cells[l[1]] === cells[l[2]]) return { who: cells[l[0]], line: l };
    }
    return null;
  }
  function emptyCells(cells) {
    var out = [];
    for (var i = 0; i < 9; i++) if (!cells[i]) out.push(i);
    return out;
  }
  function full(cells) { return emptyCells(cells).length === 0; }
  function other(mark) { return mark === "X" ? "O" : "X"; }

  // Minimax with alpha–beta over at most nine cells: a win sooner scores
  // higher, a loss later hurts less, so the machine finishes and delays.
  function score(cells, me, turn, depth, alpha, beta) {
    var w = winner(cells);
    if (w) return w.who === me ? 10 - depth : depth - 10;
    var free = emptyCells(cells);
    if (free.length === 0) return 0;
    var mine = turn === me;
    var best = mine ? -Infinity : Infinity;
    for (var i = 0; i < free.length; i++) {
      cells[free[i]] = turn;
      var s = score(cells, me, other(turn), depth + 1, alpha, beta);
      cells[free[i]] = "";
      if (mine) { if (s > best) best = s; if (best > alpha) alpha = best; }
      else { if (s < best) best = s; if (best < beta) beta = best; }
      if (alpha >= beta) break;
    }
    return best;
  }
  /** The cell that cannot lose: the best score, the first such cell in reading order. */
  function unbeatable(cells, me) {
    var free = emptyCells(cells);
    var bestCell = -1, bestScore = -Infinity;
    for (var i = 0; i < free.length; i++) {
      cells[free[i]] = me;
      var s = score(cells, me, other(me), 1, -Infinity, Infinity);
      cells[free[i]] = "";
      if (s > bestScore) { bestScore = s; bestCell = free[i]; }
    }
    return bestCell;
  }
  /** A legal cell at random; `random` is a function in [0, 1). */
  function easy(cells, random) {
    var free = emptyCells(cells);
    if (free.length === 0) return -1;
    return free[Math.min(free.length - 1, Math.floor(random() * free.length))];
  }
  /**
   * The machine's move for `me` on `cells`: -1 on a finished or full board.
   * `level` is "easy" or "unbeatable"; `random` serves the easy level.
   */
  function bestMove(cells, me, level, random) {
    if (winner(cells) || full(cells)) return -1;
    var copy = cells.slice();
    for (var i = 0; i < 9; i++) if (!copy[i]) copy[i] = "";
    return level === "unbeatable" ? unbeatable(copy, me) : easy(copy, random || Math.random);
  }

  root.TicTacToe = { LINES: LINES, winner: winner, emptyCells: emptyCells, full: full, other: other, bestMove: bestMove };
})(this);
