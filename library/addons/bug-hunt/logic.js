// Bug hunt's rules, with no page in them: the insects and how each moves,
// the tools and what each hits, the marks a strike leaves on the glass and
// how they fade or get wiped, the score, the strip's place, the words.
// `main.js` draws; `desktop/src/scenarios/addonsLogic.test.mjs` runs this
// alone with a seeded random, so the same seed walks the same path.
(function (global) {
  "use strict";

  // --- random -----------------------------------------------------------------
  /** A small seeded generator (mulberry32): the same seed, the same walk. */
  function rng(seed) {
    var s = (seed >>> 0) || 1;
    function next() {
      s = (s + 0x6d2b79f5) >>> 0;
      var t = s;
      t = Math.imul(t ^ (t >>> 15), t | 1);
      t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    }
    return {
      next: next,
      range: function (lo, hi) { return lo + (hi - lo) * next(); },
      int: function (lo, hi) { return Math.floor(lo + (hi - lo + 1) * next()); },
      pick: function (list) { return list[Math.floor(next() * list.length)]; },
      chance: function (p) { return next() < p; },
    };
  }

  // --- the insects --------------------------------------------------------------
  /**
   * Each species: its size (px across), how it moves, its speed and pause
   * ranges (px/s, s), how many hits it takes, what it scores, how close the
   * pointer may come before it flees (0: never), and the colours of its body
   * and its splat.
   */
  var SPECIES = {
    spider: { size: 34, gait: "walk", speed: [26, 56], pause: [0.8, 2.6], hp: 1, points: 15, scare: 90, legs: 8, wings: 0, color: "#2a2623", splat: "#5a4a3a" },
    cockroach: { size: 30, gait: "walk", speed: [90, 170], pause: [0.2, 1.0], hp: 1, points: 20, scare: 150, legs: 6, wings: 0, color: "#6b3a1e", splat: "#7a5a2a", darts: true },
    beetle: { size: 32, gait: "walk", speed: [18, 34], pause: [0.8, 2.4], hp: 2, points: 30, scare: 40, legs: 6, wings: 0, color: "#1f3b2d", splat: "#3b5a2a" },
    ant: { size: 16, gait: "walk", speed: [40, 70], pause: [0.1, 0.5], hp: 1, points: 5, scare: 30, legs: 6, wings: 0, color: "#3a1f12", splat: "#4a2a1a", file: 3 },
    fly: { size: 20, gait: "fly", speed: [120, 220], pause: [0.8, 3.0], hp: 1, points: 25, scare: 120, legs: 6, wings: 2, color: "#2f3540", splat: "#4a5a3a" },
    mosquito: { size: 18, gait: "hover", speed: [60, 110], pause: [0.3, 1.2], hp: 1, points: 25, scare: 0, legs: 6, wings: 2, color: "#4a4a4a", splat: "#6a2a2a", whine: true },
    cricket: { size: 26, gait: "jump", speed: [30, 50], pause: [0.5, 1.8], hp: 1, points: 20, scare: 110, legs: 6, wings: 0, color: "#4b5a1f", splat: "#5a6a2a", hop: [120, 260] },
  };
  var SPECIES_NAMES = Object.keys(SPECIES);
  /** An insect lives this long (s) before it heads for an edge and leaves. */
  var LIFETIME = [25, 60];
  /**
   * The golden one: rare, never two at once, faster and more skittish than
   * its species, worth ten times its points, and gone sooner.
   */
  var GOLDEN = { chance: 0.06, points: 10, speed: 1.35, scare: 1.4, life: [9, 15], color: "#d9a92c", splat: "#8a6a1a" };
  /** A blow scares what is near it for this long (s). */
  var SCARE_TTL = 0.7;
  /** A cricket's hop takes this long (s) and rises this high (px). */
  var HOP_TIME = 0.55, HOP_HEIGHT = 44;
  /** A fly hangs this high (px) in the air; a mosquito this high. */
  var FLY_HEIGHT = 14, HOVER_HEIGHT = 8;

  var DIFFICULTIES = {
    calm: { max: 4, interval: 6 },
    busy: { max: 8, interval: 3.5 },
    swarm: { max: 14, interval: 1.8 },
  };
  var DIFFICULTY_NAMES = Object.keys(DIFFICULTIES);
  function difficultyOf(name) { return DIFFICULTIES[name] || DIFFICULTIES.busy; }

  /** How many insects may be on the glass at once; half of them under reduced motion. */
  function maxInsects(o) {
    var d = difficultyOf(o && o.difficulty);
    return o && o.reduced ? Math.max(1, Math.ceil(d.max / 2)) : d.max;
  }
  /** Seconds between arrivals: the more that runs, the more that crawls — never under a floor, slower under reduced motion. */
  function spawnInterval(o) {
    var d = difficultyOf(o && o.difficulty);
    var working = Math.max(0, Math.min(8, (o && o.working) || 0));
    var base = d.interval / (1 + 0.25 * working);
    if (o && o.reduced) base *= 1.5;
    return Math.max(0.8, base);
  }
  /** How fast everything moves: half under reduced motion. */
  function speedScale(o) { return o && o.reduced ? 0.5 : 1; }

  function inRect(p, r, pad) {
    var m = pad || 0;
    return p.x >= r.x - m && p.x <= r.x + r.width + m && p.y >= r.y - m && p.y <= r.y + r.height + m;
  }
  function inObstacle(p, obstacles, pad) {
    for (var i = 0; i < (obstacles || []).length; i++) if (inRect(p, obstacles[i], pad)) return obstacles[i];
    return null;
  }

  /**
   * Whether the next arrival is the golden one — never while one lives — and
   * which species it is: one that walks alone and can be scared, so the
   * chase is a chase.
   */
  function goldenTurn(r, insects) {
    for (var i = 0; i < (insects || []).length; i++) {
      var b = insects[i];
      if (b.golden && b.state !== "dead" && !b.gone) return null;
    }
    if (!r.chance(GOLDEN.chance)) return null;
    return r.pick(SPECIES_NAMES.filter(function (name) { return !SPECIES[name].file && SPECIES[name].scare > 0; }));
  }

  /** A new insect at an edge of the glass, headed in, never inside a tab's box — the golden one faster and shorter-lived, its extra draws after the usual ones. */
  function spawn(species, bounds, obstacles, r, now, id, golden) {
    var sp = SPECIES[species] || SPECIES.ant;
    var half = sp.size / 2, x = 0, y = 0, heading = 0;
    for (var tries = 0; tries < 16; tries++) {
      var edge = r.int(0, 3);
      if (edge === 0) { x = r.range(half, Math.max(half, bounds.width - half)); y = -half; heading = Math.PI / 2; }
      else if (edge === 1) { x = bounds.width + half; y = r.range(half, Math.max(half, bounds.height - half)); heading = Math.PI; }
      else if (edge === 2) { x = r.range(half, Math.max(half, bounds.width - half)); y = bounds.height + half; heading = -Math.PI / 2; }
      else { x = -half; y = r.range(half, Math.max(half, bounds.height - half)); heading = 0; }
      if (!inObstacle({ x: x, y: y }, obstacles, half + 4)) break;
    }
    heading += r.range(-0.5, 0.5);
    var out = {
      id: id, species: species, x: x, y: y, z: sp.gait === "fly" ? FLY_HEIGHT : sp.gait === "hover" ? HOVER_HEIGHT : 0,
      heading: heading, speed: r.range(sp.speed[0], sp.speed[1]),
      state: sp.gait === "fly" ? "fly" : sp.gait === "hover" ? "hover" : "walk",
      t: 0, until: r.range(1, 3), hp: sp.hp, phase: r.next() * 6.283, seed: r.int(1, 1e9),
      born: now, life: r.range(LIFETIME[0], LIFETIME[1]), leaving: false, slow: 0, sprayed: false,
      hop: null, leader: null, gone: false, entered: false,
    };
    if (golden) { out.golden = true; out.speed *= GOLDEN.speed; out.life = r.range(GOLDEN.life[0], GOLDEN.life[1]); }
    return out;
  }

  function wrapAngle(a) {
    while (a > Math.PI) a -= 6.283185307179586;
    while (a < -Math.PI) a += 6.283185307179586;
    return a;
  }
  function turnToward(heading, target, rate) {
    var d = wrapAngle(target - heading);
    return heading + Math.max(-rate, Math.min(rate, d));
  }
  function dist(a, b) { var dx = a.x - b.x, dy = a.y - b.y; return Math.sqrt(dx * dx + dy * dy); }

  /** Whether a hop from here lands on the glass and crosses no tab — sampled along its path. */
  function hopClear(n, hopDist, half, bounds, obstacles) {
    for (var k = 1; k <= 6; k++) {
      var p = { x: n.x + Math.cos(n.heading) * hopDist * (k / 6), y: n.y + Math.sin(n.heading) * hopDist * (k / 6) };
      if (p.x < half || p.x > bounds.width - half || p.y < half || p.y > bounds.height - half) return false;
      if (inObstacle(p, obstacles, half + 2)) return false;
    }
    return true;
  }

  /**
   * One tick of one insect: a new object, never the old one changed. `env`
   * is `{bounds, obstacles, pointer, scares, rng, now, scale, leaderOf}` —
   * the pointer may be null, `scares` the blows that just landed
   * (`{x, y, until}`, absent for none); `leaderOf(id)` answers an ant's
   * leader or null.
   */
  function step(insect, dt, env) {
    var sp = SPECIES[insect.species] || SPECIES.ant;
    var r = env.rng, bounds = env.bounds, obstacles = env.obstacles || [];
    var n = {};
    for (var k in insect) n[k] = insect[k];
    if (n.state === "dead" || n.gone) return n;
    var scale = (env.scale || 1) * (n.slow > 0 ? 0.35 : 1);
    n.t += dt;
    if (n.slow > 0) {
      n.slow = Math.max(0, n.slow - dt);
      if (n.slow === 0 && n.sprayed) { n.state = "dead"; n.cause = "spray"; return n; }
    }
    var age = env.now - n.born;
    if (!n.leaving && age > n.life) n.leaving = true;
    var half = sp.size / 2;
    var inside = n.x > half && n.x < bounds.width - half && n.y > half && n.y < bounds.height - half;
    if (inside) n.entered = true;
    // One that dawdles at the edge is turned toward the middle of the glass.
    if (!n.entered && n.t > 4 && n.state !== "jump") n.heading = turnToward(n.heading, Math.atan2(bounds.height / 2 - n.y, bounds.width / 2 - n.x), 3 * dt);

    // Fear: a pointer too near turns the insect away, and so does a blow that
    // just landed near — louder than a hovering hand, so it reaches farther;
    // the nearest fright wins — and a cockroach darts. The golden one is
    // more skittish than its kind.
    var scare = sp.scare * (n.golden ? GOLDEN.scare : 1);
    var pointer = env.pointer, fear = null, nearest = Infinity;
    if (pointer && scare > 0) {
      var dp = dist(pointer, n);
      if (dp < scare) { nearest = dp; fear = pointer; }
    }
    var scares = env.scares || [];
    for (var si = 0; si < scares.length && scare > 0; si++) {
      var blow = scares[si];
      if (!(blow.until > env.now)) continue;
      var db = dist(blow, n);
      if (db < scare * 1.3 + 30 && db < nearest) { nearest = db; fear = blow; }
    }
    if (fear && n.state !== "jump" && n.state !== "dart") {
      var away = Math.atan2(n.y - fear.y, n.x - fear.x) + r.range(-0.4, 0.4);
      n.heading = away;
      if (sp.darts) { n.state = "dart"; n.until = n.t + 0.45; }
      else if (n.state === "pause") { n.state = sp.gait === "fly" ? "fly" : "walk"; n.until = n.t + r.range(0.6, 1.6); }
    }

    var moving = true, speed = n.speed * scale;
    switch (n.state) {
      case "pause":
        moving = false;
        if (n.t > n.until) { n.state = sp.gait === "fly" ? "fly" : sp.gait === "hover" ? "hover" : "walk"; n.until = n.t + r.range(1, 3.5); n.heading += r.range(-1.2, 1.2); }
        break;
      case "hit":
        // Struck but alive: a moment's stagger, then off the other way.
        moving = false;
        if (n.t > n.until) { n.state = sp.gait === "fly" ? "fly" : sp.gait === "hover" ? "hover" : "walk"; n.until = n.t + r.range(0.8, 2); n.heading += Math.PI + r.range(-0.6, 0.6); }
        break;
      case "dart":
        speed *= 2.2;
        if (n.t > n.until) { n.state = "walk"; n.until = n.t + r.range(0.5, 2); }
        break;
      case "walk":
        n.heading += r.range(-0.9, 0.9) * dt;
        if (n.t > n.until) {
          var hopDist = sp.gait === "jump" ? r.range(sp.hop[0], sp.hop[1]) * scale : 0;
          if (hopDist > 0 && r.chance(0.55) && hopClear(n, hopDist, half, bounds, obstacles)) {
            n.state = "jump"; n.hop = { t: 0, dist: hopDist, from: { x: n.x, y: n.y } };
          } else {
            n.state = "pause"; n.until = n.t + r.range(sp.pause[0], sp.pause[1]);
          }
        }
        break;
      case "jump":
        moving = false;
        n.hop = { t: n.hop.t + dt, dist: n.hop.dist, from: n.hop.from };
        var p = Math.min(1, n.hop.t / HOP_TIME);
        n.x = n.hop.from.x + Math.cos(n.heading) * n.hop.dist * p;
        n.y = n.hop.from.y + Math.sin(n.heading) * n.hop.dist * p;
        n.z = 4 * HOP_HEIGHT * p * (1 - p);
        if (p >= 1) { n.z = 0; n.hop = null; n.state = "pause"; n.until = n.t + r.range(sp.pause[0], sp.pause[1]); }
        break;
      case "fly":
        n.heading += Math.sin(n.t * 7 + n.phase) * 2.4 * dt + r.range(-0.6, 0.6) * dt;
        n.z = FLY_HEIGHT + Math.sin(n.t * 11 + n.phase) * 3;
        if (n.t > n.until && r.chance(0.6)) { n.state = "pause"; n.z = 0; n.until = n.t + r.range(sp.pause[0], sp.pause[1]); }
        else if (n.t > n.until) { n.until = n.t + r.range(1, 3); }
        break;
      case "hover":
        var target = pointer ? { x: pointer.x + Math.cos(n.t * 1.7 + n.phase) * 60, y: pointer.y + Math.sin(n.t * 2.3 + n.phase) * 40 } : null;
        if (target) n.heading = turnToward(n.heading, Math.atan2(target.y - n.y, target.x - n.x), 4 * dt);
        else n.heading += Math.sin(n.t * 3 + n.phase) * 1.5 * dt;
        n.z = HOVER_HEIGHT + Math.sin(n.t * 13 + n.phase) * 2;
        break;
      default:
        break;
    }

    // An ant keeps its place in the file behind its leader.
    var leader = n.leader !== null && env.leaderOf ? env.leaderOf(n.leader) : null;
    if (leader && n.state !== "pause") {
      var gap = sp.size * 1.3;
      var want = { x: leader.x - Math.cos(leader.heading) * gap, y: leader.y - Math.sin(leader.heading) * gap };
      n.heading = turnToward(n.heading, Math.atan2(want.y - n.y, want.x - n.x), 6 * dt);
      speed = Math.max(speed, dist(want, n) * 2);
    }

    // Time to go: head for the nearest edge and never bounce again.
    if (n.leaving && n.state !== "jump") {
      var toLeft = n.x, toRight = bounds.width - n.x, toTop = n.y, toBottom = bounds.height - n.y;
      var least = Math.min(toLeft, toRight, toTop, toBottom);
      var exit = least === toLeft ? Math.PI : least === toRight ? 0 : least === toTop ? -Math.PI / 2 : Math.PI / 2;
      n.heading = turnToward(n.heading, exit, 2.5 * dt);
    }

    if (moving) {
      var nx = n.x + Math.cos(n.heading) * speed * dt;
      var ny = n.y + Math.sin(n.heading) * speed * dt;
      // The edges: bounce back in unless leaving.
      if (!n.leaving && n.entered) {
        if (nx < half) { nx = half; n.heading = Math.PI - n.heading; }
        if (nx > bounds.width - half) { nx = bounds.width - half; n.heading = Math.PI - n.heading; }
        if (ny < half) { ny = half; n.heading = -n.heading; }
        if (ny > bounds.height - half) { ny = bounds.height - half; n.heading = -n.heading; }
      }
      // A tab's box is a wall: turn along it.
      var wall = inObstacle({ x: nx, y: ny }, obstacles, half + 2);
      if (wall) {
        var cx = wall.x + wall.width / 2, cy = wall.y + wall.height / 2;
        var dx = (nx - cx) / (wall.width / 2 + half), dy = (ny - cy) / (wall.height / 2 + half);
        if (Math.abs(dx) > Math.abs(dy)) n.heading = Math.PI - n.heading; else n.heading = -n.heading;
        n.heading += r.range(-0.3, 0.3);
        nx = n.x; ny = n.y;
      }
      n.x = nx; n.y = ny;
    }
    if (n.entered && !inside && n.leaving && (n.x < -sp.size || n.x > bounds.width + sp.size || n.y < -sp.size || n.y > bounds.height + sp.size)) n.gone = true;
    return n;
  }

  /** Whether an insect that walked in has walked off the glass for good. */
  function offstage(insect, bounds) {
    var sp = SPECIES[insect.species] || SPECIES.ant;
    var m = sp.size * 2;
    return insect.gone || (insect.entered && (insect.x < -m || insect.x > bounds.width + m || insect.y < -m || insect.y > bounds.height + m));
  }

  // --- the tools ------------------------------------------------------------------
  /**
   * Each tool: what shape its strike has around the pointer, its cooldown
   * (s), the mark a miss leaves and the mark a hit leaves, its sound. A whip
   * bites at its tip, which is on the pointer, and along `reach` px of the
   * lash lying behind it (the page says which way); the spray slows what it
   * reaches, which dies when the slow wears off; the squeegee wipes marks
   * along its stroke and hits nothing.
   */
  var TOOLS = {
    whip: { label: "Whip", shape: { kind: "lash", tip: 14, width: 10, reach: 56 }, cooldown: 0.35, miss: "scratch", hit: "splat", sound: "whip" },
    swatter: { label: "Flyswatter", shape: { kind: "square", side: 56 }, cooldown: 0.25, miss: "crack", hit: "splat", sound: "swat" },
    slipper: { label: "Slipper", shape: { kind: "disc", r: 26 }, cooldown: 0.3, miss: "crack", hit: "splat", sound: "thud" },
    newspaper: { label: "Newspaper", shape: { kind: "rect", w: 120, h: 40 }, cooldown: 0.45, miss: "crack", hit: "splat", sound: "thud" },
    spray: { label: "Spray", shape: { kind: "sector", r: 140, angle: 70 }, cooldown: 0.6, miss: "mist", hit: "mist", sound: "spray", slow: 1.2 },
    squeegee: { label: "Squeegee", shape: { kind: "stroke", width: 60 }, cooldown: 0.1, miss: null, hit: null, sound: "squeegee", wipes: true },
  };
  var TOOL_NAMES = Object.keys(TOOLS);
  function isTool(name) { return typeof name === "string" && Object.prototype.hasOwnProperty.call(TOOLS, name); }

  function distToSegment(p, a, b) {
    var vx = b.x - a.x, vy = b.y - a.y;
    var len2 = vx * vx + vy * vy;
    var t = len2 === 0 ? 0 : Math.max(0, Math.min(1, ((p.x - a.x) * vx + (p.y - a.y) * vy) / len2));
    return dist(p, { x: a.x + vx * t, y: a.y + vy * t });
  }
  /** Whether the tool's shape, placed by the gesture, reaches an insect's body. */
  function reaches(tool, g, insect) {
    var sp = SPECIES[insect.species] || SPECIES.ant;
    var rad = sp.size / 2, shape = tool.shape, p = { x: insect.x, y: insect.y };
    switch (shape.kind) {
      // The tip, a disc on the pointer, and the lash from it back to its tail: precise at the tip, generous along the leather.
      case "lash": return dist(p, g) <= shape.tip + rad || distToSegment(p, g.from, g.to) <= shape.width / 2 + rad;
      case "square": return Math.abs(p.x - g.x) <= shape.side / 2 + rad && Math.abs(p.y - g.y) <= shape.side / 2 + rad;
      case "disc": return dist(p, g) <= shape.r + rad;
      case "rect": {
        var a = -(g.angle || 0), dx = p.x - g.x, dy = p.y - g.y;
        var lx = dx * Math.cos(a) - dy * Math.sin(a), ly = dx * Math.sin(a) + dy * Math.cos(a);
        return Math.abs(lx) <= shape.w / 2 + rad && Math.abs(ly) <= shape.h / 2 + rad;
      }
      case "sector": {
        var d = dist(p, g);
        if (d > shape.r + rad) return false;
        if (d <= rad) return true;
        var ang = Math.abs(wrapAngle(Math.atan2(p.y - g.y, p.x - g.x) - (g.angle || 0)));
        return ang <= (shape.angle / 2) * (Math.PI / 180) + Math.atan2(rad, Math.max(1, d));
      }
      default: return false;
    }
  }

  /**
   * One strike: which insects it reaches (dead ones are never hit again), the
   * marks it leaves (a splat per hit, else the miss's mark), its sound. The
   * gesture `g` is `{x, y, angle, bounds}` — the pointer and the way the blow
   * went — and for the whip also `from` (the tip, on the pointer) and `to`
   * (the tail, `reach` px back along the lash's lie), its `angle` the way the
   * lash flew: tail to tip. Null for a tool that is not one.
   */
  function strike(toolName, g, insects, now) {
    var tool = TOOLS[toolName];
    if (!tool) return null;
    if (tool.wipes) return { hits: [], marks: [], sound: tool.sound, wipes: true };
    var hits = [], marks = [];
    for (var i = 0; i < insects.length; i++) {
      var it = insects[i];
      if (it.state === "dead" || it.gone) continue;
      if (reaches(tool, g, it)) hits.push(it.id);
    }
    var seed = Math.floor((g.x || 0) * 31 + (g.y || 0) * 17 + now * 1000) >>> 0;
    if (tool.shape.kind === "sector") {
      marks.push(mist(g.x, g.y, g.angle || 0, tool.shape, seed, now));
    } else if (hits.length) {
      for (var h = 0; h < hits.length; h++) {
        var ins = insects.filter(function (x) { return x.id === hits[h]; })[0];
        marks.push(splat(ins.x, ins.y, ins.species, seed + h, now, g.angle, !!ins.golden));
      }
    } else if (tool.miss === "crack") {
      marks.push(crack(g.x, g.y, seed, g.bounds, now));
    } else if (tool.miss === "scratch") {
      marks.push(scratch(g.from, g.to, seed, now));
    }
    return { hits: hits, marks: marks, sound: tool.sound, slow: tool.slow || 0 };
  }

  // --- the marks on the glass ---------------------------------------------------------
  var TTL = { crack: 20, scratch: 12, mist: 8, splat: null, smear: 5 };
  var markSeq = 0;
  function nextMarkId() { markSeq += 1; return "m" + markSeq; }

  function clampPoint(p, bounds) {
    if (!bounds) return p;
    return { x: Math.max(0, Math.min(bounds.width, p.x)), y: Math.max(0, Math.min(bounds.height, p.y)) };
  }
  /**
   * A crack, the way glass breaks under a blow: a crushed core, rays that
   * run out from it thinning as they go (a few forking once), and partial
   * rings that join neighbouring rays — a spider's web, kept inside the
   * bounds. `polylines` holds every line for clipping; `rays` carry their
   * widths and `rings` are the joins.
   */
  function crack(x, y, seed, bounds, now) {
    var r = rng(seed), count = r.int(6, 10), rays = [], polylines = [], reach = r.range(34, 70);
    var i, s;
    for (i = 0; i < count; i++) {
      var angle = (i / count) * 6.283 + r.range(-0.3, 0.3), p = { x: x, y: y };
      var segments = r.int(4, 7), total = reach * r.range(0.55, 1.25), points = [clampPoint(p, bounds)], widths = [], run = 0;
      for (s = 0; s < segments; s++) {
        angle += r.range(-0.35, 0.35);
        var len = (total / segments) * r.range(0.6, 1.4);
        p = { x: p.x + Math.cos(angle) * len, y: p.y + Math.sin(angle) * len };
        run += len;
        points.push(clampPoint(p, bounds));
        widths.push(Math.max(0.35, 1.7 * (1 - run / (total * 1.15))));
      }
      rays.push({ points: points, widths: widths, length: total });
      polylines.push(points);
      // A fork off the ray, once in three, from its middle.
      if (r.chance(0.33) && points.length > 3) {
        var at = r.int(1, points.length - 2), fp = points[at], fa = angle + r.range(0.5, 1.1) * (r.chance(0.5) ? 1 : -1);
        var fork = [fp], fw = [];
        for (s = 0; s < 3; s++) {
          fa += r.range(-0.3, 0.3);
          fp = { x: fp.x + Math.cos(fa) * r.range(6, 14), y: fp.y + Math.sin(fa) * r.range(6, 14) };
          fork.push(clampPoint(fp, bounds));
          fw.push(Math.max(0.3, 0.9 - s * 0.25));
        }
        rays.push({ points: fork, widths: fw, length: 0 });
        polylines.push(fork);
      }
    }
    // Rings: a few partial rings that step from ray to ray at a share of each ray's length.
    var rings = [], ringCount = r.int(1, 3);
    for (i = 0; i < ringCount; i++) {
      var share = 0.22 + i * 0.26 + r.range(-0.05, 0.05), from = r.int(0, count - 1), span = r.int(3, count), ring = [];
      for (s = 0; s <= span; s++) {
        var ray = rays[(from + s) % count], along = Math.max(1, Math.round(share * (ray.points.length - 1)));
        var a = ray.points[along - 1], b = ray.points[along], f = r.range(0.2, 0.8);
        ring.push(clampPoint({ x: a.x + (b.x - a.x) * f + r.range(-1.5, 1.5), y: a.y + (b.y - a.y) * f + r.range(-1.5, 1.5) }, bounds));
      }
      rings.push(ring);
      polylines.push(ring);
    }
    return { id: nextMarkId(), kind: "crack", x: x, y: y, seed: seed, rays: rays, rings: rings, core: r.range(2.5, 5.5), polylines: polylines, born: now, ttl: TTL.crack };
  }
  /**
   * Scratches where a whip's lash lay, from its tip (`from`) back to its
   * tail: two or three hairlines, each bowed a little, thin at both ends and
   * widest in the middle, with a few chips near the tip, where the lash bit
   * hardest.
   */
  function scratch(from, to, seed, now) {
    var r = rng(seed), lines = r.int(2, 3), polylines = [], widths = [], chips = [];
    var dx = to.x - from.x, dy = to.y - from.y, len = Math.max(1, Math.sqrt(dx * dx + dy * dy));
    var nx = -dy / len, ny = dx / len, steps = 10;
    for (var i = 0; i < lines; i++) {
      var off = (i - (lines - 1) / 2) * 3.5 + r.range(-1.2, 1.2), bow = r.range(-6, 6), line = [], w = [];
      var start = r.range(0, 0.12), end = r.range(0.88, 1);
      for (var s = 0; s <= steps; s++) {
        var t = start + (end - start) * (s / steps), curve = bow * 4 * t * (1 - t), wob = Math.sin(t * 11 + i) * 0.6;
        line.push({ x: from.x + dx * t + nx * (off + curve + wob), y: from.y + dy * t + ny * (off + curve + wob) });
        if (s < steps) w.push(0.25 + 0.95 * Math.sin(Math.PI * (s + 0.5) / steps));
      }
      polylines.push(line);
      widths.push(w);
    }
    var chipCount = r.int(2, 4);
    for (var c = 0; c < chipCount; c++) {
      var ct = r.range(0.04, 0.36);
      chips.push({ x: from.x + dx * ct + nx * r.range(-3, 3), y: from.y + dy * ct + ny * r.range(-3, 3), r: r.range(0.6, 1.4) });
    }
    return { id: nextMarkId(), kind: "scratch", x: (from.x + to.x) / 2, y: (from.y + to.y) / 2, seed: seed, polylines: polylines, widths: widths, chips: chips, born: now, ttl: TTL.scratch };
  }
  /**
   * A splat where an insect was, thrown the way the blow went: a body-sized
   * blob with a darker heart, droplets that stretch along the swing, a few
   * streaks, and what is left of the legs.
   */
  function splat(x, y, species, seed, now, angle, golden) {
    var sp = SPECIES[species] || SPECIES.ant, r = rng(seed), dir = typeof angle === "number" ? angle : r.range(0, 6.283);
    var drops = [{ dx: 0, dy: 0, r: sp.size * 0.42, ex: 1, rot: 0 }];
    var n = r.int(6, 11);
    for (var i = 0; i < n; i++) {
      var a = dir + r.range(-0.75, 0.75) + (r.chance(0.2) ? Math.PI : 0), d = r.range(sp.size * 0.3, sp.size * 1.1);
      drops.push({ dx: Math.cos(a) * d, dy: Math.sin(a) * d, r: r.range(1.5, sp.size * 0.2), ex: r.range(1.1, 2.2), rot: a });
    }
    var streaks = [], sc = r.int(1, 3);
    for (var k = 0; k < sc; k++) {
      var sa = dir + r.range(-0.5, 0.5), sl = r.range(sp.size * 0.5, sp.size * 1.4);
      streaks.push({ x1: x + Math.cos(sa) * sp.size * 0.2, y1: y + Math.sin(sa) * sp.size * 0.2, x2: x + Math.cos(sa) * sl, y2: y + Math.sin(sa) * sl, w: r.range(0.8, 2) });
    }
    var legs = [], lc = Math.min(4, sp.legs || 4);
    for (var l = 0; l < lc; l++) {
      var la = r.range(0, 6.283), ll = r.range(sp.size * 0.3, sp.size * 0.55);
      legs.push({ x1: x + Math.cos(la) * sp.size * 0.25, y1: y + Math.sin(la) * sp.size * 0.25, x2: x + Math.cos(la) * ll, y2: y + Math.sin(la) * ll });
    }
    return { id: nextMarkId(), kind: "splat", x: x, y: y, seed: seed, drops: drops, streaks: streaks, legs: legs, color: golden ? GOLDEN.splat : sp.splat, body: golden ? GOLDEN.color : sp.color, born: now, ttl: TTL.splat };
  }
  /** A spray's mist: droplets through the sector. */
  function mist(x, y, angle, shape, seed, now) {
    var r = rng(seed), drops = [], n = r.int(14, 24), half = (shape.angle / 2) * (Math.PI / 180);
    for (var i = 0; i < n; i++) {
      var a = angle + r.range(-half, half), d = r.range(shape.r * 0.15, shape.r);
      drops.push({ dx: Math.cos(a) * d, dy: Math.sin(a) * d, r: r.range(1.2, 3.6), delay: d / shape.r });
    }
    return { id: nextMarkId(), kind: "mist", x: x, y: y, seed: seed, drops: drops, angle: angle, reach: shape.r, born: now, ttl: TTL.mist };
  }
  /** A squeegee's wet streak along its stroke, which dries in a few seconds. */
  function smear(path, width, seed, now) {
    var pts = (path || []).slice(0, 40), x = pts.length ? pts[0].x : 0, y = pts.length ? pts[0].y : 0;
    return { id: nextMarkId(), kind: "smear", x: x, y: y, seed: seed, path: pts, width: width || 60, born: now, ttl: TTL.smear };
  }

  /** The marks the glass keeps when glass marks are off: a blow's crack and a whip's scratch are let go, what an insect left stays. */
  function keepMarks(marks, marksOn) {
    var list = marks || [];
    if (marksOn) return list;
    return list.filter(function (m) { return m.kind !== "crack" && m.kind !== "scratch"; });
  }
  /** The marks still on the glass. */
  function aliveMarks(marks, now) {
    return marks.filter(function (m) { return m.ttl === null || now - m.born < m.ttl; });
  }
  /** A mark's opacity: whole until its last four seconds, then fading. */
  function markOpacity(mark, now) {
    if (mark.ttl === null) return 1;
    var left = mark.ttl - (now - mark.born);
    return Math.max(0, Math.min(1, left / 4));
  }
  /** The marks a squeegee's stroke leaves behind: those its path never came near. */
  function wipe(marks, path, width) {
    if (!path || path.length < 1) return marks;
    var reach = (width || 60) / 2 + 12;
    return marks.filter(function (m) {
      for (var i = 0; i < path.length; i++) {
        var a = path[i], b = path[Math.min(i + 1, path.length - 1)];
        if (distToSegment({ x: m.x, y: m.y }, a, b) <= reach) return false;
      }
      return true;
    });
  }
  /** Polylines with every point inside an obstacle dropped: a crack never crosses a tab. */
  function clipOutside(polylines, obstacles) {
    if (!obstacles || !obstacles.length) return polylines;
    var out = [];
    for (var i = 0; i < polylines.length; i++) {
      var run = [];
      for (var j = 0; j < polylines[i].length; j++) {
        if (inObstacle(polylines[i][j], obstacles, 0)) { if (run.length > 1) out.push(run); run = []; }
        else run.push(polylines[i][j]);
      }
      if (run.length > 1) out.push(run);
    }
    return out;
  }

  // --- the score ----------------------------------------------------------------------
  /** A hit: the species' points times the streak (five at most), kept within two seconds of the last. */
  function scoreHit(score, insect, now) {
    var sp = SPECIES[insect.species] || SPECIES.ant;
    var streak = now - (score.lastHitAt || -1e9) <= 2 ? (score.streak || 0) + 1 : 1;
    var points = sp.points * (insect.golden ? GOLDEN.points : 1) * Math.min(5, streak);
    var total = (score.score || 0) + points;
    return { score: total, streak: streak, best: Math.max(score.best || 0, total), lastHitAt: now, lastPoints: points };
  }
  /** A miss ends the streak. */
  function scoreMiss(score) {
    return { score: score.score || 0, streak: 0, best: score.best || 0, lastHitAt: score.lastHitAt || 0, lastPoints: 0 };
  }
  /** The word a streak earns the moment it is reached — once, at three and at five — or none. */
  function comboWord(streak) {
    return streak === 3 ? "Combo!" : streak === 5 ? "On a roll!" : null;
  }
  /** Whether a best moved past one that was already there: the first best is no news. */
  function newBest(prev, next) {
    return prev > 0 && next > prev;
  }
  /** The strip's line about the workspace. */
  function waitWords(working) {
    var n = Math.max(0, working || 0);
    if (n === 0) return "Nothing is running — hunt anyway.";
    return (n === 1 ? "One thing is running" : n + " things are running") + " — hunt while you wait.";
  }

  // --- the strip ------------------------------------------------------------------------
  /** Where the strip stands: the right edge, mid-height; the left when a tab covers that; the bottom when both are covered. */
  function stripPlacement(viewport, obstacles, size) {
    var w = size.width, h = Math.min(size.height, Math.max(120, viewport.height - 16));
    var right = { x: Math.max(0, viewport.width - w - 8), y: Math.max(0, Math.round((viewport.height - h) / 2)), width: w, height: h };
    var left = { x: 8, y: right.y, width: w, height: h };
    var bottom = { x: Math.max(0, Math.round((viewport.width - w) / 2)), y: Math.max(0, viewport.height - h - 8), width: w, height: h };
    var places = [right, left, bottom];
    for (var i = 0; i < places.length; i++) if (!overlapsAny(places[i], obstacles)) return places[i];
    return right;
  }
  function overlapsAny(a, list) {
    for (var i = 0; i < (list || []).length; i++) {
      var b = list[i];
      if (a.x < b.x + b.width && a.x + a.width > b.x && a.y < b.y + b.height && a.y + a.height > b.y) return true;
    }
    return false;
  }
  /** The solid regions the host is told: the strip, with its corner. */
  function solidRects(strip) {
    return [{ x: Math.round(strip.x), y: Math.round(strip.y), width: Math.round(strip.width), height: Math.round(strip.height), radius: 10 }];
  }

  // --- the store ----------------------------------------------------------------------------
  /** What the store kept, read only as far as its shape allows. */
  function readSaved(raw) {
    var out = { best: 0, muted: false, tool: "swatter", difficulty: "busy", marks: false };
    var parsed;
    try { parsed = JSON.parse(raw); } catch (e) { return out; }
    if (!parsed || typeof parsed !== "object") return out;
    if (typeof parsed.best === "number" && parsed.best >= 0 && parsed.best < 1e9) out.best = Math.floor(parsed.best);
    if (parsed.muted === true) out.muted = true;
    if (parsed.marks === true) out.marks = true;
    if (isTool(parsed.tool)) out.tool = parsed.tool;
    if (typeof parsed.difficulty === "string" && DIFFICULTIES[parsed.difficulty]) out.difficulty = parsed.difficulty;
    return out;
  }

  global.BugHunt = {
    rng: rng,
    SPECIES: SPECIES,
    SPECIES_NAMES: SPECIES_NAMES,
    TOOLS: TOOLS,
    TOOL_NAMES: TOOL_NAMES,
    DIFFICULTIES: DIFFICULTIES,
    DIFFICULTY_NAMES: DIFFICULTY_NAMES,
    HOP_TIME: HOP_TIME,
    GOLDEN: GOLDEN,
    SCARE_TTL: SCARE_TTL,
    goldenTurn: goldenTurn,
    isTool: isTool,
    maxInsects: maxInsects,
    spawnInterval: spawnInterval,
    speedScale: speedScale,
    spawn: spawn,
    step: step,
    offstage: offstage,
    reaches: reaches,
    strike: strike,
    crack: crack,
    scratch: scratch,
    splat: splat,
    mist: mist,
    smear: smear,
    keepMarks: keepMarks,
    aliveMarks: aliveMarks,
    markOpacity: markOpacity,
    wipe: wipe,
    clipOutside: clipOutside,
    scoreHit: scoreHit,
    scoreMiss: scoreMiss,
    comboWord: comboWord,
    newBest: newBest,
    waitWords: waitWords,
    stripPlacement: stripPlacement,
    solidRects: solidRects,
    readSaved: readSaved,
  };
})(typeof window !== "undefined" ? window : this);
