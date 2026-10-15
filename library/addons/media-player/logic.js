// The media player's rules, with no page in them: what a pasted source is,
// how a time reads, the spectrogram's ramp and its rows, the recent list,
// the YouTube player's box, and the words for a failure. `main.js` draws;
// `desktop/src/scenarios/addonsLogic.test.mjs` runs this alone.
(function (global) {
  "use strict";

  /** A YouTube page, in every spelling the site uses: watch, short links, shorts, embeds, live. */
  var YOUTUBE = /^(?:https?:\/\/)?(?:www\.|m\.|music\.)?(?:youtube\.com\/(?:watch\?(?:[^#\s]*&)?v=|shorts\/|embed\/|live\/|v\/)|youtu\.be\/)([A-Za-z0-9_-]{11})(?![A-Za-z0-9_-])/i;
  var HTTPS = /^https:\/\/\S+$/i;
  var HTTP = /^http:\/\//i;

  /** The speeds the button cycles through. */
  var SPEEDS = [1, 1.25, 1.5, 2, 0.75];
  /** How many recent sources are kept. */
  var RECENT_CAP = 20;
  /** The least a YouTube player may be on either side — YouTube's own floor. */
  var MIN_PLAYER_SIDE = 200;
  /** The stage's colour when the page cannot read its token. */
  var STAGE = [21, 20, 26];
  /** Up to here a reading is the floor — the analyser's hiss reads as stage, not as a glow. */
  var FLOOR = 0.1;
  /** The ramp past the floor, cold to warm: indigo, teal, amber, warm white. */
  var RAMP = [
    [0.32, [39, 48, 107]],
    [0.58, [47, 127, 168]],
    [0.82, [224, 184, 74]],
    [1, [255, 244, 214]],
  ];
  /** A stage fills in about this many seconds whatever its width. */
  var FILL_SECONDS = 12;

  /**
   * What a pasted source is: a YouTube video, a media link, or nothing with
   * its reason — `empty`, `http` (plain http is refused: the window's walls
   * admit https alone), or `not-a-link`.
   */
  function parseSource(text) {
    var s = String(text || "").trim();
    if (!s) return { kind: "none", reason: "empty" };
    var yt = YOUTUBE.exec(s);
    if (yt) return { kind: "youtube", video: yt[1] };
    if (HTTP.test(s)) return { kind: "none", reason: "http" };
    if (HTTPS.test(s)) return { kind: "link", url: s, name: linkName(s) };
    return { kind: "none", reason: "not-a-link" };
  }

  /** The last path segment of a link, without its query — what the caption and the recent list call it. */
  function linkName(url) {
    var path = String(url).replace(/^https?:\/\/[^/]+/i, "").replace(/[?#].*$/, "");
    var parts = path.split("/").filter(function (p) { return p; });
    var last = parts.length ? parts[parts.length - 1] : "";
    try { last = decodeURIComponent(last); } catch (e) { /* kept as written */ }
    return last || String(url).replace(/^https?:\/\//i, "").replace(/\/.*$/, "");
  }

  /** `m:ss`, or `h:mm:ss` past an hour; nothing for a time that is not a number (a live stream). */
  function timeWords(seconds) {
    if (typeof seconds !== "number" || seconds !== seconds || seconds === Infinity || seconds < 0) return "";
    var whole = Math.floor(seconds);
    var h = Math.floor(whole / 3600), m = Math.floor((whole % 3600) / 60), s = whole % 60;
    var two = function (n) { return (n < 10 ? "0" : "") + n; };
    return h > 0 ? h + ":" + two(m) + ":" + two(s) : m + ":" + two(s);
  }

  /**
   * The ramp at `t` in 0..1, as `[r, g, b]`: the stage's own colour up to the
   * floor, then cold to warm. `stage` is the stage token as the page reads it,
   * so the picture and the field it scrolls over are one surface in either scheme.
   */
  function ramp(t, stage) {
    var base = stage || STAGE;
    var x = t !== t ? 0 : Math.max(0, Math.min(1, t));
    if (x <= FLOOR) return base.slice();
    var prev = [FLOOR, base];
    for (var i = 0; i < RAMP.length; i++) {
      if (x <= RAMP[i][0]) {
        var a = prev, b = RAMP[i];
        var f = (x - a[0]) / (b[0] - a[0]);
        return [
          Math.round(a[1][0] + (b[1][0] - a[1][0]) * f),
          Math.round(a[1][1] + (b[1][1] - a[1][1]) * f),
          Math.round(a[1][2] + (b[1][2] - a[1][2]) * f),
        ];
      }
      prev = RAMP[i];
    }
    return RAMP[RAMP.length - 1][1].slice();
  }

  /** `#rrggbb` as `[r, g, b]` — the stage when it is not one. */
  function hexToRgb(hex) {
    var m = /^#?([0-9a-f]{6})$/i.exec(String(hex || "").trim());
    if (!m) return STAGE.slice();
    var n = parseInt(m[1], 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  }

  /** How wide one spectrogram column is, in device pixels, so a stage `width` wide fills in `FILL_SECONDS` at `ticksPerSecond`. */
  function columnWidth(width, ticksPerSecond) {
    var ticks = Math.max(1, (ticksPerSecond || 30) * FILL_SECONDS);
    return Math.max(1, Math.round((width || 0) / ticks));
  }

  /**
   * The frequency bin a spectrogram row shows, bottom row first, on a
   * logarithmic scale — the bass gets its rows, the hiss is not half the
   * picture. `rows` rows over `bins` bins; the top row is the last bin.
   */
  function binFor(row, rows, bins) {
    if (rows <= 1 || bins <= 1) return 0;
    var top = bins - 1;
    var bin = Math.round(Math.pow(top, row / (rows - 1)));
    return Math.max(0, Math.min(top, bin));
  }

  /** The bars' bins: `count` bars over `bins` bins, each the loudest of its log-spaced span. */
  function barSpans(count, bins) {
    var spans = [];
    for (var i = 0; i < count; i++) {
      var from = binFor(i, count + 1, bins), to = binFor(i + 1, count + 1, bins);
      spans.push([Math.min(from, to), Math.max(from + 1, to)]);
    }
    return spans;
  }

  /** The recent list with one more entry in front — the same key moves up, never doubles — kept to the cap. */
  function pushQueue(list, entry, cap) {
    var limit = typeof cap === "number" ? cap : RECENT_CAP;
    var out = [entry];
    for (var i = 0; i < (list || []).length && out.length < limit; i++) {
      if (list[i] && list[i].key !== entry.key) out.push(list[i]);
    }
    return out;
  }

  /** A recent list as the store kept it: entries with a kind, a key and a label, nothing else trusted. */
  function readQueue(raw) {
    var parsed;
    try { parsed = JSON.parse(raw); } catch (e) { return []; }
    if (!Array.isArray(parsed)) return [];
    var out = [];
    for (var i = 0; i < parsed.length && out.length < RECENT_CAP; i++) {
      var e = parsed[i];
      if (!e || typeof e !== "object" || typeof e.key !== "string" || typeof e.label !== "string") continue;
      if (e.kind === "youtube" && typeof e.video === "string") out.push({ kind: "youtube", key: e.key, label: e.label, video: e.video });
      else if (e.kind === "link" && typeof e.url === "string") out.push({ kind: "link", key: e.key, label: e.label, url: e.url });
      else if (e.kind === "file") out.push({ kind: "file", key: e.key, label: e.label });
    }
    return out;
  }

  /** An entry for the recent list, from a parsed source or a file's name. */
  function entryOf(source) {
    if (source.kind === "youtube") return { kind: "youtube", key: "yt:" + source.video, label: source.title || "YouTube · " + source.video, video: source.video };
    if (source.kind === "link") return { kind: "link", key: "link:" + source.url, label: source.name || source.url, url: source.url };
    return { kind: "file", key: "file:" + source.name, label: source.name };
  }

  /**
   * Where YouTube's player stands: the stage, less an inset, whole — the
   * player letterboxes its own video. Null when the stage is under the
   * player's floor on either side, so the page can say to grow the window.
   * The box is from the window's top-left, as the stage's rect reads.
   */
  function youtubeBox(stage, inset) {
    var pad = typeof inset === "number" ? inset : 4;
    var width = Math.floor(stage.width - pad * 2), height = Math.floor(stage.height - pad * 2);
    if (width < MIN_PLAYER_SIDE || height < MIN_PLAYER_SIDE) return null;
    return { x: Math.round(stage.left + pad), y: Math.round(stage.top + pad), width: width, height: height };
  }

  /** A seek by `delta` seconds, kept inside the track. */
  function seekBy(current, delta, duration) {
    var end = typeof duration === "number" && duration === duration && duration !== Infinity ? duration : Infinity;
    return Math.max(0, Math.min(end, (current || 0) + delta));
  }

  /** A volume step in 0..100. */
  function volumeStep(value, delta) {
    return Math.max(0, Math.min(100, Math.round((value || 0) + delta)));
  }

  /** The next speed after `speed`, round the list. */
  function nextSpeed(speed) {
    var at = SPEEDS.indexOf(speed);
    return SPEEDS[(at + 1) % SPEEDS.length];
  }

  /** A speed as the button says it. */
  function speedWords(speed) {
    return String(speed).replace(/\.0$/, "") + "×";
  }

  /** The sentence for a media element's error code, or for the words the player page said. */
  function errorWords(code) {
    switch (String(code)) {
      case "1": return "Playback was stopped.";
      case "2": return "The file could not be read to the end.";
      case "3": return "This file could not be decoded.";
      case "4": return "Nothing here can play this file or link.";
      case "http": return "Only https:// links play here.";
      case "not-a-link": return "That is not a link — paste a media URL or a YouTube page.";
      case "unavailable": return "The YouTube player cannot be shown here.";
      case "youtube-not-allowed": return "YouTube needs the media permission with its player.";
      case "too-small": return "Make the window taller to show the YouTube player.";
      default: return "This could not be played.";
    }
  }

  global.MediaPlayer = {
    SPEEDS: SPEEDS,
    RECENT_CAP: RECENT_CAP,
    MIN_PLAYER_SIDE: MIN_PLAYER_SIDE,
    FLOOR: FLOOR,
    FILL_SECONDS: FILL_SECONDS,
    parseSource: parseSource,
    linkName: linkName,
    timeWords: timeWords,
    ramp: ramp,
    hexToRgb: hexToRgb,
    columnWidth: columnWidth,
    binFor: binFor,
    barSpans: barSpans,
    pushQueue: pushQueue,
    readQueue: readQueue,
    entryOf: entryOf,
    youtubeBox: youtubeBox,
    seekBy: seekBy,
    volumeStep: volumeStep,
    nextSpeed: nextSpeed,
    speedWords: speedWords,
    errorWords: errorWords,
  };
})(typeof window !== "undefined" ? window : this);
