// Focus, then a break, then focus again — a ring that empties as the phase
// runs, a notice at each turn where the person allowed one; the lengths in
// a popover, kept in the addon's own store.
(function () {
  "use strict";
  var app = document.getElementById("app");
  var phaseEl = document.getElementById("phase"), countEl = document.getElementById("count"), timeEl = document.getElementById("time"), arc = document.getElementById("arc");
  var toggle = document.getElementById("toggle"), skip = document.getElementById("skip"), reset = document.getElementById("reset");
  var gear = document.getElementById("gear"), form = document.getElementById("lengths");
  var focusIn = document.getElementById("focus"), restIn = document.getElementById("rest");
  var RING = 2 * Math.PI * 46;
  var lengths = { focus: 25, rest: 5 };
  var state = { phase: "focus", left: 25 * 60, running: false, done: 0, endsAt: 0 };
  var timer = null;

  function scheme(s) { document.documentElement.dataset.scheme = s; }
  function two(n) { return (n < 10 ? "0" : "") + n; }
  function whole() { return (state.phase === "focus" ? lengths.focus : lengths.rest) * 60; }
  function render() {
    app.dataset.phase = state.phase;
    phaseEl.textContent = state.phase === "focus" ? "Focus" : "Break";
    countEl.textContent = state.done > 0 ? state.done + " done" : "";
    timeEl.textContent = two(Math.floor(state.left / 60)) + ":" + two(state.left % 60);
    toggle.textContent = state.running ? "Pause" : "Start";
    // The ring: full at the start of a phase, gone at its end.
    var fraction = Math.max(0, Math.min(1, state.left / whole()));
    arc.style.strokeDashoffset = (RING * (1 - fraction)).toFixed(2);
  }
  function turn() {
    if (state.phase === "focus") { state.done += 1; state.phase = "rest"; state.left = lengths.rest * 60; }
    else { state.phase = "focus"; state.left = lengths.focus * 60; }
    if (bisa.granted("notify")) {
      bisa.notify.show({ title: state.phase === "focus" ? "Back to it" : "Take a break", body: state.phase === "focus" ? lengths.focus + " minutes of focus" : lengths.rest + " minutes off" }).catch(function () {});
    }
  }
  function tick() {
    if (!state.running) return;
    state.left = Math.max(0, Math.round((state.endsAt - Date.now()) / 1000));
    if (state.left === 0) { turn(); state.endsAt = Date.now() + state.left * 1000; }
    render();
  }
  function run(on) {
    state.running = on;
    if (timer) clearInterval(timer);
    timer = null;
    if (on) { state.endsAt = Date.now() + state.left * 1000; timer = setInterval(tick, 500); }
    render();
  }
  toggle.addEventListener("click", function () { run(!state.running); });
  skip.addEventListener("click", function () { turn(); state.endsAt = Date.now() + state.left * 1000; render(); });
  reset.addEventListener("click", function () { run(false); state = { phase: "focus", left: lengths.focus * 60, running: false, done: 0, endsAt: 0 }; render(); });

  // The lengths: a popover the gear opens; a click outside or Esc closes it.
  function openForm(open) {
    form.hidden = !open;
    gear.setAttribute("aria-expanded", open ? "true" : "false");
    if (open) focusIn.focus();
  }
  gear.addEventListener("click", function () { openForm(form.hidden); });
  document.addEventListener("pointerdown", function (e) { if (!form.hidden && !form.contains(e.target) && e.target !== gear) openForm(false); });
  document.addEventListener("keydown", function (e) { if (e.key === "Escape" && !form.hidden) { e.preventDefault(); openForm(false); gear.focus(); } });
  form.addEventListener("submit", function (e) { e.preventDefault(); openForm(false); });
  function readLengths() {
    var f = Math.min(120, Math.max(1, parseInt(focusIn.value, 10) || 25));
    var r = Math.min(60, Math.max(1, parseInt(restIn.value, 10) || 5));
    lengths = { focus: f, rest: r };
    if (!state.running) { state.left = (state.phase === "focus" ? f : r) * 60; }
    render();
    if (bisa.granted("storage")) bisa.storage.set("lengths", JSON.stringify(lengths)).catch(function () {});
  }
  focusIn.addEventListener("change", readLengths);
  restIn.addEventListener("change", readLengths);

  bisa.ready().then(function (hello) {
    scheme(hello.theme.scheme);
    bisa.on("theme", function (t) { scheme(t.scheme); });
    var start = bisa.granted("storage") ? bisa.storage.get("lengths") : Promise.resolve({ value: null });
    start.then(function (r) {
      if (r && r.value) { try { lengths = Object.assign(lengths, JSON.parse(r.value)); } catch (_e) { /* defaults */ } }
    }).catch(function () {}).then(function () {
      focusIn.value = lengths.focus; restIn.value = lengths.rest;
      state.left = lengths.focus * 60;
      render();
    });
  });
})();
