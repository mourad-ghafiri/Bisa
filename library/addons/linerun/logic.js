// Linerun's rules, with no page in them: a λ running along three lines of
// a file that scrolls ever faster, the bugs and comment walls to dodge or
// hop, the tokens to take, the near misses, the bang that doubles, the chunks the file is built
// from and the search that proves each can be run, the best to chase, the
// seed. `main.js` draws; `desktop/src/scenarios/addonsLogic.test.mjs` runs
// this alone, so the same seed and the same keys replay the same run.
// Every function returns a new game, or the very same object when nothing changed.
(function (global) {
  "use strict";

  var LANES = 3, COLS = 48, GUTTER = 5, RUNNER_X = 6, JUMP = 4, RUNWAY = 24, MARGIN = 8, TICK_MAX = 100, LOG_MAX = 6;
  var SPEED = { start: 8, cap: 20, perCell: 0.012 };
  var BURST = { cells: 40, speed: 1.5, points: 2 };
  var MULT = { per: 5, max: 5 };
  var GLYPHS = { runner: "λ", air: "Λ", best: "┊", bar: "│" };
  var ACTION_NAMES = ["up", "down", "jump", "pause", "resume"];
  /** What lives in the file: its glyphs per line it spans, its class, how wide, whether a hop clears it, what it pays. */
  var KINDS = {
    bug: { text: [["✗"]], cls: "err", span: 1, blocks: true, jumpable: true, near: 25 },
    nan: { text: [["N", "a", "N"]], cls: "err", span: 1, blocks: true, jumpable: true, near: 40 },
    todo: { text: [["T", "O", "D", "O", ":"]], cls: "cmt", span: 1, blocks: true, jumpable: false, near: 0 },
    wall: { text: [["/", "*", "*"], ["*", "*", "/"]], cls: "cmt", span: 2, blocks: true, jumpable: false, near: 0 },
    coin: { text: [["$"]], cls: "num", span: 1, collect: true, points: 10 },
    gem: { text: [["◆"]], cls: "fn", span: 1, collect: true, points: 30 },
    bang: { text: [["!"]], cls: "kw", span: 1, collect: true, points: 0, burst: true },
  };
  var KIND_NAMES = Object.keys(KINDS);
  KIND_NAMES.forEach(function (name) { KINDS[name].width = KINDS[name].text[0].length; });
  /** The file's pieces, by tier: what sits where from the chunk's first column, and the gap after it. */
  var CHUNKS = {
    "lone-bug": { tier: 1, gap: 8, cells: [{ dx: 0, lane: 1, kind: "bug" }, { dx: 3, lane: 1, kind: "coin" }] },
    "coin-run": { tier: 1, gap: 8, cells: [{ dx: 0, lane: 0, kind: "coin" }, { dx: 1, lane: 0, kind: "coin" }, { dx: 2, lane: 0, kind: "coin" }, { dx: 3, lane: 0, kind: "coin" }] },
    "todo-low": { tier: 1, gap: 8, cells: [{ dx: 0, lane: 2, kind: "todo" }, { dx: 2, lane: 1, kind: "coin" }] },
    "wall-top": { tier: 2, gap: 6, cells: [{ dx: 0, lane: 0, kind: "wall" }, { dx: 0, lane: 2, kind: "coin" }, { dx: 1, lane: 2, kind: "coin" }, { dx: 2, lane: 2, kind: "coin" }] },
    "bug-gem": { tier: 2, gap: 6, cells: [{ dx: 0, lane: 1, kind: "bug" }, { dx: 2, lane: 1, kind: "gem" }] },
    "stagger": { tier: 2, gap: 6, cells: [{ dx: 0, lane: 0, kind: "bug" }, { dx: 1, lane: 1, kind: "coin" }, { dx: 2, lane: 2, kind: "bug" }, { dx: 4, lane: 1, kind: "bug" }] },
    "nan-late": { tier: 3, gap: 5, cells: [{ dx: 0, lane: 1, kind: "nan" }, { dx: 5, lane: 1, kind: "gem" }] },
    "funnel": { tier: 3, gap: 5, cells: [{ dx: 0, lane: 0, kind: "todo" }, { dx: 0, lane: 2, kind: "todo" }, { dx: 2, lane: 1, kind: "bug" }, { dx: 4, lane: 1, kind: "bang" }] },
    "wall-swap": { tier: 3, gap: 5, cells: [{ dx: 0, lane: 0, kind: "wall" }, { dx: 1, lane: 2, kind: "coin" }, { dx: 5, lane: 1, kind: "wall" }, { dx: 6, lane: 0, kind: "coin" }] },
  };
  var CHUNK_NAMES = Object.keys(CHUNKS);
  /** Which tiers the file draws from, by the distance run: `[from, [weight of tier 1, 2, 3]]`. */
  var TIER_MIX = [[0, [1, 0, 0]], [150, [2, 2, 0]], [400, [1, 2, 2]], [800, [1, 1, 3]]];

  // --- random: the state travels in the game, so a seed replays ----------------
  /** One step of mulberry32: the next state and a value in [0, 1). */
  function rngStep(state) {
    var s = ((state >>> 0) + 0x6d2b79f5) >>> 0;
    var t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return { state: s, value: ((t ^ (t >>> 14)) >>> 0) / 4294967296 };
  }
  function rngInt(state, lo, hi) { var r = rngStep(state); return { state: r.state, value: lo + Math.floor(r.value * (hi - lo + 1)) }; }
  function rngPick(state, list) { var r = rngStep(state); return { state: r.state, value: list[Math.floor(r.value * list.length)] }; }
  /** An index drawn by weight. */
  function rngWeighted(state, weights) {
    var total = 0, i;
    for (i = 0; i < weights.length; i++) total += weights[i];
    var r = rngStep(state), pick = r.value * total;
    for (i = 0; i < weights.length; i++) { pick -= weights[i]; if (pick < 0) return { state: r.state, value: i }; }
    return { state: r.state, value: weights.length - 1 };
  }
  function assign(game, patch) {
    var out = {}, k;
    for (k in game) out[k] = game[k];
    for (k in patch) out[k] = patch[k];
    return out;
  }

  // --- the numbers --------------------------------------------------------------------
  function speedAt(distance) { return Math.min(SPEED.cap, SPEED.start + distance * SPEED.perCell); }
  function effectiveSpeed(game) { return game.speed * (game.burst > 0 ? BURST.speed : 1); }
  function multiplier(streak) { return Math.min(MULT.max, 1 + Math.floor(streak / MULT.per)); }
  function runnerCol(game) { return game.distance + RUNNER_X; }
  function chunkWidth(chunk) {
    var width = 0;
    chunk.cells.forEach(function (c) { width = Math.max(width, c.dx + KINDS[c.kind].width); });
    return width;
  }
  function tierWeights(distance) {
    var weights = TIER_MIX[0][1];
    for (var i = 0; i < TIER_MIX.length; i++) if (TIER_MIX[i][0] <= distance) weights = TIER_MIX[i][1];
    return weights;
  }
  /** Whether an entity sits on this column of this lane. */
  function covers(e, col, lane) {
    var k = KINDS[e.kind];
    return col >= e.x && col < e.x + k.width && lane >= e.lane && lane < e.lane + k.span;
  }
  function pushLog(game, text, cls) {
    var log = game.log.concat([{ at: game.distance, text: text, cls: cls }]);
    if (log.length > LOG_MAX) log = log.slice(log.length - LOG_MAX);
    return assign(game, { log: log });
  }
  function patched(list, i, patch) {
    var out = list.slice();
    out[i] = assign(list[i], patch);
    return out;
  }

  // --- the file ----------------------------------------------------------------------------
  /** Chunks laid down ahead until the file reaches the column asked for. */
  function generate(game, until) {
    var next = game;
    while (next.cursor < until) {
      var tier = rngWeighted(next.rngState, tierWeights(next.distance));
      var names = CHUNK_NAMES.filter(function (n) { return CHUNKS[n].tier === tier.value + 1; });
      var pick = rngPick(tier.state, names), chunk = CHUNKS[pick.value];
      var entities = next.entities.slice(), id = next.nextId;
      chunk.cells.forEach(function (c) { entities.push({ id: id++, kind: c.kind, x: next.cursor + c.dx, lane: c.lane, done: false, near: false }); });
      next = assign(next, { rngState: pick.state, entities: entities, nextId: id, cursor: next.cursor + chunkWidth(chunk) + chunk.gap });
    }
    return next;
  }
  /** What has scrolled off the left edge is forgotten. */
  function cull(game) {
    var kept = game.entities.filter(function (e) { return e.x + KINDS[e.kind].width >= game.distance; });
    return kept.length === game.entities.length ? game : assign(game, { entities: kept });
  }
  /**
   * The search that proves a chunk can be run: from any lane one column before
   * it, through every column, by the game's own `covers` and jump rule, taking
   * every collectible — the actions of one such path, or null.
   */
  function pathThrough(chunk) {
    var width = chunkWidth(chunk);
    var ents = chunk.cells.map(function (c, i) { return { id: i, kind: c.kind, x: c.dx, lane: c.lane }; });
    var collectibles = ents.filter(function (e) { return KINDS[e.kind].collect; }).map(function (e) { return e.id; });
    var allMask = (1 << collectibles.length) - 1;
    function meet(col, lane, air, mask) {
      var out = { dead: false, mask: mask };
      ents.forEach(function (e) {
        if (!covers(e, col, lane)) return;
        var k = KINDS[e.kind];
        if (k.collect) out.mask |= 1 << collectibles.indexOf(e.id);
        else if (k.blocks && !(air > 0 && k.jumpable)) out.dead = true;
      });
      return out;
    }
    var seen = {}, queue = [], lane;
    for (lane = 0; lane < LANES; lane++) queue.push({ col: -1, lane: lane, air: 0, mask: 0, path: [] });
    while (queue.length) {
      var s = queue.shift();
      if (s.col >= width) { if (s.mask === allMask) return s.path; continue; }
      var key = s.col + ":" + s.lane + ":" + s.air + ":" + s.mask;
      if (seen[key]) continue;
      seen[key] = true;
      var jumps = s.air === 0 ? [false, true] : [false];
      for (var j = 0; j < jumps.length; j++) for (var d = -1; d <= 1; d++) {
        var lane2 = s.lane + d, air = jumps[j] ? JUMP : s.air, mask = s.mask;
        if (lane2 < 0 || lane2 >= LANES) continue;
        if (d !== 0) { var here = meet(s.col, lane2, air, mask); if (here.dead) continue; mask = here.mask; }
        var ahead = meet(s.col + 1, lane2, air, mask);
        if (ahead.dead) continue;
        var step = (jumps[j] ? "jump" : "") + (d < 0 ? "up" : d > 0 ? "down" : "");
        queue.push({ col: s.col + 1, lane: lane2, air: Math.max(0, air - 1), mask: ahead.mask, path: s.path.concat([step || "run"]) });
      }
    }
    return null;
  }

  // --- the run -----------------------------------------------------------------------------
  function newGame(seed, saved) {
    var s = (seed >>> 0) || 1;
    var best = saved && saved.best ? saved.best : { distance: 0, score: 0, seed: 0 };
    var game = {
      seed: s, rngState: s,
      offset: 0, distance: 0, speed: speedAt(0),
      runner: { lane: 1, air: 0, landAt: -1 },
      entities: [], nextId: 1, cursor: RUNWAY,
      score: 0, streak: 0, burst: 0,
      alive: true, paused: false, crash: null,
      best: { distance: best.distance, score: best.score, seed: best.seed },
      log: [],
    };
    return generate(game, COLS + MARGIN);
  }
  /**
   * The one collision rule, for one column of one lane: a collectible is
   * taken, an obstacle crashes unless hopped; a near miss pays once; a
   * collectible left behind on any lane ends the streak.
   */
  function resolveCell(game, col, lane) {
    var next = game, i, e, k, pts;
    for (i = 0; i < next.entities.length; i++) {
      e = next.entities[i];
      if (e.done || !covers(e, col, lane)) continue;
      k = KINDS[e.kind];
      if (k.collect) {
        pts = k.points * multiplier(next.streak) * (next.burst > 0 ? BURST.points : 1);
        next = assign(next, { entities: patched(next.entities, i, { done: true }), score: next.score + pts, streak: next.streak + 1 });
        if (k.burst) next = pushLog(assign(next, { burst: BURST.cells + 1 }), "bang! ×2 for " + BURST.cells + " cells", "accent");
      } else if (k.blocks && !(next.runner.air > 0 && k.jumpable)) {
        var crash = { kind: e.kind, x: e.x, lane: e.lane };
        return assign(pushLog(next, "crash: " + k.text[0].join("") + " at " + (col - RUNNER_X), "err"), { alive: false, crash: crash, streak: 0 });
      }
    }
    for (i = 0; i < next.entities.length; i++) {
      e = next.entities[i];
      k = KINDS[e.kind];
      if (e.near || !k.near) continue;
      var beside = e.x === col && (e.lane === lane - 1 || e.lane === lane + 1);
      var landed = next.runner.landAt === col && e.lane === lane && e.x + k.width === col;
      if (!beside && !landed) continue;
      pts = k.near * multiplier(next.streak);
      next = pushLog(assign(next, { entities: patched(next.entities, i, { near: true }), score: next.score + pts }), "close! +" + pts, "accent");
    }
    for (i = 0; i < next.entities.length; i++) {
      e = next.entities[i];
      k = KINDS[e.kind];
      if (!k.collect || e.done || e.x >= col) continue;
      next = pushLog(assign(next, { entities: patched(next.entities, i, { done: true }), streak: 0 }), "missed " + k.text[0].join(""), "cmt");
    }
    return next;
  }
  function apply(game, action) {
    if (!game.alive) return game;
    if (action === "pause") return game.paused ? game : assign(game, { paused: true });
    if (action === "resume") return game.paused ? assign(game, { paused: false }) : game;
    if (game.paused) return game;
    if (action === "jump") {
      if (game.runner.air > 0) return game;
      return assign(game, { runner: assign(game.runner, { air: JUMP, landAt: runnerCol(game) + JUMP + 1 }) });
    }
    if (action !== "up" && action !== "down") return game;
    var lane = Math.max(0, Math.min(LANES - 1, game.runner.lane + (action === "up" ? -1 : 1)));
    if (lane === game.runner.lane) return game;
    return resolveCell(assign(game, { runner: assign(game.runner, { lane: lane }) }), runnerCol(game), lane);
  }
  /** Time passes: the file scrolls, every column crossed is met in order, the speed follows the distance, the file grows ahead. */
  function tick(game, dtMs) {
    if (!game.alive || game.paused || !(dtMs > 0)) return game;
    var target = game.offset + effectiveSpeed(game) * (Math.min(dtMs, TICK_MAX) / 1000);
    var next = game, to = Math.floor(target) + RUNNER_X;
    for (var col = runnerCol(game) + 1; col <= to; col++) {
      next = resolveCell(next, col, next.runner.lane);
      next = assign(next, { runner: assign(next.runner, { air: Math.max(0, next.runner.air - 1) }), burst: Math.max(0, next.burst - 1), distance: col - RUNNER_X });
      if (!next.alive) { next = assign(next, { offset: col - RUNNER_X }); break; }
    }
    if (next.alive) next = assign(next, { offset: target });
    next = assign(next, { speed: speedAt(next.distance) });
    if (game.best.distance > 0 && game.distance < game.best.distance && next.distance >= game.best.distance) next = pushLog(next, "past your best", "accent");
    return generate(cull(next), next.distance + COLS + MARGIN);
  }

  // --- what the screen shows ---------------------------------------------------------
  /** Three lines with their numbers, the file as it scrolls, the best's mark, the runner last. */
  function view(game) {
    var rows = [], lane, x, i;
    for (lane = 0; lane < LANES; lane++) {
      var row = [{ ch: " ", cls: "gutter" }, { ch: String(lane + 1), cls: lane === game.runner.lane ? "value" : "gutter" }, { ch: " ", cls: "gutter" }, { ch: GLYPHS.bar, cls: "frame" }, { ch: " ", cls: "gutter" }];
      for (x = 0; x < COLS; x++) row.push({ ch: " ", cls: "" });
      rows.push(row);
    }
    var bestCol = RUNNER_X + game.best.distance - game.distance;
    if (game.best.distance > 0 && bestCol >= 0 && bestCol < COLS) for (lane = 0; lane < LANES; lane++) rows[lane][GUTTER + bestCol] = { ch: GLYPHS.best, cls: "cmt" };
    for (i = 0; i < game.entities.length; i++) {
      var e = game.entities[i], k = KINDS[e.kind];
      if (e.done) continue;
      for (var li = 0; li < k.span; li++) for (x = 0; x < k.width; x++) {
        var col = e.x + x - game.distance;
        if (col >= 0 && col < COLS) rows[e.lane + li][GUTTER + col] = { ch: k.text[li][x], cls: k.cls };
      }
    }
    rows[game.runner.lane][GUTTER + RUNNER_X] = { ch: game.runner.air > 0 ? GLYPHS.air : GLYPHS.runner, cls: game.alive ? "fn" : "err" };
    return rows;
  }
  /** One line about the run. */
  function statusLine(game) {
    if (!game.alive) return "crash: " + KINDS[game.crash.kind].text[0].join("") + " at " + game.distance;
    var words = game.distance + " cells · " + game.score + " pts · ×" + multiplier(game.streak);
    if (game.streak > 0) words += " (streak " + game.streak + ")";
    words += " · " + effectiveSpeed(game).toFixed(1) + " c/s";
    if (game.burst > 0) words += " · ! " + game.burst;
    return words;
  }
  /** The better run: the longer, then the richer. */
  function bestOf(game) {
    var mine = { distance: game.distance, score: game.score, seed: game.seed }, b = game.best;
    if (mine.distance > b.distance || (mine.distance === b.distance && mine.score > b.score)) return mine;
    return { distance: b.distance, score: b.score, seed: b.seed };
  }
  function wholeIn(v, cap) { return typeof v === "number" && v >= 0 && v < cap ? Math.floor(v) : null; }
  /** What the store held, as far as its shape allows. */
  function readSaved(raw) {
    var out = { best: { distance: 0, score: 0, seed: 0 }, muted: false };
    var parsed;
    try { parsed = JSON.parse(raw); } catch (e) { return out; }
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return out;
    if (parsed.muted === true) out.muted = true;
    var b = parsed.best;
    if (!b || typeof b !== "object" || Array.isArray(b)) return out;
    var distance = wholeIn(b.distance, 1e9), score = wholeIn(b.score, 1e9), seed = wholeIn(b.seed, 4294967296);
    if (distance === null || score === null || seed === null) return out;
    out.best = { distance: distance, score: score, seed: seed };
    return out;
  }

  global.Linerun = {
    LANES: LANES, COLS: COLS, GUTTER: GUTTER, RUNNER_X: RUNNER_X, JUMP: JUMP, RUNWAY: RUNWAY, TICK_MAX: TICK_MAX, LOG_MAX: LOG_MAX,
    SPEED: SPEED, BURST: BURST, MULT: MULT, GLYPHS: GLYPHS, TIER_MIX: TIER_MIX,
    KINDS: KINDS, KIND_NAMES: KIND_NAMES, CHUNKS: CHUNKS, CHUNK_NAMES: CHUNK_NAMES, ACTION_NAMES: ACTION_NAMES,
    rngStep: rngStep, rngInt: rngInt, rngPick: rngPick, rngWeighted: rngWeighted,
    speedAt: speedAt, effectiveSpeed: effectiveSpeed, multiplier: multiplier, runnerCol: runnerCol, chunkWidth: chunkWidth, tierWeights: tierWeights, covers: covers,
    generate: generate, pathThrough: pathThrough, newGame: newGame, apply: apply, tick: tick, resolveCell: resolveCell,
    view: view, statusLine: statusLine, bestOf: bestOf, readSaved: readSaved,
  };
})(typeof window !== "undefined" ? window : this);
