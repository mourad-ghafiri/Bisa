// How many things wait on you, are under review, or are running — the same
// counts the pet reads; a click goes to the Inbox or the Goals.
(function () {
  "use strict";
  var cells = { waiting: document.getElementById("waiting"), review: document.getElementById("review"), working: document.getElementById("working") };
  var note = document.getElementById("note");
  function scheme(s) { document.documentElement.dataset.scheme = s; }
  // Before the app has said its scheme: the machine's own preference, so a dark app is not shown a light flash.
  scheme(window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");
  function show(summary) {
    if (!summary) return;
    Object.keys(cells).forEach(function (k) {
      var n = Number(summary[k]) || 0;
      cells[k].querySelector(".n").textContent = String(n);
      cells[k].classList.toggle("hot", n > 0 && k !== "working");
    });
  }
  function go(route) {
    return function () {
      // A door that cannot open says why: it looked like a button and did nothing.
      if (!bisa.granted("navigate")) { note.textContent = "Grant “open one of the app's screens” in Settings › Addons to go there."; return; }
      bisa.navigate({ route: route }).catch(function (e) { note.textContent = e.message; });
    };
  }
  cells.waiting.addEventListener("click", go("inbox"));
  cells.review.addEventListener("click", go("goals"));
  bisa.ready().then(function (hello) {
    scheme(hello.theme.scheme);
    bisa.on("theme", function (t) { scheme(t.scheme); });
    if (!bisa.granted("workspace_summary")) { note.textContent = "Grant “read the workspace's summary” in Settings › Addons."; return; }
    bisa.on("workspace.summary", show);
    bisa.workspace.summary().then(show).catch(function (e) { note.textContent = e.message; });
  });
})();
