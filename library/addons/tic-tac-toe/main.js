// You against the machine — easy or unbeatable — or two players at one
// board; who starts alternates, the tally is kept per mode in the addon's
// own store. The rules live in `logic.js`.
(function () {
  "use strict";
  var T = window.TicTacToe;
  var BEAT_MS = 350;
  var board = document.getElementById("board"), status = document.getElementById("status"), tallyEl = document.getElementById("tally"), again = document.getElementById("again");
  var modeSet = document.getElementById("mode"), levelSet = document.getElementById("level");
  var settings = { mode: "machine", level: "unbeatable", starter: "X" };
  var tally = { human: { X: 0, O: 0, draws: 0 }, machine: { you: 0, machine: 0, draws: 0 } };
  var cells = [], turn = "X", over = false, pending = null;
  var buttons = [];
  for (var i = 0; i < 9; i++) {
    var b = document.createElement("button");
    b.type = "button"; b.setAttribute("role", "gridcell"); b.setAttribute("aria-label", "Square " + (i + 1)); b.dataset.i = i;
    b.addEventListener("click", onCell);
    board.appendChild(b); buttons.push(b);
  }
  function scheme(s) { document.documentElement.dataset.scheme = s; }
  function vsMachine() { return settings.mode === "machine"; }
  function machineMark() { return "O"; }
  function machinesTurn() { return vsMachine() && !over && turn === machineMark(); }

  function words() {
    if (over) return status.textContent;
    if (vsMachine()) return machinesTurn() ? "The machine's move…" : "Your move";
    return turn + " to play";
  }
  function render() {
    buttons.forEach(function (b, i) {
      b.textContent = cells[i] || "";
      b.classList.toggle("o", cells[i] === "O");
      b.disabled = over || !!cells[i] || machinesTurn();
    });
    board.classList.toggle("thinking", machinesTurn());
    status.textContent = words();
    var t = vsMachine() ? tally.machine : tally.human;
    tallyEl.textContent = vsMachine()
      ? "you " + t.you + " · machine " + t.machine + " · draws " + t.draws
      : "X " + t.X + " · O " + t.O + " · draws " + t.draws;
    levelSet.hidden = !vsMachine();
  }
  function keep() {
    if (!bisa.granted("storage")) return;
    bisa.storage.set("tally", JSON.stringify(tally)).catch(function () {});
    bisa.storage.set("settings", JSON.stringify(settings)).catch(function () {});
  }
  function finish(w) {
    over = true;
    status.classList.add("won");
    if (w) {
      if (vsMachine()) { var you = w.who !== machineMark(); status.textContent = you ? "You win" : "The machine wins"; tally.machine[you ? "you" : "machine"] += 1; }
      else { status.textContent = w.who + " wins"; tally.human[w.who] += 1; }
    } else {
      status.textContent = "A draw";
      (vsMachine() ? tally.machine : tally.human).draws += 1;
    }
    render();
    if (w) w.line.forEach(function (j) { buttons[j].classList.add("win"); });
    keep();
  }
  function mark(i) {
    if (over || cells[i]) return;
    cells[i] = turn;
    var w = T.winner(cells);
    if (w) { finish(w); return; }
    if (T.full(cells)) { finish(null); return; }
    turn = T.other(turn);
    render();
    if (machinesTurn()) think();
  }
  // The machine answers after a beat — never on a finished board, never
  // twice, and never after the game it was thinking about ended.
  function think() {
    if (pending) clearTimeout(pending);
    pending = setTimeout(function () {
      pending = null;
      if (!machinesTurn()) return;
      var move = T.bestMove(cells, machineMark(), settings.level);
      if (move >= 0) mark(move);
    }, BEAT_MS);
  }
  function onCell(e) {
    if (machinesTurn()) return;
    mark(Number(e.currentTarget.dataset.i));
  }
  function reset(alternate) {
    if (pending) clearTimeout(pending);
    pending = null;
    cells = []; over = false;
    status.classList.remove("won");
    buttons.forEach(function (b) { b.classList.remove("win"); });
    if (vsMachine()) {
      if (alternate) settings.starter = T.other(settings.starter);
      turn = settings.starter;
    } else turn = "X";
    render();
    if (machinesTurn()) think();
  }
  again.addEventListener("click", function () { reset(true); keep(); });
  modeSet.addEventListener("change", function () {
    var chosen = modeSet.querySelector("input:checked");
    settings.mode = chosen && chosen.value === "human" ? "human" : "machine";
    settings.starter = "X";
    reset(false);
    keep();
  });
  levelSet.addEventListener("change", function () {
    var chosen = levelSet.querySelector("input:checked");
    settings.level = chosen && chosen.value === "easy" ? "easy" : "unbeatable";
    keep();
  });
  function applyForm() {
    modeSet.querySelectorAll("input").forEach(function (r) { r.checked = r.value === settings.mode; });
    levelSet.querySelectorAll("input").forEach(function (r) { r.checked = r.value === settings.level; });
  }
  function readTally(raw) {
    try {
      var t = JSON.parse(raw);
      if (t && t.human && t.machine) {
        ["X", "O", "draws"].forEach(function (k) { if (typeof t.human[k] === "number") tally.human[k] = t.human[k]; });
        ["you", "machine", "draws"].forEach(function (k) { if (typeof t.machine[k] === "number") tally.machine[k] = t.machine[k]; });
      }
    } catch (_e) { /* fresh */ }
  }
  function readSettings(raw) {
    try {
      var s = JSON.parse(raw);
      if (s && typeof s === "object") {
        if (s.mode === "human" || s.mode === "machine") settings.mode = s.mode;
        if (s.level === "easy" || s.level === "unbeatable") settings.level = s.level;
        if (s.starter === "X" || s.starter === "O") settings.starter = s.starter;
      }
    } catch (_e) { /* defaults */ }
  }
  bisa.ready().then(function (hello) {
    scheme(hello.theme.scheme);
    bisa.on("theme", function (t) { scheme(t.scheme); });
    if (!bisa.granted("storage")) { applyForm(); reset(false); return; }
    Promise.all([bisa.storage.get("tally"), bisa.storage.get("settings")]).then(function (r) {
      if (r[0] && r[0].value) readTally(r[0].value);
      if (r[1] && r[1].value) readSettings(r[1].value);
    }).catch(function () {}).then(function () { applyForm(); reset(false); });
  });
})();
