// Start, lap, reset — time as it passes, drawn while the window is seen;
// every lap numbered, and a reset that asks once when laps would go.
(function () {
  "use strict";
  var timeEl = document.getElementById("time"), toggle = document.getElementById("toggle"), lap = document.getElementById("lap"), reset = document.getElementById("reset"), laps = document.getElementById("laps");
  var running = false, startedAt = 0, elapsed = 0, lastLap = 0, count = 0, frame = null, visible = true, armed = null;

  function scheme(s) { document.documentElement.dataset.scheme = s; }
  function fmt(ms) {
    var t = Math.floor(ms / 100), tenths = t % 10, s = Math.floor(t / 10) % 60, m = Math.floor(t / 600);
    return m + ":" + (s < 10 ? "0" : "") + s + "." + tenths;
  }
  function total() { return elapsed + (running ? Date.now() - startedAt : 0); }
  function draw() {
    timeEl.textContent = fmt(total());
    frame = null;
    if (running && visible) frame = setTimeout(draw, 100);
  }
  function render() {
    toggle.textContent = running ? "Stop" : "Start";
    lap.disabled = !running && total() === 0;
    if (!frame) draw();
  }
  function cell(cls, text) { var s = document.createElement("span"); s.className = cls; s.textContent = text; return s; }
  function disarm() {
    if (armed) clearTimeout(armed);
    armed = null;
    reset.textContent = "Reset";
    reset.classList.remove("armed");
  }
  function clear() {
    disarm();
    running = false; elapsed = 0; lastLap = 0; count = 0; laps.textContent = "";
    render();
  }
  toggle.addEventListener("click", function () {
    disarm();
    if (running) { elapsed += Date.now() - startedAt; running = false; }
    else { startedAt = Date.now(); running = true; }
    render();
  });
  lap.addEventListener("click", function () {
    disarm();
    var now = total();
    count += 1;
    var li = document.createElement("li");
    li.appendChild(cell("n", String(count)));
    li.appendChild(cell("split", fmt(now - lastLap)));
    li.appendChild(cell("total", fmt(now)));
    laps.insertBefore(li, laps.firstChild);
    lastLap = now;
  });
  // Laps are a record: the first press asks, the second clears; two seconds
  // of nothing and the question is withdrawn.
  reset.addEventListener("click", function () {
    if (count === 0 || armed) { clear(); return; }
    reset.textContent = "Sure?";
    reset.classList.add("armed");
    armed = setTimeout(disarm, 2000);
  });
  bisa.ready().then(function (hello) {
    scheme(hello.theme.scheme);
    bisa.on("theme", function (t) { scheme(t.scheme); });
    bisa.on("visibility", function (v) { visible = !!v.visible; if (visible && !frame) draw(); });
    render();
  });
})();
