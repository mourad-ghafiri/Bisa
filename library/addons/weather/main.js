// Current conditions for a city, from Open-Meteo through the platform's broker
// — the only hosts the manifest names — refreshed every fifteen minutes while shown.
(function () {
  "use strict";
  var GEOCODE = "https://geocoding-api.open-meteo.com/v1/search?count=1&language=en&format=json&name=";
  var FORECAST = "https://api.open-meteo.com/v1/forecast?current=temperature_2m,relative_humidity_2m,weather_code,wind_speed_10m&latitude=";
  var REFRESH_MS = 15 * 60 * 1000;
  var form = document.getElementById("where");
  var cityInput = document.getElementById("city");
  var now = document.getElementById("now");
  var place = document.getElementById("placeName");
  var back = document.getElementById("back");
  var icon = document.getElementById("icon");
  var temp = document.getElementById("temp");
  var desc = document.getElementById("desc");
  var meta = document.getElementById("meta");
  var note = document.getElementById("note");
  var spot = null;
  var timer = null;
  var visible = true;

  // WMO weather interpretation codes, as Open-Meteo documents them.
  var CODES = [
    [[0], "☀️", "Clear sky"], [[1], "🌤", "Mainly clear"], [[2], "⛅", "Partly cloudy"], [[3], "☁️", "Overcast"],
    [[45, 48], "🌫", "Fog"], [[51, 53, 55], "🌦", "Drizzle"], [[56, 57], "🌧", "Freezing drizzle"],
    [[61, 63, 65], "🌧", "Rain"], [[66, 67], "🌧", "Freezing rain"], [[71, 73, 75], "🌨", "Snow"], [[77], "🌨", "Snow grains"],
    [[80, 81, 82], "🌦", "Rain showers"], [[85, 86], "🌨", "Snow showers"], [[95], "⛈", "Thunderstorm"], [[96, 99], "⛈", "Thunderstorm with hail"],
  ];
  function describe(code) {
    for (var i = 0; i < CODES.length; i++) if (CODES[i][0].indexOf(code) !== -1) return { icon: CODES[i][1], text: CODES[i][2] };
    return { icon: "🌡", text: "Code " + code };
  }
  function scheme(s) { document.documentElement.dataset.scheme = s; }
  function say(text) { note.textContent = text || ""; }
  function fetchJson(url) {
    return bisa.network.fetch({ url: url, accept: "application/json" }).then(function (r) {
      if (r.status !== 200) throw new Error("the service answered " + r.status);
      return JSON.parse(r.body);
    });
  }
  function show(current, units) {
    var d = describe(Number(current.weather_code));
    icon.textContent = d.icon;
    temp.textContent = Math.round(Number(current.temperature_2m)) + (units.temperature_2m || "°C");
    desc.textContent = d.text;
    meta.textContent = "wind " + Math.round(Number(current.wind_speed_10m)) + " " + (units.wind_speed_10m || "km/h") + " · humidity " + Number(current.relative_humidity_2m) + "%";
    showNow();
    say("read at " + new Date().toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" }));
  }
  // One thing at a time: the reading, or the form — the form with a way back
  // while a place is known.
  function showNow() { now.hidden = false; form.hidden = true; }
  function showForm() { now.hidden = true; form.hidden = false; back.hidden = !spot; cityInput.value = spot ? spot.query || "" : ""; cityInput.focus(); cityInput.select(); }
  function refresh() {
    if (!spot || !visible) return;
    say("reading…");
    fetchJson(FORECAST + spot.lat + "&longitude=" + spot.lon).then(function (j) {
      show(j.current || {}, j.current_units || {});
    }).catch(function (e) { say(e.message); });
  }
  function schedule() {
    if (timer) clearInterval(timer);
    timer = visible ? setInterval(refresh, REFRESH_MS) : null;
  }
  function locate(city) {
    say("finding " + city + "…");
    return fetchJson(GEOCODE + encodeURIComponent(city)).then(function (j) {
      var r = j.results && j.results[0];
      if (!r) throw new Error("no place called " + city);
      spot = { name: r.name + (r.country_code ? ", " + r.country_code : ""), lat: r.latitude, lon: r.longitude, query: city };
      place.textContent = spot.name;
      if (bisa.granted("storage")) bisa.storage.set("spot", JSON.stringify(spot)).catch(function () {});
      refresh();
      schedule();
    }).catch(function (e) { say(e.message); });
  }
  form.addEventListener("submit", function (e) {
    e.preventDefault();
    var city = cityInput.value.trim();
    if (city) locate(city);
  });
  document.getElementById("change").addEventListener("click", showForm);
  back.addEventListener("click", function () { if (spot) showNow(); });
  form.addEventListener("keydown", function (e) { if (e.key === "Escape" && spot) { e.preventDefault(); showNow(); } });

  bisa.ready().then(function (hello) {
    scheme(hello.theme.scheme);
    bisa.on("theme", function (t) { scheme(t.scheme); });
    bisa.on("visibility", function (v) { visible = !!v.visible; schedule(); if (visible) refresh(); });
    if (!bisa.granted("network")) { say("Grant the network permission in Settings › Addons to read the weather."); return; }
    var start = bisa.granted("storage") ? bisa.storage.get("spot") : Promise.resolve({ value: null });
    start.then(function (r) {
      if (r && r.value) { try { spot = JSON.parse(r.value); } catch (_e) { spot = null; } }
      if (spot) { place.textContent = spot.name; refresh(); schedule(); } else say("Type a city.");
    }).catch(function () { say("Type a city."); });
  });
})();
