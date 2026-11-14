// The weather's rules, pure: what a reading is, what a place is, when a
// reading is due, which answer is shown, and how a failure is said. Loaded
// before `main.js`; exposed as one global so a Node test can run this file
// alone. No clock and no network here — `main.js` hands in the time.
(function (root) {
  "use strict";
  var GEOCODE = "https://geocoding-api.open-meteo.com/v1/search?count=1&language=en&format=json&name=";
  var FORECAST = "https://api.open-meteo.com/v1/forecast?current=temperature_2m,relative_humidity_2m,weather_code,wind_speed_10m&latitude=";
  /** How long a reading stands before another is asked for. */
  var REFRESH_MS = 15 * 60 * 1000;

  // WMO weather interpretation codes, as Open-Meteo documents them.
  var CODES = [
    [[0], "☀️", "Clear sky"], [[1], "🌤", "Mainly clear"], [[2], "⛅", "Partly cloudy"], [[3], "☁️", "Overcast"],
    [[45, 48], "🌫", "Fog"], [[51, 53, 55], "🌦", "Drizzle"], [[56, 57], "🌧", "Freezing drizzle"],
    [[61, 63, 65], "🌧", "Rain"], [[66, 67], "🌧", "Freezing rain"], [[71, 73, 75], "🌨", "Snow"], [[77], "🌨", "Snow grains"],
    [[80, 81, 82], "🌦", "Rain showers"], [[85, 86], "🌨", "Snow showers"], [[95], "⛈", "Thunderstorm"], [[96, 99], "⛈", "Thunderstorm with hail"],
  ];
  function describe(code) {
    for (var i = 0; i < CODES.length; i++) if (CODES[i][0].indexOf(code) !== -1) return { icon: CODES[i][1], text: CODES[i][2] };
    return { icon: "🌡", text: "Conditions unknown" };
  }
  function num(v) { return typeof v === "number" && isFinite(v) ? v : null; }

  /**
   * A forecast answer as what the window shows — or `null` when it is no
   * reading: a temperature and a weather code that are numbers, or nothing.
   * An answer of another shape used to be drawn as *NaN°C* under *read at*.
   */
  function readingOf(json) {
    var c = json && typeof json === "object" ? json.current : null;
    if (!c || typeof c !== "object") return null;
    var t = num(c.temperature_2m), code = num(c.weather_code);
    if (t === null || code === null) return null;
    var units = json.current_units && typeof json.current_units === "object" ? json.current_units : {};
    var d = describe(code), parts = [];
    var wind = num(c.wind_speed_10m), humidity = num(c.relative_humidity_2m);
    if (wind !== null) parts.push("wind " + Math.round(wind) + " " + (typeof units.wind_speed_10m === "string" ? units.wind_speed_10m : "km/h"));
    if (humidity !== null) parts.push("humidity " + Math.round(humidity) + "%");
    // `Math.round(-3.5)` is -3 and `Math.round(-0.4)` is -0: a minus only before a number that has one.
    var whole = Math.round(t);
    return { icon: d.icon, text: d.text, temperature: (whole === 0 ? 0 : whole) + (typeof units.temperature_2m === "string" ? units.temperature_2m : "°C"), meta: parts.join(" · ") };
  }

  /** A stored spot: a place with a name and coordinates on the globe — anything else is no spot. */
  function spotOf(v) {
    if (!v || typeof v !== "object" || typeof v.name !== "string" || v.name === "") return null;
    var lat = Number(v.lat), lon = Number(v.lon);
    if (!isFinite(lat) || !isFinite(lon) || Math.abs(lat) > 90 || Math.abs(lon) > 180) return null;
    return { name: v.name, lat: lat, lon: lon, query: typeof v.query === "string" ? v.query : "" };
  }
  /** The first place a search answered, as a spot — or `null` when it found none. */
  function placeOf(json, query) {
    var r = json && Array.isArray(json.results) ? json.results[0] : null;
    if (!r || typeof r.name !== "string") return null;
    return spotOf({ name: r.name + (typeof r.country_code === "string" && r.country_code ? ", " + r.country_code : ""), lat: r.latitude, lon: r.longitude, query: query });
  }
  function geocodeUrl(city) { return GEOCODE + encodeURIComponent(city); }
  function forecastUrl(spot) { return FORECAST + spot.lat + "&longitude=" + spot.lon; }

  /** Whether a reading is due: never read, old enough, or read at a time the clock has since gone back past. */
  function due(readAt, now) {
    return typeof readAt !== "number" || now < readAt || now - readAt >= REFRESH_MS;
  }
  /** A new ask: the ticket its answer must show. */
  function asking(last) { return { ticket: last + 1 }; }
  /** Whether an answer is the one asked for last — an older one is let go. */
  function lands(ticket, latest) { return ticket === latest; }

  /** What went wrong, as a person reads it. */
  function failureWords(e) {
    if (e && typeof e.noPlace === "string") return "No place called " + e.noPlace + " was found.";
    if (e && e.notReading) return "The weather service answered something that could not be read.";
    if (e && typeof e.status === "number") return e.status === 429 ? "The weather service is busy — it will be tried again." : "The weather service is having trouble.";
    if (e && e.name === "SyntaxError") return "The weather service answered something that could not be read.";
    if (e && e.code === "unavailable") return "The weather service did not answer.";
    return "The weather service could not be reached.";
  }
  /** A refresh that failed over a reading that stands: its age is kept in view. */
  function staleWords(readAtWords) { return "Could not refresh — last read at " + readAtWords + "."; }

  root.Weather = {
    REFRESH_MS: REFRESH_MS, describe: describe, readingOf: readingOf, spotOf: spotOf, placeOf: placeOf,
    geocodeUrl: geocodeUrl, forecastUrl: forecastUrl, due: due, asking: asking, lands: lands,
    failureWords: failureWords, staleWords: staleWords,
  };
})(this);
