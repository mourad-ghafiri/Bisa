// Linefeed: a terminal worm that is the code it eats. The rules are
// `logic.js`'s (`Linefeed`); this file is the screen, the keys, the clock,
// the bell and the bridge. Between the kit markers lies the terminal kit the
// three terminal games share, word for word — a test holds the copies equal.
(function () {
  "use strict";
  var L = window.Linefeed;
  function $(id) { return document.getElementById(id); }

  // --- kit ---
  /** A string into its glyphs, a variation selector staying with the glyph before it. */
  function glyphs(str) {
    var out = [], chars = Array.from(String(str));
    for (var i = 0; i < chars.length; i++) {
      var c = chars[i];
      if ((c === "︎" || c === "️") && out.length) out[out.length - 1] += c; else out.push(c);
    }
    return out;
  }
  /** A screen of character cells: write anywhere, then flush only what changed. */
  function Screen(el, cols, rows) {
    var spans = [], shown = [], next = [], x, y;
    el.textContent = "";
    for (y = 0; y < rows; y++) {
      var row = document.createElement("div");
      row.className = "row";
      spans.push([]); shown.push([]); next.push([]);
      for (x = 0; x < cols; x++) {
        var s = document.createElement("span");
        s.className = "c";
        s.textContent = " ";
        row.appendChild(s);
        spans[y].push(s); shown[y].push(" |"); next[y].push(" |");
      }
      el.appendChild(row);
    }
    function put(cx, cy, ch, cls) {
      if (cx < 0 || cx >= cols || cy < 0 || cy >= rows) return;
      next[cy][cx] = ch + "|" + (cls || "");
    }
    function text(cx, cy, str, cls) {
      var gs = glyphs(str);
      for (var i = 0; i < gs.length; i++) put(cx + i, cy, gs[i], cls);
    }
    function clear() { for (y = 0; y < rows; y++) for (x = 0; x < cols; x++) next[y][x] = " |"; }
    function flush() {
      for (y = 0; y < rows; y++) for (x = 0; x < cols; x++) {
        var v = next[y][x];
        if (v === shown[y][x]) continue;
        var cut = v.indexOf("|"), cls = v.slice(cut + 1);
        spans[y][x].textContent = v.slice(0, cut);
        spans[y][x].className = cls ? "c " + cls : "c";
        shown[y][x] = v;
      }
    }
    return { put: put, text: text, clear: clear, flush: flush, cols: cols, rows: rows };
  }
  /** Keys to action names: the map's keys only, never with a modifier, the browser kept from them. */
  function Keys(map, onAction) {
    function onKey(e) {
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      if (!Object.prototype.hasOwnProperty.call(map, e.key)) return;
      e.preventDefault();
      onAction(map[e.key], e);
    }
    document.addEventListener("keydown", onKey);
    return { off: function () { document.removeEventListener("keydown", onKey); } };
  }
  /** A frame loop that hands every frame its measured span in milliseconds. */
  function Clock(onFrame) {
    var raf = null, last = 0, running = false;
    function frame(ts) {
      raf = null;
      if (!running) return;
      var dt = last ? ts - last : 16;
      last = ts;
      onFrame(dt);
      if (running) raf = window.requestAnimationFrame(frame);
    }
    return {
      start: function () { if (running) return; running = true; last = 0; raf = window.requestAnimationFrame(frame); },
      stop: function () { running = false; if (raf) window.cancelAnimationFrame(raf); raf = null; },
      running: function () { return running; },
    };
  }
  /** Square-wave blips, the terminal's bell, behind a mute. */
  function Bell() {
    var ctx = null, muted = false;
    var NOTES = { tick: [220, 0.04, 0.06], move: [330, 0.02, 0.035], score: [660, 0.07, 0.1], bonus: [880, 0.1, 0.14], over: [110, 0.35, 0.16] };
    function wake() {
      if (ctx) return ctx;
      var C = window.AudioContext || window.webkitAudioContext;
      if (!C) return null;
      ctx = new C();
      return ctx;
    }
    function blip(kind) {
      if (muted) return;
      var c = wake();
      if (!c) return;
      if (c.state === "suspended") c.resume().catch(function () {});
      var n = NOTES[kind] || NOTES.tick, t0 = c.currentTime, o = c.createOscillator(), g = c.createGain();
      o.type = "square"; o.frequency.value = n[0];
      g.gain.setValueAtTime(0.0001, t0);
      g.gain.exponentialRampToValueAtTime(n[2], t0 + 0.005);
      g.gain.exponentialRampToValueAtTime(0.0001, t0 + n[1]);
      o.connect(g); g.connect(c.destination);
      o.start(t0); o.stop(t0 + n[1] + 0.02);
    }
    return { blip: blip, wake: wake, mute: function (m) { muted = !!m; }, muted: function () { return muted; } };
  }
  /** The prompt types its command at open, then the cursor blinks at its end; later words appear at once. */
  function Prompt(el, command, reduced) {
    el.textContent = "";
    var chev = document.createElement("span"), text = document.createElement("span"), cursor = document.createElement("span");
    chev.className = "chev"; chev.textContent = "❯ ";
    cursor.className = "cursor";
    el.appendChild(chev); el.appendChild(text); el.appendChild(cursor);
    var typed = 0, typing = !reduced;
    function type() {
      if (!typing) return;
      if (typed >= command.length) { typing = false; return; }
      text.textContent = command.slice(0, ++typed);
      setTimeout(type, 30);
    }
    if (typing) type(); else text.textContent = command;
    return { set: function (s) { typing = false; text.textContent = s; } };
  }
  /** The largest type at which the screen's columns and rows, and the rows around it, fit the window. */
  function Fit(term, cols, rows, extraRows) {
    function fit() {
      var w = window.innerWidth - 18, h = window.innerHeight - 18;
      var size = Math.floor(Math.min(w / (cols * 0.61), h / ((rows + extraRows) * 1.25)));
      term.style.fontSize = Math.max(11, Math.min(48, size)) + "px";
    }
    window.addEventListener("resize", fit);
    fit();
    return { fit: fit };
  }
  /** A line of segments joined by dots, the least wanted dropped until it fits the columns; never a word cut. */
  function fitLine(segments, cols) {
    var kept = segments.filter(function (s) { return s && s.text; });
    function joined() { return kept.map(function (s) { return s.text; }).join(" \u00b7 "); }
    while (kept.length > 1 && glyphs(joined()).length > cols) {
      var worst = 0;
      for (var i = 1; i < kept.length; i++) if (kept[i].rank >= kept[worst].rank) worst = i;
      kept.splice(worst, 1);
    }
    return joined();
  }
  /** Words wrapped to a width, whole. */
  function wrapWords(text, width) {
    var words = String(text).split(" "), lines = [], line = "";
    words.forEach(function (w) {
      if (line && glyphs(line + " " + w).length > width) { lines.push(line); line = w; } else line = line ? line + " " + w : w;
    });
    if (line) lines.push(line);
    return lines;
  }
  function pad(n, width) { var s = String(n); while (s.length < width) s = " " + s; return s; }
  /** Words for a screen reader, said once each: a change of state, never the running numbers. */
  function Live(el) {
    var last = "";
    return { say: function (text) { if (text === last) return; last = text; el.textContent = text; } };
  }
  /** The workspace's line: what runs, and what waits on you in the accent; nothing when nothing does. */
  function WaitRow(el, live) {
    var lastWaiting = 0;
    function set(summary) {
      var working = summary && summary.working > 0 ? Math.floor(summary.working) : 0;
      var waiting = summary && summary.waiting > 0 ? Math.floor(summary.waiting) : 0;
      if (live && waiting > lastWaiting) live.say(waiting + (waiting === 1 ? " needs you" : " need you"));
      lastWaiting = waiting;
      el.textContent = "";
      if (!working && !waiting) return;
      var b = document.createElement("b");
      b.textContent = String(working);
      el.appendChild(b);
      el.appendChild(document.createTextNode(" running"));
      if (!waiting) return;
      el.appendChild(document.createTextNode(" · "));
      var n = document.createElement("span");
      n.className = "needs";
      n.textContent = waiting + (waiting === 1 ? " needs you" : " need you");
      el.appendChild(n);
    }
    return { set: set };
  }
  /** The addon's store: one key, read through the rules' reader, written on a beat. */
  function Store(read) {
    var granted = false, timer = null;
    return {
      open: function () { granted = bisa.granted("storage"); },
      load: function (cb) {
        if (!granted) { cb(read(null)); return; }
        bisa.storage.get("state").then(function (r) { cb(read(r.value)); }).catch(function () { cb(read(null)); });
      },
      save: function (value) {
        if (!granted) return;
        if (timer) clearTimeout(timer);
        timer = setTimeout(function () { timer = null; bisa.storage.set("state", JSON.stringify(value)).catch(function () {}); }, 300);
      },
    };
  }
  /** The keys overlay. */
  function Help(el, closeBtn) {
    function hide() { el.hidden = true; }
    closeBtn.addEventListener("click", hide);
    return { hide: hide, toggle: function () { el.hidden = !el.hidden; }, shown: function () { return !el.hidden; } };
  }
  /** A seed for a new game: the clock and the dice, never zero. */
  function newSeed() { return ((Date.now() ^ Math.floor(Math.random() * 4294967296)) >>> 0) || 1; }
  function reducedMotion() { return window.matchMedia("(prefers-reduced-motion: reduce)").matches; }
  function scheme(s) { document.documentElement.dataset.scheme = s; }
  function clip(str, n) { return str.length > n ? str.slice(0, n - 1) + "…" : str; }
  // --- end kit ---

  // --- the game -----------------------------------------------------------------------
  var GUTTER = 4, COLS = GUTTER + 2 * L.COLS, ROWS = L.ROWS;
  var KEYS = {
    ArrowUp: "up", k: "up", w: "up", ArrowDown: "down", j: "down", s: "down",
    ArrowLeft: "left", h: "left", a: "left", ArrowRight: "right", l: "right", d: "right",
    p: "pause", " ": "pause", Escape: "escape", r: "restart", m: "mute", "?": "help", Enter: "start",
  };
  var reduced = reducedMotion();
  var screen = Screen($("screen"), COLS, ROWS), statusEl = $("status");
  var prompt = Prompt($("prompt"), "bisa play linefeed", reduced);
  var live = Live($("live")), bell = Bell(), store = Store(L.readSaved), wait = WaitRow($("wait"), live), help = Help($("help"), $("helpClose"));
  Fit($("term"), COLS, ROWS, 3);
  var saved = L.readSaved(null), game = null, phase = "prompt", visible = true, restAfterHide = false, note = null;

  function toast(text) { note = { text: text, until: Date.now() + 1500 }; renderStatus(); }
  /** The line the worm has written so far, oldest first, its start elided when long. */
  function written(width) {
    var code = game.body.slice(1).reverse().map(function (s) { return s.ch; }).join("");
    var gs = glyphs(code);
    return gs.length > width ? "\u2026" + gs.slice(gs.length - width + 1).join("") : code;
  }
  function renderStatus() {
    var segs;
    if (phase === "prompt") segs = [{ text: "click the grid, then any key to feed", rank: 0 }, { text: "? keys", rank: 3 }];
    else if (!game.alive) segs = [{ text: L.words(game), rank: 0 }, { text: "score " + game.score, rank: 0 }, { text: "best " + saved.best, rank: 2 }, { text: written(18), rank: 1 }, { text: "r again", rank: 3 }];
    else if (game.paused) segs = [{ text: "paused", rank: 0 }, { text: "p to resume", rank: 2 }];
    else segs = [{ text: "score " + game.score, rank: 0 }, { text: "best " + saved.best, rank: 2 }, { text: "len " + game.body.length, rank: 1 }, { text: written(18), rank: 1 }, { text: L.words(game), rank: 0 }];
    if (note && Date.now() < note.until) segs.push({ text: note.text, rank: 1 });
    statusEl.textContent = fitLine(segs, COLS);
  }
  function render() {
    screen.clear();
    if (game) {
      var rows = L.view(game);
      for (var y = 0; y < L.ROWS; y++) {
        screen.text(0, y, pad(y + 1, 2) + " ", y === game.body[0].y ? "value" : "gutter");
        screen.put(3, y, "\u2502", "frame");
        for (var x = 0; x < L.COLS; x++) {
          var c = rows[y][x];
          if (c.ch === " ") continue;
          if (c.cls.indexOf("head") === 0) screen.text(GUTTER + 2 * x, y, "\u2588\u2588", c.cls);
          else screen.put(GUTTER + 2 * x, y, c.ch, c.cls);
        }
      }
    } else {
      for (var r = 0; r < L.ROWS; r++) { screen.text(0, r, pad(r + 1, 2) + " ", "gutter"); screen.put(3, r, "\u2502", "frame"); }
      screen.text(GUTTER + 6, 7, "a worm that is the code it eats", "cmt");
      screen.text(GUTTER + 6, 8, "click, then any key", "cmt");
    }
    screen.flush();
    renderStatus();
  }
  function keepBest() {
    if (game.best > saved.best) { saved.best = game.best; store.save(saved); }
  }
  function setGame(next) {
    if (next === game) return;
    var was = game;
    game = next;
    if (was && was.alive) {
      if (!game.alive) {
        bell.blip(game.cause === "full" ? "bonus" : "over");
        phase = "over";
        live.say(L.words(game) + ", score " + game.score);
        keepBest();
        store.save(saved);
      } else if (game.score > was.score) {
        bell.blip(game.bonus && game.bonus !== was.bonus ? "bonus" : "score");
        keepBest();
      }
    }
    render();
  }
  var clock = Clock(function (dt) {
    if (phase !== "play" || !visible) return;
    setGame(L.tick(game, dt));
  });
  function start(seed) {
    game = L.newGame(seed, { best: saved.best });
    phase = "play";
    prompt.set("bisa play linefeed --seed " + seed);
    live.say("a new line");
    render();
    clock.start();
  }
  function setMuted(m) {
    saved.muted = m;
    bell.mute(m);
    store.save(saved);
    toast(m ? "muted" : "sound on");
  }
  function onAction(name) {
    if (name === "help") { help.toggle(); return; }
    if (name === "escape") {
      if (help.shown()) help.hide();
      else if (phase === "play") setGame(L.apply(game, game.paused ? "resume" : "pause"));
      return;
    }
    if (help.shown()) return;
    if (name === "mute") { setMuted(!saved.muted); return; }
    if (name === "restart") { start(newSeed()); return; }
    if (phase === "prompt") { start(newSeed()); return; }
    if (phase === "over") { if (name === "start") start(newSeed()); return; }
    if (name === "start") return;
    if (name === "pause") { setGame(L.apply(game, game.paused ? "resume" : "pause")); live.say(game.paused ? "paused" : "resumed"); return; }
    setGame(L.apply(game, name));
  }
  Keys(KEYS, onAction);
  $("screen").addEventListener("pointerdown", function () {
    $("screen").focus();
    bell.wake();
    if (phase === "prompt") start(newSeed());
  });
  window.addEventListener("blur", function () { if (phase === "play" && !game.paused) setGame(L.apply(game, "pause")); });

  // --- the host ---------------------------------------------------------------------------
  scheme(window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");
  render();
  bisa.ready().then(function (hello) {
    scheme(hello.theme.scheme);
    store.open();
    store.load(function (s) { saved = s; bell.mute(saved.muted); renderStatus(); });
    if (bisa.granted("workspace_summary")) bisa.workspace.summary().then(wait.set).catch(function () {});
  });
  bisa.on("theme", function (p) { scheme(p.scheme); });
  bisa.on("workspace.summary", wait.set);
  bisa.on("visibility", function (p) {
    visible = !!p.visible;
    if (!visible) {
      restAfterHide = phase === "play" && !game.paused;
      if (restAfterHide) setGame(L.apply(game, "pause"));
      clock.stop();
    } else {
      if (restAfterHide && phase === "play") setGame(L.apply(game, "resume"));
      restAfterHide = false;
      if (phase === "play") clock.start();
    }
  });
})();
