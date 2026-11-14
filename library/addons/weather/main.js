// Current conditions for a city, from Open-Meteo through the platform's broker
// — the only hosts the manifest names — read again every fifteen minutes while
// shown. The rules live in `logic.js`.
(function () {
  "use strict";
  var W = window.Weather;
  var form = document.getElementById("where");
  var cityInput = document.getElementById("city");
  var now = document.getElementById("now");
  var place = document.getElementById("placeName");
  var change = document.getElementById("change");
  var back = document.getElementById("back");
  var icon = document.getElementById("icon");
  var temp = document.getElementById("temp");
  var desc = document.getElementById("desc");
  var meta = document.getElementById("meta");
  var note = document.getElementById("note");
  var spot = null;
  var timer = null;
  var visible = true;
  // `readAt`: when the reading on screen was read, and the words for it. `ticket`: the ask whose answer is shown.
  var readAt = null, readAtWords = "", ticket = 0;

  function scheme(s) { document.documentElement.dataset.scheme = s; }
  // Before the app has said its scheme: the machine's own preference, so a dark app is not shown a light flash.
  scheme(window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");
  // The whole sentence is there for a pointer where the window is too narrow for it.
  function say(text) { note.textContent = text || ""; note.title = text || ""; }
  function fetchJson(url) {
    return bisa.network.fetch({ url: url, accept: "application/json" }).then(function (r) {
      if (r.status !== 200) throw { status: r.status };
      return JSON.parse(r.body);
    });
  }
  function show(reading) {
    icon.textContent = reading.icon;
    temp.textContent = reading.temperature;
    desc.textContent = reading.text;
    meta.textContent = reading.meta;
    readAt = Date.now();
    readAtWords = new Date(readAt).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
    showNow();
    say("read at " + readAtWords);
  }
  /** A read that failed: said in plain words — and over a reading that stands, with that reading's age. */
  function failed(e) { say(readAt !== null ? W.staleWords(readAtWords) : W.failureWords(e)); }
  // One thing at a time: the reading, or the form — the form with a way back
  // while a place is known.
  function showNow() { now.hidden = false; form.hidden = true; }
  function showForm() { now.hidden = true; form.hidden = false; back.hidden = !spot; cityInput.value = spot ? spot.query || "" : ""; cityInput.focus(); cityInput.select(); }
  function backToReading() { if (!spot) return; showNow(); change.focus(); }
  /** Read the weather for the place — when it is due, or when asked outright. */
  function refresh(force) {
    if (!spot || !visible) return;
    if (!force && !W.due(readAt, Date.now())) return;
    var mine = (ticket = W.asking(ticket).ticket);
    if (readAt === null) say("reading…");
    fetchJson(W.forecastUrl(spot)).then(function (j) {
      // An answer to an ask a newer one replaced — another place, a later read — is let go.
      if (!W.lands(mine, ticket)) return;
      var reading = W.readingOf(j);
      if (!reading) throw { notReading: true };
      show(reading);
    }).catch(function (e) { if (W.lands(mine, ticket)) failed(e); });
  }
  function schedule() {
    if (timer) clearInterval(timer);
    timer = visible ? setInterval(function () { refresh(false); }, 60 * 1000) : null;
  }
  function locate(city) {
    var mine = (ticket = W.asking(ticket).ticket);
    say("finding " + city + "…");
    return fetchJson(W.geocodeUrl(city)).then(function (j) {
      if (!W.lands(mine, ticket)) return;
      var found = W.placeOf(j, city);
      if (!found) throw { noPlace: city };
      spot = found;
      // Another place: its weather has never been read.
      readAt = null;
      place.textContent = spot.name;
      place.title = spot.name;
      if (bisa.granted("storage")) bisa.storage.set("spot", JSON.stringify(spot)).catch(function () {});
      refresh(true);
      schedule();
    }).catch(function (e) { if (W.lands(mine, ticket)) say(W.failureWords(e)); });
  }
  form.addEventListener("submit", function (e) {
    e.preventDefault();
    var city = cityInput.value.trim();
    if (city) locate(city);
  });
  change.addEventListener("click", showForm);
  back.addEventListener("click", backToReading);
  form.addEventListener("keydown", function (e) { if (e.key === "Escape" && spot) { e.preventDefault(); backToReading(); } });

  bisa.ready().then(function (hello) {
    scheme(hello.theme.scheme);
    bisa.on("theme", function (t) { scheme(t.scheme); });
    // Shown again: read only when the reading is due — a tab passing over the window is no reason to ask the service.
    bisa.on("visibility", function (v) { visible = !!v.visible; schedule(); if (visible) refresh(false); });
    if (!bisa.granted("network")) { say("Grant the network permission in Settings › Addons to read the weather."); return; }
    var start = bisa.granted("storage") ? bisa.storage.get("spot") : Promise.resolve({ value: null });
    start.then(function (r) {
      if (r && r.value) { try { spot = W.spotOf(JSON.parse(r.value)); } catch (_e) { spot = null; } }
      if (spot) { place.textContent = spot.name; place.title = spot.name; refresh(true); schedule(); } else say("Type a city.");
    }).catch(function () { say("Type a city."); });
  });
})();
