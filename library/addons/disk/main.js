// The volume the platform's data lives on, and what the workspace itself weighs.
(function () {
  "use strict";
  var value = document.getElementById("value");
  var note = document.getElementById("note");
  var bar = document.getElementById("bar");
  var barfill = document.getElementById("barfill");
  var unit = document.getElementById("unit");
  // No sparkline: a volume does not move by the second.
  document.getElementById("spark").setAttribute("hidden", "");

  function scheme(s) { document.documentElement.dataset.scheme = s; }
  function gib(bytes) { return (bytes / 1073741824).toFixed(1); }
  function mib(bytes) { return (bytes / 1048576).toFixed(0) + " MiB"; }
  function sample(load) {
    var d = load.disk;
    if (!d) { value.textContent = "—"; unit.textContent = ""; note.textContent = "The volume is not read yet."; return; }
    var pct = d.total > 0 ? Math.max(0, Math.min(100, (d.used / d.total) * 100)) : 0;
    value.textContent = gib(d.used) + " / " + gib(d.total);
    unit.textContent = "GiB";
    barfill.style.width = pct.toFixed(0) + "%";
    bar.setAttribute("aria-valuenow", pct.toFixed(0));
    note.textContent = pct.toFixed(0) + "% of " + d.mount + " · workspace " + (d.workspace_bytes > 1073741824 ? gib(d.workspace_bytes) + " GiB" : mib(d.workspace_bytes));
  }
  bisa.ready().then(function (hello) {
    scheme(hello.theme.scheme);
    bisa.on("theme", function (t) { scheme(t.scheme); });
    if (!bisa.granted("system_load")) { note.textContent = "Grant “read the machine's load” in Settings › Addons."; return; }
    bisa.on("system.load", sample);
    bisa.system.load.subscribe().catch(function (e) { note.textContent = e.message; });
  });
})();
