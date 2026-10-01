// A clock — one face at a time, analog or digital; the settings a popover
// the gear opens; every choice kept in the addon's own store.
(function () {
  "use strict";
  var app = document.getElementById("app");
  var time = document.getElementById("time");
  var meridiem = document.getElementById("meridiem");
  var date = document.getElementById("date");
  var dialDate = document.getElementById("dialDate");
  var reading = document.getElementById("reading");
  var gear = document.getElementById("gear");
  var settings = document.getElementById("settings");
  var hour = document.getElementById("hour");
  var minute = document.getElementById("minute");
  var second = document.getElementById("second");
  var SVG = "http://www.w3.org/2000/svg";
  var DEFAULTS = { style: "analog", h24: false, seconds: true, date: true };
  var prefs = Object.assign({}, DEFAULTS);
  var reduced = window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  var visible = true;
  var timer = null;
  var frame = null;
  var lastSecond = -1;

  // The dial: sixty ticks, twelve numerals.
  var ticks = document.getElementById("ticks");
  var numerals = document.getElementById("numerals");
  for (var i = 0; i < 60; i++) {
    var big = i % 5 === 0;
    var t = document.createElementNS(SVG, "line");
    var a = (i / 60) * Math.PI * 2;
    var r1 = big ? 42.5 : 44;
    t.setAttribute("x1", (50 + Math.sin(a) * r1).toFixed(2));
    t.setAttribute("y1", (50 - Math.cos(a) * r1).toFixed(2));
    t.setAttribute("x2", (50 + Math.sin(a) * 46).toFixed(2));
    t.setAttribute("y2", (50 - Math.cos(a) * 46).toFixed(2));
    t.setAttribute("class", "tick" + (big ? " big" : ""));
    ticks.appendChild(t);
  }
  for (var n = 1; n <= 12; n++) {
    var text = document.createElementNS(SVG, "text");
    var angle = (n / 12) * Math.PI * 2;
    text.setAttribute("x", (50 + Math.sin(angle) * 35.5).toFixed(2));
    text.setAttribute("y", (50 - Math.cos(angle) * 35.5 + 2.7).toFixed(2));
    text.setAttribute("text-anchor", "middle");
    text.setAttribute("class", "numeral");
    text.textContent = String(n);
    numerals.appendChild(text);
  }

  function scheme(s) { document.documentElement.dataset.scheme = s; }
  function two(n) { return (n < 10 ? "0" : "") + n; }
  function dateWords(now) { return now.toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" }); }
  function smooth() { return prefs.style === "analog" && prefs.seconds && !reduced; }

  function renderAnalog(now) {
    var h = now.getHours(), m = now.getMinutes(), s = now.getSeconds() + (smooth() ? now.getMilliseconds() / 1000 : 0);
    hour.style.transform = "rotate(" + ((h % 12) * 30 + m * 0.5 + s / 120) + "deg)";
    minute.style.transform = "rotate(" + (m * 6 + s * 0.1) + "deg)";
    second.style.transform = "rotate(" + s * 6 + "deg)";
    second.setAttribute("visibility", prefs.seconds ? "visible" : "hidden");
    dialDate.textContent = prefs.date ? dateWords(now) : "";
  }
  function renderDigital(now) {
    var h = now.getHours(), m = now.getMinutes(), s = now.getSeconds();
    var hh = prefs.h24 ? two(h) : String(h % 12 === 0 ? 12 : h % 12);
    time.textContent = hh + ":" + two(m) + (prefs.seconds ? ":" + two(s) : "");
    meridiem.textContent = prefs.h24 ? "" : h < 12 ? "am" : "pm";
    date.textContent = prefs.date ? dateWords(now) : "";
    date.hidden = !prefs.date;
  }
  function render() {
    var now = new Date();
    app.dataset.style = prefs.style;
    if (prefs.style === "analog") renderAnalog(now); else renderDigital(now);
    if (now.getSeconds() !== lastSecond) {
      lastSecond = now.getSeconds();
      reading.textContent = now.toLocaleTimeString(undefined, { hour12: !prefs.h24, hour: "2-digit", minute: "2-digit" });
    }
  }
  function stop() {
    if (timer) clearTimeout(timer);
    timer = null;
    if (frame) cancelAnimationFrame(frame);
    frame = null;
  }
  // A sweeping second hand draws every frame; otherwise the clock wakes at
  // the next second, or the next minute when seconds do not show. Nothing
  // runs while the window is not seen.
  function loop() {
    frame = null;
    if (!visible) return;
    render();
    if (smooth()) { frame = requestAnimationFrame(loop); return; }
    var period = prefs.seconds ? 1000 - (Date.now() % 1000) : 60000 - (Date.now() % 60000);
    timer = setTimeout(loop, period);
  }
  function schedule() { stop(); loop(); }

  // The settings: a popover; a click outside or Esc closes it.
  function applyForm() {
    settings.querySelectorAll("input[name=style]").forEach(function (r) { r.checked = r.value === prefs.style; });
    settings.querySelector("input[name=h24]").checked = prefs.h24;
    settings.querySelector("input[name=seconds]").checked = prefs.seconds;
    settings.querySelector("input[name=date]").checked = prefs.date;
  }
  function openSettings(open) {
    settings.hidden = !open;
    gear.setAttribute("aria-expanded", open ? "true" : "false");
    if (open) { var first = settings.querySelector("input:checked") || settings.querySelector("input"); if (first) first.focus(); }
  }
  settings.addEventListener("change", function () {
    var style = settings.querySelector("input[name=style]:checked");
    prefs = {
      style: style && style.value === "digital" ? "digital" : "analog",
      h24: settings.querySelector("input[name=h24]").checked,
      seconds: settings.querySelector("input[name=seconds]").checked,
      date: settings.querySelector("input[name=date]").checked,
    };
    if (bisa.granted("storage")) bisa.storage.set("prefs", JSON.stringify(prefs)).catch(function () {});
    schedule();
  });
  settings.addEventListener("submit", function (e) { e.preventDefault(); openSettings(false); });
  gear.addEventListener("click", function () { openSettings(settings.hidden); });
  document.addEventListener("pointerdown", function (e) { if (!settings.hidden && !settings.contains(e.target) && e.target !== gear) openSettings(false); });
  document.addEventListener("keydown", function (e) { if (e.key === "Escape" && !settings.hidden) { e.preventDefault(); openSettings(false); gear.focus(); } });

  function readPrefs(raw) {
    var next = Object.assign({}, DEFAULTS);
    try {
      var p = JSON.parse(raw);
      if (p && typeof p === "object") {
        if (p.style === "digital" || p.style === "analog") next.style = p.style;
        ["h24", "seconds", "date"].forEach(function (k) { if (typeof p[k] === "boolean") next[k] = p[k]; });
      }
    } catch (_e) { /* the defaults stand */ }
    return next;
  }
  bisa.ready().then(function (hello) {
    scheme(hello.theme.scheme);
    bisa.on("theme", function (t) { scheme(t.scheme); });
    bisa.on("visibility", function (v) { visible = !!v.visible; schedule(); });
    var start = bisa.granted("storage") ? bisa.storage.get("prefs") : Promise.resolve({ value: null });
    start.then(function (r) {
      if (r && r.value) prefs = readPrefs(r.value);
    }).catch(function () {}).then(function () { applyForm(); schedule(); });
  });
})();
