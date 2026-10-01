// Length, mass and temperature, either way — the pair swapped in one click; the last pair remembered.
(function () {
  "use strict";
  var KINDS = {
    length: { name: "Length", units: { m: 1, km: 1000, cm: 0.01, mm: 0.001, mi: 1609.344, yd: 0.9144, ft: 0.3048, in: 0.0254 } },
    mass: { name: "Mass", units: { kg: 1, g: 0.001, mg: 0.000001, t: 1000, lb: 0.45359237, oz: 0.028349523125 } },
    temperature: { name: "Temperature", units: { "°C": null, "°F": null, K: null } },
  };
  var kind = document.getElementById("kind"), from = document.getElementById("from"), fromUnit = document.getElementById("fromUnit"), to = document.getElementById("to"), toUnit = document.getElementById("toUnit"), swap = document.getElementById("swap");
  var pref = { kind: "length", from: "m", to: "ft" };

  function scheme(s) { document.documentElement.dataset.scheme = s; }
  function fill(select, names, chosen) {
    select.textContent = "";
    names.forEach(function (n) { var o = document.createElement("option"); o.value = n; o.textContent = n; if (n === chosen) o.selected = true; select.appendChild(o); });
  }
  function toKelvin(v, u) { return u === "K" ? v : u === "°C" ? v + 273.15 : (v - 32) * (5 / 9) + 273.15; }
  function fromKelvin(k, u) { return u === "K" ? k : u === "°C" ? k - 273.15 : (k - 273.15) * (9 / 5) + 32; }
  function convert() {
    var v = parseFloat(from.value);
    if (!isFinite(v)) { to.textContent = "—"; return; }
    var k = KINDS[pref.kind];
    var result;
    if (pref.kind === "temperature") result = fromKelvin(toKelvin(v, pref.from), pref.to);
    else result = (v * k.units[pref.from]) / k.units[pref.to];
    to.textContent = String(Number(result.toPrecision(8)));
  }
  function setKind(name, keep) {
    pref.kind = name;
    var names = Object.keys(KINDS[name].units);
    if (!keep) { pref.from = names[0]; pref.to = names[1]; }
    fill(fromUnit, names, pref.from);
    fill(toUnit, names, pref.to);
    convert();
  }
  function remember() { if (bisa.granted("storage")) bisa.storage.set("pref", JSON.stringify(pref)).catch(function () {}); }
  fill(kind, Object.keys(KINDS), pref.kind);
  kind.addEventListener("change", function () { setKind(kind.value, false); remember(); });
  fromUnit.addEventListener("change", function () { pref.from = fromUnit.value; convert(); remember(); });
  toUnit.addEventListener("change", function () { pref.to = toUnit.value; convert(); remember(); });
  from.addEventListener("input", convert);
  // Swap: the units trade places and the result becomes the value, so the
  // same quantity reads the other way.
  swap.addEventListener("click", function () {
    var was = pref.from;
    pref.from = pref.to;
    pref.to = was;
    var result = parseFloat(to.textContent);
    if (isFinite(result)) from.value = String(result);
    fill(fromUnit, Object.keys(KINDS[pref.kind].units), pref.from);
    fill(toUnit, Object.keys(KINDS[pref.kind].units), pref.to);
    convert();
    remember();
  });

  bisa.ready().then(function (hello) {
    scheme(hello.theme.scheme);
    bisa.on("theme", function (t) { scheme(t.scheme); });
    var start = bisa.granted("storage") ? bisa.storage.get("pref") : Promise.resolve({ value: null });
    start.then(function (r) {
      if (r && r.value) { try { var p = JSON.parse(r.value); if (KINDS[p.kind] && KINDS[p.kind].units[p.from] !== undefined && KINDS[p.kind].units[p.to] !== undefined) pref = p; } catch (_e) { /* defaults */ } }
    }).catch(function () {}).then(function () { fill(kind, Object.keys(KINDS), pref.kind); setKind(pref.kind, true); });
  });
})();
