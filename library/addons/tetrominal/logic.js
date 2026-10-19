// Tetrominal's rules, with no page in them: the well and its hidden spawn
// rows, the seven pieces and their turns, gravity and the lock, the lines
// and the score, the compile log, the seed. `main.js` draws;
// `desktop/src/scenarios/addonsLogic.test.mjs` runs this alone, so the same
// seed and the same keys replay the same game. Every function returns a new
// game, or the very same object when nothing changed.
(function (global) {
  "use strict";

  var COLS = 10, ROWS = 20, HIDDEN = 2, ROWS_ALL = ROWS + HIDDEN;
  var LOCK_MS = 500, LOCK_RESETS = 15, MAX_DT = 1000, LOG_MAX = 6;
  /** Milliseconds a piece takes to fall one row, by level; past the table, the last. */
  var SPEEDS = [1000, 800, 630, 500, 400, 300, 230, 170, 130, 100, 80, 65, 50, 40, 30];
  var LINE_SCORES = [0, 100, 300, 500, 800];
  var SPAWN = { rot: 0, x: 3, y: 1 };
  var LOG_KINDS = ["start", "clear", "quad", "combo", "level", "over"];
  var ACTION_NAMES = ["left", "right", "rotateCw", "rotateCcw", "softDrop", "hardDrop", "hold", "pause", "resume"];

  // --- random: the state travels in the game, so a seed replays ----------------
  /** One step of mulberry32: the next state and a value in [0, 1). */
  function rngStep(state) {
    var s = ((state >>> 0) + 0x6d2b79f5) >>> 0;
    var t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return { state: s, value: ((t ^ (t >>> 14)) >>> 0) / 4294967296 };
  }

  // --- the pieces -----------------------------------------------------------------
  /** Each piece as string art, four turns each; `O` turns into itself. */
  var ART = {
    I: { box: 4, shapes: [["....", "####", "....", "...."], ["..#.", "..#.", "..#.", "..#."], ["....", "....", "####", "...."], [".#..", ".#..", ".#..", ".#.."]] },
    O: { box: 3, shapes: [[".##", ".##", "..."], [".##", ".##", "..."], [".##", ".##", "..."], [".##", ".##", "..."]] },
    T: { box: 3, shapes: [[".#.", "###", "..."], [".#.", ".##", ".#."], ["...", "###", ".#."], [".#.", "##.", ".#."]] },
    S: { box: 3, shapes: [[".##", "##.", "..."], [".#.", ".##", "..#"], ["...", ".##", "##."], ["#..", "##.", ".#."]] },
    Z: { box: 3, shapes: [["##.", ".##", "..."], ["..#", ".##", ".#."], ["...", "##.", ".##"], [".#.", "##.", "#.."]] },
    J: { box: 3, shapes: [["#..", "###", "..."], [".##", ".#.", ".#."], ["...", "###", "..#"], [".#.", ".#.", "##."]] },
    L: { box: 3, shapes: [["..#", "###", "..."], [".#.", ".#.", ".##"], ["...", "###", "#.."], ["##.", ".#.", ".#."]] },
  };
  var PIECE_NAMES = Object.keys(ART);
  /** The cells of one drawing: `[x, y]` for every `#`. */
  function shapeFrom(rows) {
    var cells = [];
    for (var y = 0; y < rows.length; y++) for (var x = 0; x < rows[y].length; x++) if (rows[y].charAt(x) === "#") cells.push([x, y]);
    return cells;
  }
  var PIECES = {};
  PIECE_NAMES.forEach(function (name) {
    PIECES[name] = { box: ART[name].box, shapes: ART[name].shapes.map(shapeFrom) };
  });
  /** A piece wears a syntax role, and never the colour the arcade gave it: I was cyan, O yellow, T purple, S green, Z red, J blue, L orange. */
  var ROLES = { I: "cmt", O: "type", T: "str", S: "num", Z: "kw", J: "fn", L: "op" };
  /** The nudges a turn may try, in order, until one fits: the long piece reaches two across. */
  var KICKS = {
    I: [[0, 0], [-1, 0], [1, 0], [-2, 0], [2, 0], [0, -1], [0, -2]],
    O: [[0, 0]],
    other: [[0, 0], [-1, 0], [1, 0], [0, -1], [-2, 0], [2, 0]],
  };

  function shapeOf(name, rot) { return PIECES[name].shapes[((rot % 4) + 4) % 4]; }
  function cellsOf(piece) {
    return shapeOf(piece.name, piece.rot).map(function (c) { return [piece.x + c[0], piece.y + c[1]]; });
  }
  /** Inside the well and on empty cells. */
  function fits(cells, piece) {
    var at = cellsOf(piece);
    for (var i = 0; i < at.length; i++) {
      var x = at[i][0], y = at[i][1];
      if (x < 0 || x >= COLS || y < 0 || y >= ROWS_ALL || cells[y][x] !== 0) return false;
    }
    return true;
  }
  function shifted(piece, dx, dy, rot) {
    return { name: piece.name, rot: rot === undefined ? piece.rot : rot, x: piece.x + dx, y: piece.y + dy };
  }
  /** A bag: every piece once, shuffled by the game's random. */
  function shuffledBag(state) {
    var bag = PIECE_NAMES.slice(), s = state;
    for (var i = bag.length - 1; i > 0; i--) {
      var r = rngStep(s);
      s = r.state;
      var j = Math.floor(r.value * (i + 1));
      var t = bag[i]; bag[i] = bag[j]; bag[j] = t;
    }
    return { bag: bag, state: s };
  }

  // --- the game -----------------------------------------------------------------------
  function assign(game, patch) {
    var out = {}, k;
    for (k in game) out[k] = game[k];
    for (k in patch) out[k] = patch[k];
    return out;
  }
  function emptyCells() {
    var cells = [];
    for (var y = 0; y < ROWS_ALL; y++) { var row = []; for (var x = 0; x < COLS; x++) row.push(0); cells.push(row); }
    return cells;
  }
  function copyCells(cells) { return cells.map(function (row) { return row.slice(); }); }
  function pushLog(game, kind, text) {
    var log = game.log.concat([{ kind: kind, text: text }]);
    if (log.length > LOG_MAX) log = log.slice(log.length - LOG_MAX);
    return assign(game, { log: log });
  }
  function speedOf(level) { return SPEEDS[Math.max(1, Math.min(SPEEDS.length, level)) - 1]; }
  function levelOf(lines, startLevel) { return startLevel + Math.floor(lines / 10); }
  /** What a clear is worth: the count by level, a second quad in a row half again, a combo on top. */
  function clearScore(count, level, backToBack, combo) {
    var points = LINE_SCORES[count] * level;
    if (backToBack && count === 4) points = Math.floor(points * 1.5);
    if (combo > 0) points += 50 * combo * level;
    return points;
  }
  /** Full rows gone, the rows above shifted down, the letters kept. The input is left as it was. */
  function clearLines(cells) {
    var kept = cells.filter(function (row) { return row.some(function (c) { return c === 0; }); });
    var count = cells.length - kept.length;
    var fresh = [];
    for (var i = 0; i < count; i++) { var row = []; for (var x = 0; x < COLS; x++) row.push(0); fresh.push(row); }
    return { cells: fresh.concat(kept.map(function (row) { return row.slice(); })), count: count };
  }
  /** Rows from the floor to the highest settled cell, visible rows only. */
  function stackHeight(cells) {
    for (var y = HIDDEN; y < ROWS_ALL; y++) {
      if (cells[y].some(function (c) { return c !== 0; })) return ROWS_ALL - y;
    }
    return 0;
  }
  function danger(game) { return stackHeight(game.cells) >= ROWS - 5; }
  /** The queue keeps a bag and a half ahead, so the next three always show. */
  function refill(game) {
    var queue = game.queue, state = game.rngState;
    while (queue.length < 4) {
      var r = shuffledBag(state);
      queue = queue.concat(r.bag);
      state = r.state;
    }
    return queue === game.queue ? game : assign(game, { queue: queue, rngState: state });
  }
  /** A piece enters at the top; one that cannot ends the build. */
  function spawn(game, name) {
    var piece = { name: name, rot: SPAWN.rot, x: SPAWN.x, y: SPAWN.y };
    var next = assign(game, { piece: piece, fallMs: 0, lockMs: 0, lockResets: 0, lowY: SPAWN.y });
    if (fits(game.cells, piece)) return next;
    return pushLog(assign(next, { over: true }), "over", "✗ build failed: stack overflow");
  }
  function takeNext(game) {
    var name = game.queue[0];
    return spawn(refill(assign(game, { queue: game.queue.slice(1) })), name);
  }
  function newGame(seed, options) {
    var s = (seed >>> 0) || 1;
    var level = options && typeof options.level === "number" && options.level >= 1 ? Math.floor(options.level) : 1;
    var game = {
      seed: s, rngState: s, startLevel: level,
      cells: emptyCells(), piece: null, queue: [], hold: null, canHold: true,
      fallMs: 0, lockMs: 0, lockResets: 0, lowY: SPAWN.y,
      score: 0, lines: 0, level: level, pieces: 0, ms: 0, combo: -1, lastQuad: false,
      paused: false, over: false, log: [],
    };
    game = pushLog(refill(game), "start", "$ tetrominal --seed " + s);
    return takeNext(game);
  }

  function canFall(game) { return fits(game.cells, shifted(game.piece, 0, 1)); }
  function ghostRow(game) {
    var piece = game.piece;
    while (fits(game.cells, shifted(piece, 0, 1))) piece = shifted(piece, 0, 1);
    return piece.y;
  }
  /** A piece moved or turned: on the floor, the lock is given another moment, a limited number of times. */
  function settle(game, piece) {
    var next = assign(game, { piece: piece });
    if (canFall(next)) return assign(next, { lockMs: 0 });
    if (game.lockResets >= LOCK_RESETS) return next;
    return assign(next, { lockMs: 0, lockResets: game.lockResets + 1 });
  }
  function move(game, dx) {
    var piece = shifted(game.piece, dx, 0);
    return fits(game.cells, piece) ? settle(game, piece) : game;
  }
  function rotate(game, dir) {
    var rot = (((game.piece.rot + dir) % 4) + 4) % 4;
    var kicks = KICKS[game.piece.name] || KICKS.other;
    for (var i = 0; i < kicks.length; i++) {
      var piece = shifted(game.piece, kicks[i][0], kicks[i][1], rot);
      if (fits(game.cells, piece)) return settle(game, piece);
    }
    return game;
  }
  function softDrop(game) {
    if (!canFall(game)) return game;
    return assign(game, { piece: shifted(game.piece, 0, 1), score: game.score + 1, fallMs: 0 });
  }
  /** The piece settles into the well: the lines, the score, the log, the next piece. */
  function lockPiece(game) {
    var cells = copyCells(game.cells), hiddenOnly = true;
    cellsOf(game.piece).forEach(function (c) {
      cells[c[1]][c[0]] = game.piece.name;
      if (c[1] >= HIDDEN) hiddenOnly = false;
    });
    var next = assign(game, { cells: cells, pieces: game.pieces + 1, canHold: true, fallMs: 0, lockMs: 0, lockResets: 0 });
    if (hiddenOnly) return pushLog(assign(next, { over: true }), "over", "✗ build failed: stack overflow");
    var cleared = clearLines(cells);
    if (cleared.count === 0) return takeNext(assign(next, { combo: -1 }));
    var combo = next.combo + 1, quad = cleared.count === 4, backToBack = quad && next.lastQuad;
    var points = clearScore(cleared.count, next.level, next.lastQuad, combo);
    var lines = next.lines + cleared.count, level = levelOf(lines, next.startLevel);
    next = assign(next, { cells: cleared.cells, score: next.score + points, lines: lines, combo: combo, lastQuad: quad });
    next = pushLog(next, quad ? "quad" : "clear", "✓ " + cleared.count + (cleared.count === 1 ? " line" : " lines") + " compiled" + (backToBack ? " — back-to-back quad" : quad ? " — quad" : ""));
    if (combo > 0) next = pushLog(next, "combo", "  combo ×" + (combo + 1));
    if (level !== next.level) next = pushLog(assign(next, { level: level }), "level", "→ level " + level + " · " + speedOf(level) + " ms");
    return takeNext(next);
  }
  function hardDrop(game) {
    var y = ghostRow(game);
    return lockPiece(assign(game, { piece: shifted(game.piece, 0, y - game.piece.y), score: game.score + 2 * (y - game.piece.y) }));
  }
  /** The piece goes to the slot and the slot's piece, or the next, comes in — once per piece. */
  function hold(game) {
    if (!game.canHold) return game;
    var kept = assign(game, { hold: game.piece.name, canHold: false });
    return game.hold ? spawn(kept, game.hold) : takeNext(kept);
  }
  var ACTIONS = {
    left: function (g) { return move(g, -1); },
    right: function (g) { return move(g, 1); },
    rotateCw: function (g) { return rotate(g, 1); },
    rotateCcw: function (g) { return rotate(g, -1); },
    softDrop: softDrop,
    hardDrop: hardDrop,
    hold: hold,
    pause: function (g) { return assign(g, { paused: true }); },
    resume: function (g) { return g; },
  };
  function isAction(name) { return typeof name === "string" && Object.prototype.hasOwnProperty.call(ACTIONS, name); }
  /** One key: a new game, or the same object when the key did nothing. */
  function apply(game, action) {
    if (!isAction(action) || game.over) return game;
    if (game.paused) return action === "resume" ? assign(game, { paused: false }) : game;
    return ACTIONS[action](game);
  }
  /** Time passes: the piece falls as far as its level says; on the floor, the lock counts down. */
  function tick(game, dtMs) {
    if (game.over || game.paused || !(dtMs > 0)) return game;
    var dt = Math.min(dtMs, MAX_DT);
    var next = assign(game, { ms: game.ms + dt });
    if (canFall(next)) {
      var speed = speedOf(next.level), fall = next.fallMs + dt, piece = next.piece, lowY = next.lowY, resets = next.lockResets;
      while (fall >= speed && fits(next.cells, shifted(piece, 0, 1))) {
        fall -= speed;
        piece = shifted(piece, 0, 1);
        if (piece.y > lowY) { lowY = piece.y; resets = 0; }
      }
      return assign(next, { piece: piece, fallMs: fall, lowY: lowY, lockResets: resets, lockMs: 0 });
    }
    var lock = next.lockMs + dt;
    if (lock >= LOCK_MS) return lockPiece(assign(next, { lockMs: 0 }));
    return assign(next, { lockMs: lock });
  }

  // --- what the screen shows ---------------------------------------------------------
  function cell(ch, cls) { return { ch: ch, cls: cls }; }
  /** The visible well, row by row: settled cells, the piece, its ghost, and the gutter's line number. */
  function view(game, options) {
    var ghost = !(options && options.ghost === false) && game.piece && !game.over;
    var marks = {};
    if (game.piece) {
      if (ghost) {
        var gy = ghostRow(game);
        if (gy !== game.piece.y) cellsOf(shifted(game.piece, 0, gy - game.piece.y)).forEach(function (c) { marks[c[1] * COLS + c[0]] = cell("░░", "ghost"); });
      }
      cellsOf(game.piece).forEach(function (c) { marks[c[1] * COLS + c[0]] = cell("██", ROLES[game.piece.name]); });
    }
    var warn = danger(game), rows = [];
    for (var y = HIDDEN; y < ROWS_ALL; y++) {
      var cells = [];
      for (var x = 0; x < COLS; x++) {
        var settled = game.cells[y][x];
        cells.push(marks[y * COLS + x] || (settled ? cell("██", ROLES[settled]) : cell("  ", "empty")));
      }
      rows.push({ line: y - HIDDEN + 1, warn: warn, cells: cells });
    }
    return { rows: rows };
  }
  /** A piece in a 4 × 2 box, for the hold and next slots. */
  function preview(name) {
    var rows = [[], []], shape = shapeOf(name, 0);
    for (var y = 0; y < 2; y++) for (var x = 0; x < 4; x++) {
      var on = shape.some(function (c) { return c[0] === x && c[1] === y; });
      rows[y].push(on ? cell("██", ROLES[name]) : cell("  ", "empty"));
    }
    return rows;
  }
  /** What the store held, as far as its shape allows. */
  function readSaved(raw) {
    var out = { best: 0, muted: false };
    var parsed;
    try { parsed = JSON.parse(raw); } catch (e) { return out; }
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return out;
    if (typeof parsed.best === "number" && parsed.best >= 0 && parsed.best < 1e9) out.best = Math.floor(parsed.best);
    if (parsed.muted === true) out.muted = true;
    return out;
  }

  global.Tetrominal = {
    COLS: COLS, ROWS: ROWS, HIDDEN: HIDDEN, ROWS_ALL: ROWS_ALL,
    LOCK_MS: LOCK_MS, LOCK_RESETS: LOCK_RESETS, MAX_DT: MAX_DT, LOG_MAX: LOG_MAX,
    SPEEDS: SPEEDS, LINE_SCORES: LINE_SCORES, SPAWN: SPAWN,
    PIECES: PIECES, PIECE_NAMES: PIECE_NAMES, ROLES: ROLES, KICKS: KICKS,
    ACTION_NAMES: ACTION_NAMES, LOG_KINDS: LOG_KINDS,
    rngStep: rngStep, shuffledBag: shuffledBag, shapeOf: shapeOf, cellsOf: cellsOf, fits: fits,
    newGame: newGame, apply: apply, tick: tick, ghostRow: ghostRow, view: view, preview: preview,
    speedOf: speedOf, levelOf: levelOf, clearScore: clearScore, clearLines: clearLines, stackHeight: stackHeight, danger: danger,
    readSaved: readSaved,
  };
})(typeof window !== "undefined" ? window : this);
