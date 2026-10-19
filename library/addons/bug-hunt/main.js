// Bug hunt: insects on a pane over the whole app. The host owns the mode —
// in work the pane is click-through and only the strip is solid; in hunt the
// pane takes the pointer and the cursor is the tool. This file draws the
// insects, the marks and the strip, plays the sounds, and talks to the host;
// every rule is `logic.js`'s (`BugHunt`).
(function () {
  "use strict";
  var H = window.BugHunt;
  function $(id) { return document.getElementById(id); }
  var glass = $("glass"), bugsEl = $("bugs"), marksEl = $("marks"), fxEl = $("fx"), strip = $("strip");
  var hand = $("hand"), handUse = $("handUse"), handSvg = hand.querySelector("svg"), trail = $("trail");
  var scoreEl = $("score"), streakEl = $("streak"), bestEl = $("best"), waitEl = $("wait");
  var modeBtn = $("mode"), diffBtn = $("difficulty"), paceValue = $("paceValue"), muteBtn = $("mute"), muteIcon = $("muteIcon"), leaveBtn = $("leave");
  var toolBtns = Array.prototype.slice.call(strip.querySelectorAll(".tool"));
  var SVG = "http://www.w3.org/2000/svg", XLINK = "http://www.w3.org/1999/xlink";
  var STRIP_WIDTH = 56;
  var reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  var state = {
    mode: "work", tool: "swatter", muted: false, difficulty: "busy",
    bounds: { width: window.innerWidth, height: window.innerHeight }, obstacles: [], cuts: false,
    working: 0, visible: true, score: { score: 0, streak: 0, best: 0, lastHitAt: -1e9, lastPoints: 0 },
    insects: [], marks: [], nextSpawn: 0.6, nextId: 1, cooldownUntil: 0, strip: null,
  };
  var r = H.rng((Date.now() ^ (Math.random() * 1e9)) >>> 0);
  var nodes = {};        // insect id → its element
  var markNodes = {};    // mark id → its group
  var pointer = null;    // the pointer on the glass in hunt mode, or null
  var lastMove = null;   // the last two points, for a swing's direction
  var stroke = null;     // a whip's or a squeegee's stroke in progress
  var history = [];      // where the pointer has been, newest last: the whip's lash lies along it
  var handAngle = 0, handTarget = 0;   // the tool in hand turns into the swing, a touch behind the hand
  var ribbon = null;     // the whip's lash on the trail layer, once drawn
  var ribbonHeld = null; // the lash pulled straight by a crack: { pts, until }
  var band = null;       // the squeegee's wet band while it drags
  var canStore = false, saveTimer = null;
  var clock = 0, raf = null, now = 0;

  // --- words and the store ------------------------------------------------------
  function scheme(s) { document.documentElement.dataset.scheme = s; }
  function save() {
    if (!canStore) return;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(function () {
      saveTimer = null;
      bisa.storage.set("state", JSON.stringify({ best: state.score.best, muted: state.muted, tool: state.tool, difficulty: state.difficulty })).catch(function () {});
    }, 300);
  }
  function renderHud() {
    scoreEl.textContent = String(state.score.score);
    streakEl.textContent = state.score.streak > 1 ? "×" + Math.min(5, state.score.streak) : "";
    bestEl.textContent = "best " + state.score.best;
    waitEl.textContent = "";
    var n = document.createElement("b");
    n.textContent = String(state.working);
    waitEl.appendChild(n);
    waitEl.appendChild(document.createTextNode(state.working === 1 ? "running" : "running"));
    waitEl.title = H.waitWords(state.working);
    waitEl.setAttribute("aria-label", H.waitWords(state.working));
    waitEl.dataset.running = state.working > 0 ? "true" : "false";
  }
  function renderStrip() {
    toolBtns.forEach(function (b) { b.setAttribute("aria-pressed", b.dataset.tool === state.tool ? "true" : "false"); });
    document.body.dataset.tool = state.tool;
    document.body.dataset.mode = state.mode;
    modeBtn.setAttribute("aria-pressed", state.mode === "hunt" ? "true" : "false");
    modeBtn.textContent = state.mode === "hunt" ? "Work" : "Hunt";
    modeBtn.title = state.mode === "hunt" ? "Hand the app the pointer back (Esc)" : "Take the pointer and hunt";
    paceValue.textContent = state.difficulty.charAt(0).toUpperCase() + state.difficulty.slice(1);
    muteIcon.setAttribute("href", state.muted ? "#t-muted" : "#t-sound");
    muteBtn.setAttribute("aria-pressed", state.muted ? "true" : "false");
    muteBtn.setAttribute("aria-label", state.muted ? "Unmute" : "Mute");
    muteBtn.title = state.muted ? "Unmute" : "Mute";
    renderHand();
  }

  // --- the strip's place -------------------------------------------------------------
  var placed = null;
  function placeStrip() {
    strip.classList.toggle("tight", state.bounds.height < 520);
    var h = strip.getBoundingClientRect().height || 360;
    var rect = H.stripPlacement(state.bounds, state.obstacles, { width: STRIP_WIDTH, height: h });
    strip.style.left = rect.x + "px";
    strip.style.top = rect.y + "px";
    state.strip = rect;
    var key = rect.x + ":" + rect.y + ":" + rect.width + ":" + rect.height;
    if (key === placed) return;
    placed = key;
    bisa.scene.solid(H.solidRects(rect)).catch(function () {});
  }

  // --- sound ------------------------------------------------------------------------
  var audio = { ctx: null, master: null, whine: null };
  function ensureAudio() {
    if (audio.ctx) return audio.ctx;
    var Ctx = window.AudioContext || window.webkitAudioContext;
    if (!Ctx) return null;
    audio.ctx = new Ctx();
    audio.master = audio.ctx.createGain();
    audio.master.gain.value = state.muted ? 0 : 0.6;
    audio.master.connect(audio.ctx.destination);
    return audio.ctx;
  }
  function noise(ctx, seconds) {
    var frames = Math.floor(ctx.sampleRate * seconds), buffer = ctx.createBuffer(1, frames, ctx.sampleRate), data = buffer.getChannelData(0);
    for (var i = 0; i < frames; i++) data[i] = Math.random() * 2 - 1;
    var src = ctx.createBufferSource();
    src.buffer = buffer;
    return src;
  }
  function burst(ctx, t0, kind, freq, q, seconds, peak, decay) {
    var src = noise(ctx, seconds), filter = ctx.createBiquadFilter(), gain = ctx.createGain();
    filter.type = kind; filter.frequency.value = freq; filter.Q.value = q || 1;
    gain.gain.setValueAtTime(0.0001, t0);
    gain.gain.exponentialRampToValueAtTime(peak, t0 + 0.004);
    gain.gain.exponentialRampToValueAtTime(0.0001, t0 + decay);
    src.connect(filter); filter.connect(gain); gain.connect(audio.master);
    src.start(t0); src.stop(t0 + seconds);
  }
  function tone(ctx, t0, type, from, to, seconds, peak) {
    var osc = ctx.createOscillator(), gain = ctx.createGain();
    osc.type = type; osc.frequency.setValueAtTime(from, t0); osc.frequency.exponentialRampToValueAtTime(Math.max(1, to), t0 + seconds);
    gain.gain.setValueAtTime(0.0001, t0);
    gain.gain.exponentialRampToValueAtTime(peak, t0 + 0.006);
    gain.gain.exponentialRampToValueAtTime(0.0001, t0 + seconds);
    osc.connect(gain); gain.connect(audio.master);
    osc.start(t0); osc.stop(t0 + seconds + 0.02);
  }
  function swish(ctx, t0) {
    var src = noise(ctx, 0.1), filter = ctx.createBiquadFilter(), gain = ctx.createGain();
    filter.type = "bandpass"; filter.Q.value = 0.7;
    filter.frequency.setValueAtTime(500, t0); filter.frequency.exponentialRampToValueAtTime(1800, t0 + 0.08);
    gain.gain.setValueAtTime(0.0001, t0);
    gain.gain.exponentialRampToValueAtTime(0.22, t0 + 0.03);
    gain.gain.exponentialRampToValueAtTime(0.0001, t0 + 0.09);
    src.connect(filter); filter.connect(gain); gain.connect(audio.master);
    src.start(t0); src.stop(t0 + 0.1);
  }
  function play(name, delay) {
    if (state.muted) return;
    var ctx = ensureAudio();
    if (!ctx) return;
    if (ctx.state === "suspended") ctx.resume().catch(function () {});
    var t0 = ctx.currentTime + (delay || 0);
    switch (name) {
      case "swish": swish(ctx, t0); break;
      case "whip": burst(ctx, t0, "highpass", 2400, 0.8, 0.12, 0.9, 0.07); tone(ctx, t0, "sine", 1800, 300, 0.05, 0.15); break;
      case "swat": burst(ctx, t0, "lowpass", 900, 1, 0.1, 0.8, 0.06); break;
      case "thud": tone(ctx, t0, "sine", 110, 40, 0.14, 0.9); burst(ctx, t0, "lowpass", 300, 1, 0.08, 0.3, 0.05); break;
      case "splat": burst(ctx, t0, "lowpass", 500, 1, 0.18, 0.7, 0.14); tone(ctx, t0 + 0.02, "sine", 240, 90, 0.12, 0.3); break;
      case "crack": burst(ctx, t0, "highpass", 4200, 2, 0.04, 0.7, 0.025); burst(ctx, t0 + 0.035, "highpass", 5200, 2, 0.04, 0.5, 0.02); burst(ctx, t0 + 0.08, "highpass", 3600, 2, 0.05, 0.4, 0.03); break;
      case "spray": burst(ctx, t0, "bandpass", 3200, 0.7, 0.3, 0.5, 0.26); break;
      case "squeegee": tone(ctx, t0, "triangle", 420, 1500, 0.22, 0.12); burst(ctx, t0, "bandpass", 1200, 1.2, 0.22, 0.2, 0.2); break;
      default: break;
    }
  }
  function whine(on, nearness) {
    var ctx = audio.ctx;
    if (!ctx || state.muted) on = false;
    if (on && !audio.whine) {
      var osc = ctx.createOscillator(), vib = ctx.createOscillator(), vibGain = ctx.createGain(), gain = ctx.createGain();
      osc.type = "sawtooth"; osc.frequency.value = 620;
      vib.frequency.value = 9; vibGain.gain.value = 18;
      vib.connect(vibGain); vibGain.connect(osc.frequency);
      gain.gain.value = 0.0001;
      osc.connect(gain); gain.connect(audio.master);
      osc.start(); vib.start();
      audio.whine = { osc: osc, vib: vib, gain: gain };
    }
    if (audio.whine) {
      var target = on ? 0.02 + 0.04 * Math.max(0, Math.min(1, nearness || 0)) : 0.0001;
      audio.whine.gain.gain.setTargetAtTime(target, ctx.currentTime, 0.05);
    }
  }
  function setMuted(m) {
    state.muted = m;
    if (audio.master) audio.master.gain.setTargetAtTime(m ? 0 : 0.6, audio.ctx.currentTime, 0.02);
    renderStrip();
    save();
  }

  // --- the insects on the glass ----------------------------------------------------------
  var WEIGHTS = [["spider", 1], ["cockroach", 2], ["beetle", 1], ["ant", 2], ["fly", 2], ["mosquito", 1], ["cricket", 1.5]];
  function pickSpecies() {
    var total = 0, i;
    for (i = 0; i < WEIGHTS.length; i++) total += WEIGHTS[i][1];
    var at = r.next() * total;
    for (i = 0; i < WEIGHTS.length; i++) { at -= WEIGHTS[i][1]; if (at <= 0) return WEIGHTS[i][0]; }
    return "ant";
  }
  function alive() { return state.insects.filter(function (b) { return b.state !== "dead"; }).length; }
  /** What the insects keep off: the tabs the host named, and the strip, which the pointer never reaches on the glass. */
  function walls() { return state.strip ? state.obstacles.concat([state.strip]) : state.obstacles; }
  function arrive() {
    var species = pickSpecies();
    var leader = H.spawn(species, state.bounds, walls(), r, now, state.nextId++);
    state.insects.push(leader);
    var file = H.SPECIES[species].file || 1;
    for (var k = 1; k < file; k++) {
      var ant = H.spawn(species, state.bounds, walls(), r, now, state.nextId++);
      ant.x = leader.x - Math.cos(leader.heading) * k * 22;
      ant.y = leader.y - Math.sin(leader.heading) * k * 22;
      ant.heading = leader.heading;
      ant.leader = leader.id;
      state.insects.push(ant);
    }
  }
  function leaderOf(id) {
    for (var i = 0; i < state.insects.length; i++) if (state.insects[i].id === id && state.insects[i].state !== "dead") return state.insects[i];
    return null;
  }
  function nodeFor(bug) {
    var el = nodes[bug.id];
    if (el) return el;
    var sp = H.SPECIES[bug.species];
    el = document.createElement("div");
    el.className = "bug";
    el.style.setProperty("--size", sp.size + "px");
    el.style.setProperty("--color", sp.color);
    var shadow = document.createElement("span");
    shadow.className = "shadow";
    el.appendChild(shadow);
    var svg = document.createElementNS(SVG, "svg");
    svg.setAttribute("viewBox", "-20 -20 40 40");
    var use = document.createElementNS(SVG, "use");
    use.setAttribute("href", "#s-" + bug.species);
    use.setAttributeNS(XLINK, "xlink:href", "#s-" + bug.species);
    svg.appendChild(use);
    el.appendChild(svg);
    bugsEl.appendChild(el);
    nodes[bug.id] = el;
    return el;
  }
  function drawBug(bug) {
    var el = nodeFor(bug);
    var pose = "translate3d(" + bug.x.toFixed(1) + "px," + bug.y.toFixed(1) + "px,0) rotate(" + (bug.heading * 57.2958).toFixed(1) + "deg)";
    el.style.setProperty("--pose", pose);
    el.style.transform = pose;
    el.style.setProperty("--z", bug.z.toFixed(1));
    el.dataset.airborne = bug.z > 0.5 ? "true" : "false";
    if (bug.by && el.dataset.by !== bug.by) el.dataset.by = bug.by;
    if (el.dataset.state !== bug.state) {
      el.dataset.state = bug.state;
      // The legs stride as fast as the insect goes: a dart is a blur, a beetle plods.
      var pace = bug.state === "dart" ? bug.speed * 2.2 : bug.speed;
      el.style.setProperty("--stride", Math.max(80, Math.round(300 - pace * 1.1)) + "ms");
    }
  }
  function dropBug(bug) {
    var el = nodes[bug.id];
    if (el) { el.remove(); delete nodes[bug.id]; }
  }

  // --- the marks on the glass -------------------------------------------------------------
  function pathOf(polylines) {
    var d = "";
    for (var i = 0; i < polylines.length; i++) {
      var line = polylines[i];
      for (var j = 0; j < line.length; j++) d += (j === 0 ? "M" : "L") + line[j].x.toFixed(1) + " " + line[j].y.toFixed(1);
    }
    return d;
  }
  function el(name, attrs, parent) {
    var node = document.createElementNS(SVG, name);
    for (var k in attrs) node.setAttribute(k, attrs[k]);
    if (parent) parent.appendChild(node);
    return node;
  }
  function inWall(p) { return H.clipOutside([[p, p]], state.obstacles).length === 0; }
  /** A round mark: an ellipse of one radius. */
  function dot(cx, cy, r, attrs, parent) {
    var a = { cx: cx.toFixed(1), cy: cy.toFixed(1), rx: r.toFixed(1), ry: r.toFixed(1) };
    for (var k in attrs) a[k] = attrs[k];
    return el("ellipse", a, parent);
  }
  /** A line per segment, so a ray can thin toward its tip: a dark underline for a light field, a pale line for a dark one. */
  function taperedLines(g, points, widths, underScale, overScale) {
    for (var i = 0; i + 1 < points.length; i++) {
      var a = points[i], b = points[i + 1];
      if (inWall(a) || inWall(b)) continue;
      var w = widths[Math.min(i, widths.length - 1)];
      el("line", { class: "under", x1: a.x.toFixed(1), y1: a.y.toFixed(1), x2: b.x.toFixed(1), y2: b.y.toFixed(1), "stroke-width": (w * underScale).toFixed(2) }, g);
      el("line", { class: "over", x1: a.x.toFixed(1), y1: a.y.toFixed(1), x2: b.x.toFixed(1), y2: b.y.toFixed(1), "stroke-width": (w * overScale).toFixed(2) }, g);
    }
  }
  function drawMark(mark) {
    if (markNodes[mark.id]) return;
    var g = el("g", { class: "mark mark-" + mark.kind });
    var i;
    if (mark.kind === "crack") {
      for (i = 0; i < mark.rays.length; i++) taperedLines(g, mark.rays[i].points, mark.rays[i].widths, 2.2, 1);
      for (i = 0; i < mark.rings.length; i++) {
        var d = pathOf(H.clipOutside([mark.rings[i]], state.obstacles));
        el("path", { class: "ring-under", d: d }, g);
        el("path", { class: "ring-over", d: d }, g);
      }
      dot(mark.x, mark.y, mark.core, { class: "core" }, g);
      dot(mark.x - mark.core * 0.3, mark.y - mark.core * 0.3, mark.core * 0.3, { class: "core-glint" }, g);
    } else if (mark.kind === "scratch") {
      for (i = 0; i < mark.polylines.length; i++) taperedLines(g, mark.polylines[i], mark.widths[i], 2.4, 0.8);
      for (i = 0; i < mark.chips.length; i++) dot(mark.chips[i].x, mark.chips[i].y, mark.chips[i].r, { class: "chip" }, g);
    } else if (mark.kind === "splat") {
      for (i = 0; i < mark.streaks.length; i++) {
        var st = mark.streaks[i];
        el("line", { class: "streak", x1: st.x1.toFixed(1), y1: st.y1.toFixed(1), x2: st.x2.toFixed(1), y2: st.y2.toFixed(1), stroke: mark.color, "stroke-width": st.w.toFixed(1) }, g);
      }
      for (i = 1; i < mark.drops.length; i++) {
        var dr = mark.drops[i];
        el("ellipse", { class: "drop", cx: (mark.x + dr.dx).toFixed(1), cy: (mark.y + dr.dy).toFixed(1), rx: (dr.r * dr.ex).toFixed(1), ry: dr.r.toFixed(1), fill: mark.color, transform: "rotate(" + (dr.rot * 57.2958).toFixed(1) + " " + (mark.x + dr.dx).toFixed(1) + " " + (mark.y + dr.dy).toFixed(1) + ")" }, g);
      }
      var heart = mark.drops[0];
      dot(mark.x, mark.y, heart.r, { class: "heart", fill: mark.color }, g);
      dot(mark.x, mark.y, heart.r * 0.55, { fill: mark.body, opacity: "0.55" }, g);
      for (i = 0; i < mark.legs.length; i++) {
        var lg = mark.legs[i];
        el("line", { class: "limb", x1: lg.x1.toFixed(1), y1: lg.y1.toFixed(1), x2: lg.x2.toFixed(1), y2: lg.y2.toFixed(1), stroke: mark.body }, g);
      }
      dot(mark.x - heart.r * 0.3, mark.y - heart.r * 0.35, heart.r * 0.28, { class: "shine" }, g);
    } else if (mark.kind === "mist") {
      for (i = 0; i < mark.drops.length; i++) {
        var md = mark.drops[i];
        var c = dot(mark.x + md.dx, mark.y + md.dy, md.r * (1 - md.delay * 0.45), {}, g);
        c.style.setProperty("--d", md.delay.toFixed(2));
        c.setAttribute("opacity", (0.95 - md.delay * 0.5).toFixed(2));
      }
    } else if (mark.kind === "smear") {
      var pd = pathOf([mark.path]);
      el("path", { class: "wet", d: pd, "stroke-width": mark.width }, g);
      el("path", { class: "edge", d: pd, transform: "translate(0 " + (-mark.width / 2 + 1).toFixed(1) + ")" }, g);
    }
    marksEl.appendChild(g);
    markNodes[mark.id] = g;
  }
  // --- the tool in hand ---------------------------------------------------------------------
  /** The tools that turn into the swing, and the symbol's own angle when the swing points right. */
  var DIRECTIONAL = { newspaper: 0, spray: 0, squeegee: Math.PI / 2, whip: Math.PI / 4 };
  var LASH = 170;   // px of lash behind the grip
  var SWINGS = ["slam", "stomp", "whack", "flick", "puff", "press"];
  function f(n) { return n.toFixed(1); }
  function wrap(a) { while (a > Math.PI) a -= 2 * Math.PI; while (a < -Math.PI) a += 2 * Math.PI; return a; }
  function renderHand() {
    hand.dataset.tool = state.tool;
    handUse.setAttribute("href", "#x-" + state.tool);
    handUse.setAttributeNS(XLINK, "xlink:href", "#x-" + state.tool);
    trail.classList.toggle("whip", state.tool === "whip");
    hand.classList.remove("down");
  }
  function hideHand() { hand.hidden = true; trail.classList.remove("on"); }
  function handPlay(name) {
    SWINGS.forEach(function (c) { hand.classList.remove(c); });
    void hand.offsetWidth;
    hand.classList.add(name);
  }
  hand.addEventListener("animationend", function (e) { if (e.target === handSvg) SWINGS.forEach(function (c) { hand.classList.remove(c); }); });
  /** Every frame in hunt mode: the tool sits on the pointer and turns into the swing a touch behind it; a whip's lash lies along the way the hand came. */
  function updateHand() {
    var show = state.mode === "hunt" && !!pointer;
    if (hand.hidden !== !show) hand.hidden = !show;
    if (!show) { trail.classList.remove("on"); return; }
    if (Object.prototype.hasOwnProperty.call(DIRECTIONAL, state.tool)) handAngle += wrap(handTarget + DIRECTIONAL[state.tool] - handAngle) * 0.3;
    else handAngle = 0;
    var rot = handAngle, mirror = "";
    if (state.tool === "spray") {
      // A can is held near upright and tilted at most so far; to spray the other way the hand turns it over.
      if (Math.cos(handAngle) < 0) { mirror = " scaleX(-1)"; rot = wrap(Math.PI - handAngle); }
      rot = Math.max(-0.8, Math.min(0.8, rot));
    }
    hand.style.transform = "translate3d(" + f(pointer.x) + "px," + f(pointer.y) + "px,0)" + mirror + " rotate(" + (rot * 57.2958).toFixed(1) + "deg)";
    if (state.tool === "whip") drawRibbon(); else trail.classList.remove("on");
  }
  /** The swing's direction as the strike sees it: the hand's smoothed turn for a tool that turns, else the last move. */
  function strikeAngle() {
    return Object.prototype.hasOwnProperty.call(DIRECTIONAL, state.tool) ? wrap(handAngle - DIRECTIONAL[state.tool]) : swingAngle();
  }
  /** A smooth curve through points: Catmull-Rom turned into cubic Béziers. */
  function smoothPath(pts) {
    if (pts.length < 2) return "";
    var d = "M" + f(pts[0].x) + " " + f(pts[0].y);
    for (var i = 0; i + 1 < pts.length; i++) {
      var p0 = pts[i - 1] || pts[i], p1 = pts[i], p2 = pts[i + 1], p3 = pts[i + 2] || p2;
      d += "C" + f(p1.x + (p2.x - p0.x) / 6) + " " + f(p1.y + (p2.y - p0.y) / 6) + " " + f(p2.x - (p3.x - p1.x) / 6) + " " + f(p2.y - (p3.y - p1.y) / 6) + " " + f(p2.x) + " " + f(p2.y);
    }
    return d;
  }
  /** The lash: back from the grip along the pointer's history for LASH px; what the hand has not dragged yet lies in a loose curl past the end. */
  function ribbonPoints() {
    var pts = [pointer], len = 0, q, i;
    for (i = history.length - 1; i >= 0 && len < LASH; i--) {
      var p = history[i];
      q = pts[pts.length - 1];
      var d = Math.sqrt((p.x - q.x) * (p.x - q.x) + (p.y - q.y) * (p.y - q.y));
      if (d < 3) continue;
      if (len + d > LASH) { var k = (LASH - len) / d; pts.push({ x: q.x + (p.x - q.x) * k, y: q.y + (p.y - q.y) * k }); len = LASH; break; }
      pts.push(p); len += d;
    }
    q = pts[pts.length - 1];
    var dir = pts.length > 1 ? Math.atan2(q.y - pts[pts.length - 2].y, q.x - pts[pts.length - 2].x) : handTarget + Math.PI;
    while (len < LASH - 1) {
      var r0 = Math.min(LASH - len, 22);
      dir += 0.14;
      q = { x: q.x + Math.cos(dir) * r0, y: q.y + Math.sin(dir) * r0 };
      pts.push(q); len += r0;
    }
    return pts;
  }
  /** The curve through the points, sampled four times a span, for an outline to be built on. */
  function densify(pts) {
    var out = [];
    for (var i = 0; i + 1 < pts.length; i++) {
      var p0 = pts[i - 1] || pts[i], p1 = pts[i], p2 = pts[i + 1], p3 = pts[i + 2] || p2;
      var c1x = p1.x + (p2.x - p0.x) / 6, c1y = p1.y + (p2.y - p0.y) / 6, c2x = p2.x - (p3.x - p1.x) / 6, c2y = p2.y - (p3.y - p1.y) / 6;
      for (var k = 0; k < 4; k++) {
        var t = k / 4, mt = 1 - t;
        out.push({ x: mt * mt * mt * p1.x + 3 * mt * mt * t * c1x + 3 * mt * t * t * c2x + t * t * t * p2.x, y: mt * mt * mt * p1.y + 3 * mt * mt * t * c1y + 3 * mt * t * t * c2y + t * t * t * p2.y });
      }
    }
    out.push(pts[pts.length - 1]);
    return out;
  }
  /** A pointer's raw history has corners; leather has none. A three-point mean, twice, ends pinned. */
  function smoothPts(pts) {
    var out = pts;
    for (var pass = 0; pass < 2 && out.length > 2; pass++) {
      var next = [out[0]];
      for (var i = 1; i + 1 < out.length; i++) next.push({ x: (out[i - 1].x + out[i].x + out[i + 1].x) / 3, y: (out[i - 1].y + out[i].y + out[i + 1].y) / 3 });
      next.push(out[out.length - 1]);
      out = next;
    }
    return out;
  }
  /** A closed outline around the curve, its width falling from the grip to the tip. */
  function taperedOutline(pts, wGrip, wTip) {
    var n = pts.length, left = [], right = [], i;
    for (i = 0; i < n; i++) {
      var a = pts[Math.max(0, i - 1)], b = pts[Math.min(n - 1, i + 1)];
      var dx = b.x - a.x, dy = b.y - a.y, len = Math.sqrt(dx * dx + dy * dy) || 1;
      var nx = -dy / len, ny = dx / len, hw = (wGrip + (wTip - wGrip) * (i / (n - 1))) / 2;
      left.push({ x: pts[i].x + nx * hw, y: pts[i].y + ny * hw });
      right.push({ x: pts[i].x - nx * hw, y: pts[i].y - ny * hw });
    }
    var d = "M" + f(left[0].x) + " " + f(left[0].y);
    for (i = 1; i < n; i++) d += "L" + f(left[i].x) + " " + f(left[i].y);
    for (i = n - 1; i >= 0; i--) d += "L" + f(right[i].x) + " " + f(right[i].y);
    return d + "Z";
  }
  function makeRibbon() {
    ribbon = { under: el("path", { class: "lash-under" }, trail), over: el("path", { class: "lash-over" }, trail), hi: el("path", { class: "lash-hi", "stroke-width": 1 }, trail) };
  }
  /** One filled shape from grip to tip, a darker one beneath it, a highlight along its thick half. */
  function drawRibbon() {
    if (!ribbon) makeRibbon();
    var pts = densify(ribbonHeld && now < ribbonHeld.until ? ribbonHeld.pts : smoothPts(ribbonPoints()));
    ribbon.under.setAttribute("d", taperedOutline(pts, 6.4, 2.4));
    ribbon.over.setAttribute("d", taperedOutline(pts, 5, 1.2));
    ribbon.hi.setAttribute("d", smoothPath(pts.slice(0, Math.ceil(pts.length * 0.6))));
    trail.classList.add("on");
  }
  function drawBand(path) {
    var w = H.TOOLS.squeegee.shape.width;
    if (!band) band = { wet: el("path", { class: "wet-band", "stroke-width": w }, trail), edge: el("path", { class: "wet-edge", transform: "translate(0 " + (-w / 2 + 1).toFixed(1) + ")" }, trail), streaks: [el("path", { class: "wet-streak", transform: "translate(0 " + (-w / 4).toFixed(1) + ")" }, trail), el("path", { class: "wet-streak", transform: "translate(0 " + (w / 5).toFixed(1) + ")" }, trail)], bead: el("line", { class: "wet-bead" }, trail) };
    var d = smoothPath(path.length > 1 ? path : [path[0], path[0]]);
    band.wet.setAttribute("d", d);
    band.edge.setAttribute("d", d);
    band.streaks[0].setAttribute("d", d);
    band.streaks[1].setAttribute("d", d);
    // The bead of water rides just ahead of the blade: across the way the hand moves, as long as the blade.
    var p = path[path.length - 1], a = handAngle - DIRECTIONAL.squeegee, nx = -Math.sin(a), ny = Math.cos(a), ax = Math.cos(a) * 8, ay = Math.sin(a) * 8;
    band.bead.setAttribute("x1", f(p.x + ax + nx * w * 0.46)); band.bead.setAttribute("y1", f(p.y + ay + ny * w * 0.46));
    band.bead.setAttribute("x2", f(p.x + ax - nx * w * 0.46)); band.bead.setAttribute("y2", f(p.y + ay - ny * w * 0.46));
  }
  function clearBand() { if (band) { band.wet.remove(); band.edge.remove(); band.streaks[0].remove(); band.streaks[1].remove(); band.bead.remove(); band = null; } }

  // --- what a blow shows, for a blink ----------------------------------------------------------
  function fx(className, x, y, extra, ttl) {
    var node = document.createElement("div");
    node.className = className;
    var pose = "translate3d(" + f(x) + "px," + f(y) + "px,0)" + (extra && extra["--rot"] ? " rotate(" + extra["--rot"] + ")" : "");
    node.style.setProperty("--pose", pose);
    node.style.transform = pose;
    if (extra) for (var k in extra) node.style.setProperty(k, extra[k]);
    fxEl.appendChild(node);
    setTimeout(function () { node.remove(); }, ttl || 520);
    return node;
  }
  /** Droplets of the insect flung from the blow, most of them forward along the swing. */
  function fling(x, y, angle, color, count, power) {
    if (reduced) return;
    for (var i = 0; i < count; i++) {
      var a = angle + (r.next() - 0.5) * 2.4 + (r.next() < 0.2 ? Math.PI : 0);
      var v = (14 + r.next() * 30) * power, size = 2 + r.next() * 3;
      fx("fling", x, y, { "--dx": f(Math.cos(a) * v) + "px", "--dy": f(Math.sin(a) * v) + "px", "--s": size.toFixed(1) + "px", "--c": color, "--t": Math.round(260 + r.next() * 200) + "ms" }, 520);
    }
  }
  /** Glass dust: pale flakes thrown from a blow that found no insect. */
  function shards(x, y, count) {
    if (reduced) return;
    for (var i = 0; i < count; i++) {
      var a = r.next() * Math.PI * 2, v = 22 + r.next() * 34;
      fx("shard", x, y, { "--dx": f(Math.cos(a) * v) + "px", "--dy": f(Math.sin(a) * v) + "px", "--spin": Math.round((r.next() - 0.5) * 540) + "deg", "--rot": Math.round(r.next() * 360) + "deg" }, 480);
    }
  }
  /** Sparks where a whip's tip snapped. */
  function sparks(x, y, count) {
    if (reduced) return;
    for (var i = 0; i < count; i++) fx("spark", x, y, { "--a": Math.round(r.next() * 360) + "deg" }, 240);
  }
  /** The points a kill earned, rising from it, the streak beside them when it counts. */
  function floatScore(x, y, points, streak) {
    var node = fx("float", x, y, null, 900);
    var b = document.createElement("b");
    b.textContent = "+" + points;
    node.appendChild(b);
    if (streak > 1) {
      var i = document.createElement("i");
      i.textContent = "\u00d7" + Math.min(5, streak);
      node.appendChild(i);
    }
  }
  /** What the glass shows of the blow itself: a shock ring the tool's size, or a spray's cone. */
  function impactFx(tool, g) {
    var rot = ((g.angle || 0) * 57.2958).toFixed(1) + "deg";
    if (tool === "swatter") fx("ring", g.x, g.y, { "--w": "58px", "--h": "58px", "--r": "10px" }, 360);
    else if (tool === "slipper") fx("ring", g.x, g.y, { "--w": "54px", "--h": "54px", "--r": "50%" }, 360);
    else if (tool === "newspaper") fx("ring", g.x, g.y, { "--w": "124px", "--h": "44px", "--r": "22px", "--rot": rot }, 360);
    else if (tool === "spray") fx("cone", g.x, g.y, { "--a": (((g.angle || 0) * 57.2958) + 90).toFixed(1) + "deg" });
  }
  function syncMarks() {
    var keep = H.aliveMarks(state.marks, now);
    var seen = {};
    for (var i = 0; i < keep.length; i++) {
      var m = keep[i];
      seen[m.id] = true;
      drawMark(m);
      if (m.ttl !== null) markNodes[m.id].setAttribute("opacity", H.markOpacity(m, now).toFixed(2));
    }
    for (var id in markNodes) {
      if (!seen[id]) { markNodes[id].remove(); delete markNodes[id]; }
    }
    state.marks = keep;
  }
  function addMarks(marks) {
    for (var i = 0; i < marks.length; i++) state.marks.push(marks[i]);
    if (state.marks.length > 40) state.marks.splice(0, state.marks.length - 40);
  }

  // --- the loop ---------------------------------------------------------------------------
  function frame(ts) {
    raf = null;
    if (!state.visible) return;
    var t = ts / 1000;
    var dt = clock ? Math.min(0.05, t - clock) : 0.016;
    clock = t;
    now += dt;
    var scale = H.speedScale({ reduced: reduced });
    var env = { bounds: state.bounds, obstacles: walls(), pointer: state.mode === "hunt" ? pointer : null, rng: r, now: now, scale: scale, leaderOf: leaderOf };
    var next = [], nearest = Infinity;
    for (var i = 0; i < state.insects.length; i++) {
      var b = state.insects[i];
      if (b.state === "dead") {
        if (now - b.diedAt > 0.55) { dropBug(b); continue; }
        next.push(b);
        continue;
      }
      b = H.step(b, dt, env);
      if (b.state === "dead") { b.diedAt = now; b.by = "spray"; addMarks([H.splat(b.x, b.y, b.species, b.seed, now)]); }
      if (H.offstage(b, state.bounds)) { dropBug(b); continue; }
      if (b.species === "mosquito" && pointer && state.mode === "hunt") {
        var d = Math.sqrt((b.x - pointer.x) * (b.x - pointer.x) + (b.y - pointer.y) * (b.y - pointer.y));
        if (d < nearest) nearest = d;
      }
      next.push(b);
      drawBug(b);
    }
    state.insects = next;
    whine(nearest < 160, 1 - nearest / 160);
    updateHand();
    if (now >= state.nextSpawn && alive() < H.maxInsects({ difficulty: state.difficulty, reduced: reduced })) {
      arrive();
      state.nextSpawn = now + H.spawnInterval({ working: state.working, difficulty: state.difficulty, reduced: reduced, count: alive() });
    }
    syncMarks();
    raf = window.requestAnimationFrame(frame);
  }
  function startLoop() { if (!raf && state.visible) { clock = 0; raf = window.requestAnimationFrame(frame); } }
  function stopLoop() { if (raf) { window.cancelAnimationFrame(raf); raf = null; } }

  // --- the strikes ---------------------------------------------------------------------------
  /** How each tool swings: the hand's animation, how long until it lands (ms), how hard the glass shakes (px). */
  var SWING = {
    whip: { anim: "flick", impact: 0, shake: 1 },
    swatter: { anim: "slam", impact: 70, shake: 2 },
    slipper: { anim: "stomp", impact: 90, shake: 3 },
    newspaper: { anim: "whack", impact: 100, shake: 4 },
    spray: { anim: "puff", impact: 0, shake: 0 },
    squeegee: { anim: "press", impact: 0, shake: 0 },
  };
  function shake(px) {
    if (reduced || !px) return;
    document.body.style.setProperty("--shake", px + "px");
    document.body.classList.remove("shake");
    void document.body.offsetWidth;
    document.body.classList.add("shake");
  }
  function swingAngle() {
    if (!lastMove || !lastMove[0]) return -Math.PI / 2;
    var dx = lastMove[1].x - lastMove[0].x, dy = lastMove[1].y - lastMove[0].y;
    if (dx * dx + dy * dy < 4) return -Math.PI / 2;
    return Math.atan2(dy, dx);
  }
  function inside(p) { return { x: Math.max(0, Math.min(state.bounds.width, p.x)), y: Math.max(0, Math.min(state.bounds.height, p.y)) }; }
  /** The blow has landed: the insects it reached, the marks, the sounds, and what the glass shows of it. */
  function applyStrike(result, g) {
    if (!result) return;
    var hit = false;
    for (var i = 0; i < state.insects.length; i++) {
      var b = state.insects[i];
      if (result.hits.indexOf(b.id) === -1) continue;
      hit = true;
      if (result.slow) { b.slow = result.slow; b.sprayed = true; continue; }
      var sp = H.SPECIES[b.species];
      b.hp -= 1;
      b.by = state.tool;
      if (b.hp <= 0) {
        b.state = "dead"; b.diedAt = now;
        state.score = H.scoreHit(state.score, b, now);
        fling(b.x, b.y, g.angle, sp.splat, 9, 1);
        floatScore(b.x, b.y - sp.size / 2 - 36, state.score.lastPoints, state.score.streak);
      } else {
        // Struck and alive: knocked a step along the swing, a few droplets lost.
        b.state = "hit"; b.until = b.t + 0.2;
        var k = inside({ x: b.x + Math.cos(g.angle) * 10, y: b.y + Math.sin(g.angle) * 10 });
        b.x = k.x; b.y = k.y;
        fling(b.x, b.y, g.angle, sp.splat, 4, 0.6);
      }
      drawBug(b);
    }
    if (!hit && !result.wipes && !result.slow) state.score = H.scoreMiss(state.score);
    play(result.sound);
    if (hit && !result.slow) { for (var n = 0; n < result.hits.length; n++) play("splat", n * 0.03); }
    if (!hit && result.marks.length && result.marks[0].kind === "crack") { play("crack", 0.02); shards(g.x, g.y, 7); fx("flash", g.x, g.y, null, 300); }
    addMarks(result.marks);
    impactFx(state.tool, g);
    shake(SWING[state.tool].shake);
    renderHud();
    save();
  }
  function resolve(g) {
    if (state.mode !== "hunt") return;
    var result = H.strike(state.tool, g, state.insects, now);
    if (result) applyStrike(result, g);
  }
  /** A click tool: the hand comes down, and the blow lands when it touches the glass. */
  function swing(p) {
    var tool = H.TOOLS[state.tool];
    if (now < state.cooldownUntil) return;
    state.cooldownUntil = now + tool.cooldown;
    var g = { x: p.x, y: p.y, angle: strikeAngle(), bounds: state.bounds };
    handPlay(SWING[state.tool].anim);
    play("swish");
    var travel = reduced ? 0 : SWING[state.tool].impact;
    if (travel) setTimeout(function () { resolve(g); }, travel);
    else resolve(g);
  }
  /** The whip, on release: the lash flies out straight from the grip to where the stroke began, bowed a little, and snaps there; the blow lands at the snap. */
  function crack(s, p) {
    var tool = H.TOOLS.whip;
    var dx = p.x - s.from.x, dy = p.y - s.from.y, len = Math.sqrt(dx * dx + dy * dy);
    if (len < tool.minStroke || now < state.cooldownUntil) return;
    state.cooldownUntil = now + tool.cooldown;
    var g = { from: s.from, to: p, x: (s.from.x + p.x) / 2, y: (s.from.y + p.y) / 2, angle: Math.atan2(dy, dx), length: len, bounds: state.bounds };
    handPlay("flick");
    history = [];
    // The leather pulls straight along the stroke, bowed a little, and holds there a beat; a streak of light runs its last stretch to the tip.
    var nx = -dy / len, ny = dx / len, bow = Math.min(24, len * 0.12), cx = g.x + nx * bow, cy = g.y + ny * bow;
    var held = [], streak = "";
    for (var k = 0; k <= 16; k++) {
      var t = k / 16, mt = 1 - t;
      var q = { x: mt * mt * p.x + 2 * mt * t * cx + t * t * s.from.x, y: mt * mt * p.y + 2 * mt * t * cy + t * t * s.from.y };
      held.push(q);
      if (t >= 0.55) streak += (streak ? "L" : "M") + f(q.x) + " " + f(q.y);
    }
    ribbonHeld = { pts: held, until: now + 0.3 };
    var lash = el("path", { class: "lash-crack", d: streak, "stroke-dasharray": (len * 0.5).toFixed(0) }, trail);
    lash.style.setProperty("--len", (len * 0.5).toFixed(0));
    setTimeout(function () { lash.remove(); }, 400);
    var snap = function () {
      sparks(s.from.x, s.from.y, 6);
      fx("flash", s.from.x, s.from.y, { "--size": "70px" }, 240);
      resolve(g);
    };
    if (reduced) snap(); else setTimeout(snap, 90);
  }
  function onDown(e) {
    if (state.mode !== "hunt" || e.button !== 0) return;
    e.preventDefault();
    ensureAudio();
    var p = { x: e.clientX, y: e.clientY };
    pointer = p;
    var tool = H.TOOLS[state.tool];
    if (tool.minStroke) { stroke = { from: p, path: [p] }; return; }
    if (tool.wipes) { stroke = { from: p, path: [p] }; handPlay("press"); hand.classList.add("down"); play("squeegee"); drawBand(stroke.path); return; }
    swing(p);
  }
  function onMove(e) {
    var p = { x: e.clientX, y: e.clientY };
    var q = lastMove ? lastMove[1] : p;
    lastMove = [q, p];
    if (state.mode !== "hunt") return;
    pointer = p;
    var dx = p.x - q.x, dy = p.y - q.y;
    if (dx * dx + dy * dy >= 9) handTarget = Math.atan2(dy, dx);
    history.push(p);
    if (history.length > 80) history.splice(0, history.length - 80);
    if (stroke && H.TOOLS[state.tool].wipes) {
      var last = stroke.path[stroke.path.length - 1];
      stroke.path.push(p);
      state.marks = H.wipe(state.marks, [last, p], H.TOOLS.squeegee.shape.width);
      drawBand(stroke.path);
    } else if (stroke) {
      stroke.path.push(p);
    }
  }
  function onUp(e) {
    if (state.mode !== "hunt" || !stroke) return;
    var p = { x: e.clientX, y: e.clientY };
    var s = stroke;
    stroke = null;
    var tool = H.TOOLS[state.tool];
    if (tool.wipes) {
      s.path.push(p);
      hand.classList.remove("down");
      clearBand();
      addMarks([H.smear(s.path, tool.shape.width, Math.floor(p.x * 7 + p.y * 3) >>> 0, now)]);
      return;
    }
    crack(s, p);
  }
  glass.addEventListener("pointerdown", onDown);
  glass.addEventListener("pointermove", onMove);
  glass.addEventListener("pointerup", onUp);
  glass.addEventListener("pointercancel", function () { stroke = null; hand.classList.remove("down"); clearBand(); });
  glass.addEventListener("pointerenter", function (e) { if (state.mode === "hunt") pointer = { x: e.clientX, y: e.clientY }; });
  glass.addEventListener("pointerleave", function (e) { if (stroke) onUp(e); pointer = null; whine(false, 0); hideHand(); });
  glass.addEventListener("contextmenu", function (e) { if (state.mode === "hunt") e.preventDefault(); });

  // --- the strip ---------------------------------------------------------------------------------
  function askMode(mode) { bisa.scene.mode(mode).catch(function () {}); }
  function setTool(name) {
    if (!H.isTool(name)) return;
    state.tool = name;
    renderStrip();
    save();
  }
  toolBtns.forEach(function (b) {
    b.addEventListener("click", function () {
      setTool(b.dataset.tool);
      if (state.mode !== "hunt") askMode("hunt");
    });
  });
  modeBtn.addEventListener("click", function () { askMode(state.mode === "hunt" ? "work" : "hunt"); });
  diffBtn.addEventListener("click", function () {
    var names = H.DIFFICULTY_NAMES;
    state.difficulty = names[(names.indexOf(state.difficulty) + 1) % names.length];
    renderStrip();
    save();
  });
  muteBtn.addEventListener("click", function () { setMuted(!state.muted); });
  leaveBtn.addEventListener("click", function () { bisa.window.close().catch(function () {}); });
  // In work mode the strip is the one solid thing: when the pointer leaves it, the app has the pointer again.
  strip.addEventListener("pointerleave", function () { if (state.mode === "work") bisa.scene.release().catch(function () {}); });
  document.addEventListener("keydown", function (e) {
    if (e.key === "Escape" && state.mode === "hunt") { e.preventDefault(); askMode("work"); return; }
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    var n = parseInt(e.key, 10);
    if (n >= 1 && n <= H.TOOL_NAMES.length && state.mode === "hunt") setTool(H.TOOL_NAMES[n - 1]);
  });
  if (window.ResizeObserver) new ResizeObserver(function () { placeStrip(); }).observe(strip);

  // --- the host ------------------------------------------------------------------------------------
  function setMode(mode) {
    if (mode === state.mode) return;
    state.mode = mode;
    if (mode === "work") { stroke = null; pointer = null; history = []; whine(false, 0); clearBand(); hideHand(); }
    renderStrip();
  }
  function onScene(ev) {
    if (!ev) return;
    if (ev.viewport && ev.viewport.width > 0 && ev.viewport.height > 0) state.bounds = { width: ev.viewport.width, height: ev.viewport.height };
    state.obstacles = Array.isArray(ev.obstacles) ? ev.obstacles : [];
    state.cuts = !!ev.cuts;
    if (ev.mode === "work" || ev.mode === "hunt") setMode(ev.mode);
    placeStrip();
  }
  function setWorking(n) {
    state.working = typeof n === "number" && n > 0 ? Math.floor(n) : 0;
    renderHud();
  }

  scheme(window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");
  renderHud();
  renderStrip();
  bisa.ready().then(function (hello) {
    scheme(hello.theme.scheme);
    canStore = bisa.granted("storage");
    if (canStore) {
      bisa.storage.get("state").then(function (res) {
        var saved = H.readSaved(res.value);
        state.score.best = saved.best;
        state.muted = saved.muted;
        state.tool = saved.tool;
        state.difficulty = saved.difficulty;
        renderHud();
        renderStrip();
      }).catch(function () {});
    }
    if (bisa.granted("workspace_summary")) {
      bisa.workspace.summary().then(function (s) { setWorking(s && s.working); }).catch(function () {});
    }
    placeStrip();
    startLoop();
  });
  bisa.on("theme", function (p) { scheme(p.scheme); });
  bisa.on("visibility", function (p) {
    state.visible = !!p.visible;
    if (state.visible) startLoop(); else { stopLoop(); whine(false, 0); }
  });
  bisa.on("workspace.summary", function (s) { setWorking(s && s.working); });
  bisa.on("scene", onScene);
  window.addEventListener("resize", function () {
    if (!state.cuts && state.obstacles.length === 0) state.bounds = { width: window.innerWidth, height: window.innerHeight };
    placeStrip();
  });
})();
