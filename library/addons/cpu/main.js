// The processor's load, from the platform's own sample, at the footer's cadence.
(function () {
  "use strict";
  var MAX = 60;
  var history = [];
  var value = document.getElementById("value");
  var note = document.getElementById("note");
  var line = document.getElementById("line");
  var fill = document.getElementById("fill");
  var dot = document.getElementById("dot");
  var unit = document.getElementById("unit");
  var bar = document.getElementById("bar");
  var barfill = document.getElementById("barfill");

  function scheme(s) { document.documentElement.dataset.scheme = s; }
  function points(values) {
    if (values.length < 2) return "";
    var step = 120 / (MAX - 1);
    var start = MAX - values.length;
    return values.map(function (v, i) { return ((start + i) * step).toFixed(1) + "," + (32 - (v / 100) * 30).toFixed(1); }).join(" ");
  }
  function draw() {
    var pts = points(history);
    line.setAttribute("points", pts);
    if (history.length >= 2) {
      var step = 120 / (MAX - 1);
      var start = MAX - history.length;
      fill.setAttribute("points", (start * step).toFixed(1) + ",32 " + pts + " 120,32");
    } else fill.setAttribute("points", "");
    if (history.length >= 1) {
      var last = history[history.length - 1];
      dot.setAttribute("cx", "120");
      dot.setAttribute("cy", (32 - (last / 100) * 30).toFixed(1));
      dot.removeAttribute("hidden");
    }
  }
  function sample(load) {
    var pct = Math.max(0, Math.min(100, Number(load.cpu_percent) || 0));
    history.push(pct);
    if (history.length > MAX) history.shift();
    value.textContent = pct.toFixed(0);
    unit.textContent = "%";
    barfill.style.width = pct.toFixed(0) + "%";
    bar.setAttribute("aria-valuenow", pct.toFixed(0));
    var avg = load.load && load.load.length ? Number(load.load[0]).toFixed(2) : "";
    note.textContent = avg ? "load " + avg + " · " + history.length + " samples" : history.length + " samples";
    draw();
  }
  bisa.ready().then(function (hello) {
    scheme(hello.theme.scheme);
    bisa.on("theme", function (t) { scheme(t.scheme); });
    if (!bisa.granted("system_load")) { note.textContent = "Grant “read the machine's load” in Settings › Addons."; return; }
    bisa.on("system.load", sample);
    bisa.system.load.subscribe().catch(function (e) { note.textContent = e.message; });
  });
})();
