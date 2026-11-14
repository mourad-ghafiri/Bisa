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
  // Before the app has said its scheme: the machine's own preference, so a dark app is not shown a light flash.
  scheme(window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");
  function gib(bytes) { return (bytes / 1073741824).toFixed(1); }
  function mib(bytes) { return (bytes / 1048576).toFixed(0) + " MiB"; }
  function sample(load) {
    var d = load.disk;
    if (!d) { value.textContent = "—"; unit.textContent = ""; note.textContent = "The volume is not read yet."; return; }
    var pct = d.total > 0 ? Math.max(0, Math.min(100, (d.used / d.total) * 100)) : 0;
    // Past a tebibyte the figures are said in tebibytes: four digits a side do not fit the window.
    var big = d.total >= 1024 * 1073741824;
    value.textContent = big ? (d.used / 1099511627776).toFixed(2) + " / " + (d.total / 1099511627776).toFixed(2) : gib(d.used) + " / " + gib(d.total);
    unit.textContent = big ? "TiB" : "GiB";
    value.parentNode.title = value.textContent + " " + unit.textContent;
    barfill.style.width = pct.toFixed(0) + "%";
    bar.setAttribute("aria-valuenow", pct.toFixed(0));
    // A size that did not come is left out, never said as NaN.
    var workspace = typeof d.workspace_bytes === "number" && isFinite(d.workspace_bytes) ? " · workspace " + (d.workspace_bytes > 1073741824 ? gib(d.workspace_bytes) + " GiB" : mib(d.workspace_bytes)) : "";
    note.textContent = pct.toFixed(0) + "% of " + (typeof d.mount === "string" ? d.mount : "the volume") + workspace;
    note.title = note.textContent;
  }
  bisa.ready().then(function (hello) {
    scheme(hello.theme.scheme);
    bisa.on("theme", function (t) { scheme(t.scheme); });
    if (!bisa.granted("system_load")) { note.textContent = "Grant “read the machine's load” in Settings › Addons."; return; }
    bisa.on("system.load", sample);
    bisa.system.load.subscribe().catch(function (e) { note.textContent = e.message; });
  });
})();
