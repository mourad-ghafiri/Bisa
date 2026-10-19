// Linefeed's rules, with no page in them: a worm on a grid that is the code
// it eats — every token becomes its neck, so the body reads as a line of
// code — the turns, the food, the brackets and statements that pay, the
// speed, the death. `main.js` draws; `desktop/src/scenarios/addonsLogic.test.mjs`
// runs this alone, so the same seed and the same turns replay the same game.
// Every function returns a new game, or the very same object when nothing changed.
(function (global) {
  "use strict";

  var COLS = 24, ROWS = 16, MAX_DT = 250, MAX_STEPS = 2, FOOD = 2;
  var DIRS = { up: [0, -1], down: [0, 1], left: [-1, 0], right: [1, 0] };
  var OPPOSITE = { up: "down", down: "up", left: "right", right: "left" };
  var ACTION_NAMES = ["up", "down", "left", "right", "pause", "resume"];
  /** The step, in milliseconds, by the worm's length; the last row is the floor. */
  var SPEEDS = [{ from: 0, ms: 240 }, { from: 8, ms: 200 }, { from: 14, ms: 170 }, { from: 20, ms: 140 }, { from: 28, ms: 115 }, { from: 36, ms: 90 }];
  var PAIRS = { "}": "{", ")": "(", "]": "[" };
  var OPENERS = { "{": true, "(": true, "[": true };
  var BONUS = { balanced: 10, perSegment: 2, streakCap: 4, toastTicks: 8 };
  /** The food: a token's glyph, its syntax class, its points, how often it comes. */
  var TOKENS = {
    lambda: { ch: "λ", cls: "fn", points: 5, weight: 1 },
    semi: { ch: ";", cls: "op", points: 1, weight: 4 },
    braceOpen: { ch: "{", cls: "kw", points: 2, weight: 3 },
    braceClose: { ch: "}", cls: "kw", points: 2, weight: 3 },
    parenOpen: { ch: "(", cls: "fn", points: 2, weight: 3 },
    parenClose: { ch: ")", cls: "fn", points: 2, weight: 3 },
    bracketOpen: { ch: "[", cls: "type", points: 2, weight: 2 },
    bracketClose: { ch: "]", cls: "type", points: 2, weight: 2 },
    assign: { ch: "=", cls: "op", points: 2, weight: 3 },
    digit: { glyphs: "0123456789", cls: "num", points: 3, weight: 3 },
    quote: { ch: "\"", cls: "str", points: 3, weight: 2 },
    hash: { ch: "#", cls: "cmt", points: 1, weight: 2 },
  };
  var TOKEN_NAMES = Object.keys(TOKENS);

  // --- random: the state travels in the game, so a seed replays ----------------
  /** One step of mulberry32: the next state and a value in [0, 1). */
  function rngStep(state) {
    var s = ((state >>> 0) + 0x6d2b79f5) >>> 0;
    var t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return { state: s, value: ((t ^ (t >>> 14)) >>> 0) / 4294967296 };
  }
  function assign(game, patch) {
    var out = {}, k;
    for (k in game) out[k] = game[k];
    for (k in patch) out[k] = patch[k];
    return out;
  }

  // --- the food -----------------------------------------------------------------------
  /** A food item without a cell: `digit` takes the glyph given, else its own. */
  function tokenOf(kind, glyph) {
    var t = TOKENS[kind];
    return { ch: t.ch || glyph || t.glyphs.charAt(0), cls: t.cls, points: t.points, kind: kind };
  }
  function at(list, x, y) {
    for (var i = 0; i < list.length; i++) if (list[i].x === x && list[i].y === y) return i;
    return -1;
  }
  /** The cells on neither the worm nor a food, row by row. */
  function freeCells(game) {
    var free = [];
    for (var y = 0; y < ROWS; y++) for (var x = 0; x < COLS; x++) if (at(game.body, x, y) < 0 && at(game.food, x, y) < 0) free.push({ x: x, y: y });
    return free;
  }
  /** One token drawn by weight, its glyph if it has several, its cell among the free ones. */
  function rollFood(game) {
    var free = freeCells(game);
    if (!free.length) return null;
    var total = 0, i;
    for (i = 0; i < TOKEN_NAMES.length; i++) total += TOKENS[TOKEN_NAMES[i]].weight;
    var r = rngStep(game.rng), pick = r.value * total, kind = TOKEN_NAMES[TOKEN_NAMES.length - 1];
    for (i = 0; i < TOKEN_NAMES.length; i++) { pick -= TOKENS[TOKEN_NAMES[i]].weight; if (pick < 0) { kind = TOKEN_NAMES[i]; break; } }
    var glyph = null;
    if (TOKENS[kind].glyphs) { r = rngStep(r.state); glyph = TOKENS[kind].glyphs.charAt(Math.floor(r.value * TOKENS[kind].glyphs.length)); }
    r = rngStep(r.state);
    var cell = free[Math.floor(r.value * free.length)];
    var item = tokenOf(kind, glyph);
    item.x = cell.x; item.y = cell.y;
    return { item: item, state: r.state };
  }
  /** Food topped up to two, never on the worm or on food; none when the grid is full. */
  function spawnFood(game) {
    var next = game;
    while (next.food.length < FOOD) {
      var rolled = rollFood(next);
      if (!rolled) break;
      next = assign(next, { food: next.food.concat([rolled.item]), rng: rolled.state });
    }
    return next;
  }

  // --- the game -----------------------------------------------------------------------
  function intervalFor(length) {
    var ms = SPEEDS[0].ms;
    for (var i = 0; i < SPEEDS.length; i++) if (SPEEDS[i].from <= length) ms = SPEEDS[i].ms;
    return ms;
  }
  function segment(x, y, ch, cls) { return { x: x, y: y, ch: ch, cls: cls }; }
  function newGame(seed, options) {
    var s = (seed >>> 0) || 1;
    var best = options && typeof options.best === "number" && options.best >= 0 ? Math.floor(options.best) : 0;
    var game = {
      seed: s, rng: s,
      body: [segment(12, 8, "█", "head"), segment(11, 8, ">", "cmt"), segment(10, 8, ">", "cmt")],
      dir: "right", queue: [], food: [], open: [],
      sinceSemi: 0, streak: 0, lastCls: "", score: 0, best: best, bonus: null,
      alive: true, cause: "", paused: false,
      interval: intervalFor(3), acc: 0, ticks: 0,
    };
    return spawnFood(game);
  }
  /** A turn joins the queue unless it is the way the worm already goes, or its opposite; two wait at most. */
  function turn(game, dir) {
    var last = game.queue.length ? game.queue[game.queue.length - 1] : game.dir;
    if (dir === last || dir === OPPOSITE[last] || game.queue.length >= 2) return game;
    return assign(game, { queue: game.queue.concat([dir]) });
  }
  function apply(game, action) {
    if (!game.alive) return game;
    if (action === "pause") return game.paused ? game : assign(game, { paused: true });
    if (action === "resume") return game.paused ? assign(game, { paused: false }) : game;
    if (!DIRS[action]) return game;
    return turn(game, action);
  }
  /** The scoring half of a bite: the streak by class, a bracket that balances, a statement a `;` banks. */
  function eat(game, token) {
    var streak = token.cls === game.lastCls ? game.streak + 1 : 1;
    var score = game.score + token.points * Math.min(streak, BONUS.streakCap);
    var open = game.open, bonus = null, sinceSemi = game.sinceSemi + 1;
    if (OPENERS[token.ch]) open = open.concat([token.ch]);
    else if (PAIRS[token.ch] && open.length && open[open.length - 1] === PAIRS[token.ch]) {
      open = open.slice(0, -1);
      bonus = { kind: "balanced", points: BONUS.balanced, at: game.ticks };
      score += BONUS.balanced;
    }
    if (token.ch === ";") {
      var banked = BONUS.perSegment * game.sinceSemi;
      if (banked > 0) { bonus = { kind: "statement", points: banked, at: game.ticks }; score += banked; }
      sinceSemi = 0;
    }
    if (token.cls === "cmt") streak = 0;
    return assign(game, { streak: streak, lastCls: token.cls, score: score, open: open, sinceSemi: sinceSemi, bonus: bonus, best: Math.max(game.best, score) });
  }
  /** One move: the queued turn, the step, a bite that grows the neck, the wall or the body that kills. */
  function step(game) {
    if (!game.alive) return game;
    var dir = game.queue.length ? game.queue[0] : game.dir;
    var head = game.body[0], nx = head.x + DIRS[dir][0], ny = head.y + DIRS[dir][1];
    var base = assign(game, { dir: dir, queue: game.queue.slice(1), ticks: game.ticks + 1 });
    if (nx < 0 || nx >= COLS || ny < 0 || ny >= ROWS) return assign(base, { alive: false, cause: "wall" });
    var bite = at(game.food, nx, ny), eating = bite >= 0;
    var blockers = eating ? game.body : game.body.slice(0, -1);
    if (at(blockers, nx, ny) >= 0) return assign(base, { alive: false, cause: "self" });
    var cells = [[nx, ny]].concat(game.body.map(function (s) { return [s.x, s.y]; }));
    var worn = game.body.map(function (s) { return { ch: s.ch, cls: s.cls }; });
    if (eating) worn = [worn[0], { ch: game.food[bite].ch, cls: game.food[bite].cls }].concat(worn.slice(1));
    else cells = cells.slice(0, -1);
    var body = cells.map(function (c, i) { return segment(c[0], c[1], worn[i].ch, worn[i].cls); });
    var next = assign(base, { body: body });
    if (!eating) return next;
    next = eat(next, game.food[bite]);
    next = assign(next, { food: game.food.slice(0, bite).concat(game.food.slice(bite + 1)), interval: intervalFor(body.length) });
    next = spawnFood(next);
    return next.food.length ? next : assign(next, { alive: false, cause: "full" });
  }
  /** Time passes: a step whenever the interval is due, two at most, never while paused or dead. */
  function tick(game, dtMs) {
    if (!game.alive || game.paused || !(dtMs > 0)) return game;
    var next = assign(game, { acc: game.acc + Math.min(dtMs, MAX_DT) });
    for (var n = 0; n < MAX_STEPS && next.alive && next.acc >= next.interval; n++) next = assign(step(next), { acc: next.acc - next.interval });
    return next;
  }

  // --- what the screen shows ---------------------------------------------------------
  /** The grid: blanks, bright food, the body dimmed by class, the head last. */
  function view(game) {
    var rows = [], y, x, i;
    for (y = 0; y < ROWS; y++) { var row = []; for (x = 0; x < COLS; x++) row.push({ ch: " ", cls: "" }); rows.push(row); }
    for (i = 0; i < game.food.length; i++) rows[game.food[i].y][game.food[i].x] = { ch: game.food[i].ch, cls: game.food[i].cls };
    for (i = game.body.length - 1; i >= 1; i--) rows[game.body[i].y][game.body[i].x] = { ch: game.body[i].ch, cls: "body " + game.body[i].cls };
    var head = game.body[0];
    rows[head.y][head.x] = { ch: head.ch, cls: game.alive ? "head" : "head dead" };
    return rows;
  }
  /** The status word: what the last bite earned, the streak that runs, or how it ended. */
  function words(game) {
    if (!game.alive) return game.cause === "wall" ? "hit the wall" : game.cause === "self" ? "bit yourself" : "the grid is full — a win";
    if (game.bonus && game.ticks - game.bonus.at < BONUS.toastTicks) return game.bonus.kind + " +" + game.bonus.points;
    if (game.streak >= 2) return "streak ×" + Math.min(game.streak, BONUS.streakCap);
    return "";
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

  global.Linefeed = {
    COLS: COLS, ROWS: ROWS, MAX_DT: MAX_DT, FOOD: FOOD,
    TOKENS: TOKENS, TOKEN_NAMES: TOKEN_NAMES, ACTION_NAMES: ACTION_NAMES, SPEEDS: SPEEDS, PAIRS: PAIRS, BONUS: BONUS,
    rngStep: rngStep, tokenOf: tokenOf, freeCells: freeCells, spawnFood: spawnFood, intervalFor: intervalFor,
    newGame: newGame, apply: apply, tick: tick, step: step, eat: eat, view: view, words: words, readSaved: readSaved,
  };
})(typeof window !== "undefined" ? window : this);
