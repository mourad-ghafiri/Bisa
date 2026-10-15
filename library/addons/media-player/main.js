// A player for a file from this machine, an https link, or a YouTube video:
// sound as a scrolling spectrogram and a level meter, video on the stage,
// YouTube through its own player beside the window (the platform shows it
// over the stage's box). A fresh <video> element per source, an analyser
// on it once it plays; the last twenty sources in the addon's own store.
// The rules live in `logic.js`.
(function () {
  "use strict";
  var P = window.MediaPlayer;
  function $(id) { return document.getElementById(id); }
  var app = $("app"), stage = $("stage"), empty = $("empty"), spectro = $("spectro"), barsEl = $("bars"), yt = $("yt");
  var caption = $("caption"), nameEl = $("name"), stateEl = $("state"), dropEl = $("drop");
  var play = $("play"), playIcon = $("playIcon"), now = $("now"), seek = $("seek"), total = $("total");
  var mute = $("mute"), muteIcon = $("muteIcon"), volume = $("volume"), speedBtn = $("speed"), loop = $("loop");
  var form = $("source"), link = $("link"), open = $("open"), file = $("file"), recent = $("recent"), status = $("status");
  var notice = $("notice"), ytTerms = $("ytTerms"), ytPrivacy = $("ytPrivacy"), ytAgree = $("ytAgree"), ytNotNow = $("ytNotNow");
  // YouTube's developer rules ask an app that embeds its player to show
  // these and have the person agree before the player shows.
  var TERMS_URL = "https://www.youtube.com/t/terms", PRIVACY_URL = "https://policies.google.com/privacy";
  var agreed = false, pendingYouTube = null;

  var media = null;            // the element now playing — a fresh one per source
  var current = null;          // {kind, name, url?, video?, spectrum}
  var objectUrl = null;        // the blob: URL of a dropped file, revoked on the next
  var audio = { ctx: null, analyser: null, data: null, connected: null };
  var queue = [];
  var canStore = false, youtubeAllowed = false, embedded = false, boxTimer = null;
  var visible = true, drawing = null, lastFrame = 0;
  var reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  var speed = P.SPEEDS[0], muted = false, seeking = false, dragDepth = 0;
  var spectroCtx = spectro.getContext("2d"), barsCtx = barsEl.getContext("2d");
  var column = null, sweepX = 0;
  // The stage's tokens as the page reads them: the picture is drawn in the
  // stage's own colour and the stage's own accent, so field and picture are
  // one surface in either scheme.
  var stageHex = "#15141a", stageRgb = P.hexToRgb(stageHex), stageAccent = "#fbbf24";
  function readTokens() {
    var cs = getComputedStyle(document.documentElement);
    stageHex = cs.getPropertyValue("--stage").trim() || "#15141a";
    stageRgb = P.hexToRgb(stageHex);
    stageAccent = cs.getPropertyValue("--stage-accent").trim() || "#fbbf24";
  }

  // --- words --------------------------------------------------------------
  function scheme(s) { document.documentElement.dataset.scheme = s; readTokens(); }
  function say(text, bad) { status.textContent = text || ""; status.classList.toggle("bad", !!bad); }
  function setTitle(text) { bisa.window.setTitle(text || "Media player").catch(function () {}); }
  function showCaption(name, state) {
    nameEl.textContent = name || "";
    stateEl.textContent = state || "";
    caption.hidden = !name;
  }
  function setMode(mode) {
    app.dataset.mode = mode;
    empty.hidden = mode !== "idle";
    notice.hidden = mode !== "notice";
    yt.hidden = mode !== "youtube";
    spectro.hidden = mode !== "audio";
    barsEl.hidden = mode !== "audio";
    if (media) media.hidden = mode !== "video";
    if (mode === "idle" || mode === "youtube") showCaption("");
  }

  // --- the transport ------------------------------------------------------
  function enableTransport(on) {
    play.disabled = !on;
    seek.disabled = !on;
  }
  function renderPlay() {
    var playing = media && !media.paused && !media.ended;
    playIcon.setAttribute("href", playing ? "#i-pause" : "#i-play");
    play.setAttribute("aria-label", playing ? "Pause" : "Play");
    play.title = playing ? "Pause (space)" : "Play (space)";
  }
  function renderTime() {
    if (!media) { now.textContent = "0:00"; total.textContent = "0:00"; seek.value = "0"; return; }
    var d = media.duration;
    now.textContent = P.timeWords(media.currentTime) || "0:00";
    total.textContent = P.timeWords(d) || (d === Infinity ? "live" : "0:00");
    if (!seeking) seek.value = d > 0 && d !== Infinity ? String(Math.round((media.currentTime / d) * 1000)) : "0";
  }
  function renderVolume() {
    muteIcon.setAttribute("href", muted || Number(volume.value) === 0 ? "#i-muted" : "#i-sound");
    mute.setAttribute("aria-pressed", muted ? "true" : "false");
    mute.setAttribute("aria-label", muted ? "Unmute" : "Mute");
    if (media) { media.muted = muted; media.volume = Number(volume.value) / 100; }
  }
  function toggle() {
    if (!media) return;
    if (audio.ctx && audio.ctx.state === "suspended") audio.ctx.resume().catch(function () {});
    if (media.paused || media.ended) media.play().catch(function (e) { say(e && e.message ? e.message : P.errorWords("default"), true); });
    else media.pause();
  }
  play.addEventListener("click", toggle);
  seek.addEventListener("input", function () { seeking = true; if (media && media.duration > 0 && media.duration !== Infinity) now.textContent = P.timeWords((Number(seek.value) / 1000) * media.duration); });
  seek.addEventListener("change", function () {
    seeking = false;
    if (media && media.duration > 0 && media.duration !== Infinity) media.currentTime = (Number(seek.value) / 1000) * media.duration;
  });
  mute.addEventListener("click", function () { muted = !muted; renderVolume(); });
  volume.addEventListener("input", function () { if (Number(volume.value) > 0) muted = false; renderVolume(); });
  speedBtn.addEventListener("click", function () {
    speed = P.nextSpeed(speed);
    speedBtn.textContent = P.speedWords(speed);
    if (media) media.playbackRate = speed;
  });
  loop.addEventListener("click", function () {
    var on = loop.getAttribute("aria-pressed") !== "true";
    loop.setAttribute("aria-pressed", on ? "true" : "false");
    if (media) media.loop = on;
  });

  // --- the analyser and the pictures ---------------------------------------
  function ensureAudio() {
    if (audio.ctx) return audio.ctx;
    var Ctx = window.AudioContext || window.webkitAudioContext;
    if (!Ctx) return null;
    audio.ctx = new Ctx();
    audio.analyser = audio.ctx.createAnalyser();
    audio.analyser.fftSize = 2048;
    audio.analyser.smoothingTimeConstant = 0.6;
    audio.data = new Uint8Array(audio.analyser.frequencyBinCount);
    audio.analyser.connect(audio.ctx.destination);
    return audio.ctx;
  }
  // The element's sound goes through the analyser from the first play on —
  // once connected, an element plays through the graph and nowhere else, so
  // the link that could not be read across origins is never connected.
  function connect(el) {
    if (audio.connected === el || !current || !current.spectrum) return;
    var ctx = ensureAudio();
    if (!ctx) return;
    try {
      var source = ctx.createMediaElementSource(el);
      source.connect(audio.analyser);
      audio.connected = el;
    } catch (e) {
      current.spectrum = false;
    }
    if (ctx.state === "suspended") ctx.resume().catch(function () {});
  }
  function sizeCanvases() {
    var dpr = window.devicePixelRatio || 1;
    var w = Math.max(1, Math.floor(spectro.clientWidth * dpr)), h = Math.max(1, Math.floor(spectro.clientHeight * dpr));
    if (spectro.width !== w || spectro.height !== h) {
      // The picture so far, stretched into the new size: a resize keeps the past.
      var keep = spectro.width > 1 && spectro.height > 1 ? spectroCtx.getImageData(0, 0, spectro.width, spectro.height) : null;
      var old = null;
      if (keep) { old = document.createElement("canvas"); old.width = spectro.width; old.height = spectro.height; old.getContext("2d").putImageData(keep, 0, 0); }
      spectro.width = w; spectro.height = h;
      spectroCtx.fillStyle = stageHex; spectroCtx.fillRect(0, 0, w, h);
      if (old) spectroCtx.drawImage(old, 0, 0, w, h);
      column = null;
      sweepX = Math.min(sweepX, w);
    }
    var bw = Math.max(1, Math.floor(barsEl.clientWidth * dpr)), bh = Math.max(1, Math.floor(barsEl.clientHeight * dpr));
    if (barsEl.width !== bw || barsEl.height !== bh) { barsEl.width = bw; barsEl.height = bh; }
  }
  // One column per tick, as wide as the stage needs to fill in twelve
  // seconds; the picture scrolls left — or, under reduced motion, is laid
  // down left to right and over again, a sweep with nothing in motion.
  function drawColumn() {
    var h = spectro.height, w = spectro.width, data = audio.data;
    var step = P.columnWidth(w, reduced ? 4 : 30);
    if (!column || column.width !== step || column.height !== h) column = spectroCtx.createImageData(step, h);
    var px = column.data;
    for (var row = 0; row < h; row++) {
      var v = data[P.binFor(row, h, data.length)] / 255;
      var c = P.ramp(v, stageRgb);
      var at = (h - 1 - row) * step * 4;
      for (var x = 0; x < step; x++) {
        var i = at + x * 4;
        px[i] = c[0]; px[i + 1] = c[1]; px[i + 2] = c[2]; px[i + 3] = 255;
      }
    }
    if (reduced) {
      if (sweepX + step > w) sweepX = 0;
      spectroCtx.putImageData(column, sweepX, 0);
      sweepX += step;
      return;
    }
    spectroCtx.drawImage(spectro, -step, 0);
    spectroCtx.putImageData(column, w - step, 0);
  }
  function drawBars() {
    var w = barsEl.width, h = barsEl.height, data = audio.data;
    var count = Math.max(8, Math.min(64, Math.floor(w / (4 * (window.devicePixelRatio || 1)))));
    var spans = P.barSpans(count, data.length), gap = Math.max(1, Math.floor(w / count / 4)), bar = w / count - gap;
    var floor = Math.round(P.FLOOR * 255);
    barsCtx.clearRect(0, 0, w, h);
    barsCtx.fillStyle = stageAccent;
    for (var i = 0; i < count; i++) {
      var peak = 0;
      for (var b = spans[i][0]; b < spans[i][1]; b++) if (data[b] > peak) peak = data[b];
      // Silence draws nothing: a bar stands only for a reading above the floor.
      if (peak <= floor) continue;
      var height = Math.max(1, Math.round(((peak - floor) / (255 - floor)) * (h - 2)));
      barsCtx.globalAlpha = 0.45 + 0.55 * (peak / 255);
      barsCtx.fillRect(i * (bar + gap) + gap / 2, h - height, bar, height);
    }
    barsCtx.globalAlpha = 1;
  }
  function frame(ts) {
    drawing = null;
    if (!visible || !media || media.paused || !audio.analyser || audio.connected !== media || app.dataset.mode !== "audio") return;
    var interval = reduced ? 250 : 33;
    if (ts - lastFrame >= interval) {
      lastFrame = ts;
      sizeCanvases();
      audio.analyser.getByteFrequencyData(audio.data);
      drawColumn();
      drawBars();
    }
    drawing = window.requestAnimationFrame(frame);
  }
  function startDrawing() { if (!drawing) drawing = window.requestAnimationFrame(frame); }
  function clearPictures() {
    sizeCanvases();
    spectroCtx.fillStyle = stageHex; spectroCtx.fillRect(0, 0, spectro.width, spectro.height);
    barsCtx.clearRect(0, 0, barsEl.width, barsEl.height);
    sweepX = 0;
  }

  // --- one element per source --------------------------------------------
  function dropElement() {
    if (!media) return;
    media.pause();
    media.removeAttribute("src");
    try { media.load(); } catch (e) { /* an element with no source */ }
    media.remove();
    media = null;
    enableTransport(false);
    renderPlay();
    renderTime();
  }
  function freshElement(withCors) {
    dropElement();
    var el = document.createElement("video");
    el.className = "video";
    el.setAttribute("playsinline", "");
    el.preload = "metadata";
    if (withCors) el.crossOrigin = "anonymous";
    el.loop = loop.getAttribute("aria-pressed") === "true";
    el.playbackRate = speed;
    el.hidden = true;
    el.addEventListener("loadedmetadata", function () {
      setMode(el.videoWidth > 0 ? "video" : "audio");
      if (app.dataset.mode === "audio") clearPictures();
      renderTime();
      enableTransport(true);
      say(current && !current.spectrum && current.kind === "link" ? "No spectrum for this link — its host does not let the sound be read." : "");
    });
    el.addEventListener("durationchange", renderTime);
    el.addEventListener("timeupdate", renderTime);
    // `play` fires as play() is asked, before any data; `playing` once sound
    // runs — the analyser and the pictures start there, after the metadata
    // has said whether this is sound or video.
    el.addEventListener("play", function () { renderPlay(); showCaption(current && current.name, ""); });
    el.addEventListener("playing", function () { connect(el); startDrawing(); });
    el.addEventListener("pause", function () { renderPlay(); showCaption(current && current.name, "paused"); });
    el.addEventListener("ended", function () { renderPlay(); showCaption(current && current.name, "ended"); });
    el.addEventListener("error", function () { onMediaError(el); });
    stage.insertBefore(el, spectro);
    media = el;
    renderVolume();
    return el;
  }
  function onMediaError(el) {
    var code = el.error ? el.error.code : 0;
    // A link whose host allows no cross-origin read fails outright with the
    // attribute on; the same link plays without it — with no spectrum.
    if (current && current.kind === "link" && current.spectrum && (code === 4 || code === 2)) {
      current.spectrum = false;
      var plain = freshElement(false);
      plain.src = current.url;
      plain.play().catch(function () {});
      return;
    }
    say(P.errorWords(code), true);
    showCaption(current && current.name, "failed");
    enableTransport(false);
  }

  // --- the sources --------------------------------------------------------
  function leaveYouTube() {
    if (!embedded) return;
    embedded = false;
    bisa.media.remove().catch(function () {});
  }
  function remember(entry) {
    queue = P.pushQueue(queue, entry);
    renderRecent();
    if (canStore) bisa.storage.set("recent", JSON.stringify(queue)).catch(function () {});
  }
  function loadFile(f) {
    if (!f) return;
    leaveYouTube();
    if (objectUrl) { URL.revokeObjectURL(objectUrl); objectUrl = null; }
    current = { kind: "file", name: f.name, spectrum: true };
    objectUrl = URL.createObjectURL(f);
    var el = freshElement(false);
    say("Loading…");
    setTitle(f.name);
    showCaption(f.name, "loading");
    el.src = objectUrl;
    el.play().catch(function () { /* the person presses play */ });
    remember(P.entryOf(current));
  }
  function loadLink(source) {
    leaveYouTube();
    current = { kind: "link", name: source.name, url: source.url, spectrum: true };
    var el = freshElement(true);
    say("Loading…");
    setTitle(source.name);
    showCaption(source.name, "loading");
    el.src = source.url;
    el.play().catch(function () { /* the person presses play */ });
    remember(P.entryOf(source));
  }
  function stageBox() {
    var r = stage.getBoundingClientRect();
    return P.youtubeBox({ left: r.left, top: r.top, width: r.width, height: r.height });
  }
  function openLink(url) {
    if (bisa.granted("open_url")) bisa.url.open(url).catch(function () {});
    else say("Open " + url + " in your browser.");
  }
  ytTerms.addEventListener("click", function () { openLink(TERMS_URL); });
  ytPrivacy.addEventListener("click", function () { openLink(PRIVACY_URL); });
  ytAgree.addEventListener("click", function () {
    agreed = true;
    if (canStore) bisa.storage.set("youtube-agreed", "1").catch(function () {});
    var source = pendingYouTube;
    pendingYouTube = null;
    if (source) loadYouTube(source);
  });
  ytNotNow.addEventListener("click", function () {
    pendingYouTube = null;
    setMode(media ? (app.dataset.mode === "notice" && media.videoWidth > 0 ? "video" : "audio") : "idle");
    if (!media) say("");
  });
  function loadYouTube(source) {
    if (!youtubeAllowed) { say(P.errorWords("youtube-not-allowed"), true); return; }
    if (!agreed) {
      // The notice first, once; what played goes on until the person decides.
      pendingYouTube = source;
      setMode("notice");
      say("");
      return;
    }
    dropElement();
    current = { kind: "youtube", name: "YouTube", video: source.video, spectrum: false };
    setMode("youtube");
    say("");
    setTitle("YouTube");
    var box = stageBox();
    if (!box) { say(P.errorWords("too-small"), true); return; }
    embedded = true;
    bisa.media.embed({ kind: "youtube", video: source.video, box: box }).catch(function (e) {
      embedded = false;
      say(e && e.message ? e.message : P.errorWords("unavailable"), true);
    });
    remember(P.entryOf(source));
  }
  function placeYouTube() {
    if (!embedded) return;
    var box = stageBox();
    if (!box) { say(P.errorWords("too-small"), true); return; }
    say("");
    bisa.media.box({ box: box }).catch(function () {});
  }
  function playSource(text) {
    var source = P.parseSource(text);
    if (source.kind === "none") { say(source.reason === "empty" ? "" : P.errorWords(source.reason), !!source.reason); return false; }
    if (source.kind === "youtube") loadYouTube(source);
    else loadLink(source);
    return true;
  }
  form.addEventListener("submit", function (e) {
    e.preventDefault();
    if (playSource(link.value)) { link.value = ""; link.blur(); }
  });
  open.addEventListener("click", function () { file.click(); });
  file.addEventListener("change", function () { loadFile(file.files && file.files[0]); file.value = ""; });

  // A file dragged over the window: the stage says so, and the drop plays it.
  document.addEventListener("dragenter", function (e) { e.preventDefault(); dragDepth += 1; dropEl.hidden = false; });
  document.addEventListener("dragover", function (e) { e.preventDefault(); if (e.dataTransfer) e.dataTransfer.dropEffect = "copy"; });
  document.addEventListener("dragleave", function () { dragDepth = Math.max(0, dragDepth - 1); if (dragDepth === 0) dropEl.hidden = true; });
  document.addEventListener("drop", function (e) {
    e.preventDefault();
    dragDepth = 0;
    dropEl.hidden = true;
    var files = e.dataTransfer && e.dataTransfer.files;
    if (files && files.length) { loadFile(files[0]); return; }
    var text = e.dataTransfer ? e.dataTransfer.getData("text/uri-list") || e.dataTransfer.getData("text/plain") : "";
    if (text) playSource(text.split("\n")[0]);
  });

  // --- the recent list ------------------------------------------------------
  function renderRecent() {
    recent.textContent = "";
    var head = document.createElement("option");
    head.value = ""; head.disabled = true; head.selected = true; head.textContent = "Recent";
    recent.appendChild(head);
    queue.forEach(function (entry, i) {
      var o = document.createElement("option");
      o.value = String(i);
      o.textContent = entry.label;
      recent.appendChild(o);
    });
    recent.disabled = queue.length === 0;
  }
  recent.addEventListener("change", function () {
    var entry = queue[Number(recent.value)];
    renderRecent();
    if (!entry) return;
    if (entry.kind === "youtube") loadYouTube({ kind: "youtube", video: entry.video, title: entry.label });
    else if (entry.kind === "link") loadLink({ kind: "link", url: entry.url, name: entry.label });
    else say("Drop " + entry.label + " again to play it — a file is not kept.");
  });

  // --- keys ---------------------------------------------------------------------
  document.addEventListener("keydown", function (e) {
    var inField = e.target === link || e.target === recent;
    if (e.key === "/" && !inField) { e.preventDefault(); link.focus(); return; }
    if (inField || e.metaKey || e.ctrlKey || e.altKey) return;
    if (app.dataset.mode === "youtube" || app.dataset.mode === "idle") return;
    switch (e.key) {
      case " ": e.preventDefault(); toggle(); break;
      case "ArrowLeft": if (media) { e.preventDefault(); media.currentTime = P.seekBy(media.currentTime, -5, media.duration); } break;
      case "ArrowRight": if (media) { e.preventDefault(); media.currentTime = P.seekBy(media.currentTime, 5, media.duration); } break;
      case "ArrowUp": e.preventDefault(); volume.value = String(P.volumeStep(Number(volume.value), 5)); muted = false; renderVolume(); break;
      case "ArrowDown": e.preventDefault(); volume.value = String(P.volumeStep(Number(volume.value), -5)); renderVolume(); break;
      case "m": muted = !muted; renderVolume(); break;
      case "l": loop.click(); break;
      default: return;
    }
  });

  // --- the window ----------------------------------------------------------------
  var fit = window.ResizeObserver ? new ResizeObserver(function () {
    app.classList.toggle("mini", app.clientWidth < 340);
    if (app.dataset.mode === "audio") sizeCanvases();
    if (embedded) {
      if (boxTimer) clearTimeout(boxTimer);
      boxTimer = setTimeout(placeYouTube, 80);
    }
  }) : null;
  if (fit) fit.observe(app);
  app.classList.toggle("mini", app.clientWidth < 340);

  // --- the platform ----------------------------------------------------------------
  scheme(window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");
  readTokens();
  setMode("idle");
  renderRecent();
  renderVolume();
  speedBtn.textContent = P.speedWords(speed);
  bisa.ready().then(function (hello) {
    scheme(hello.theme.scheme);
    canStore = bisa.granted("storage");
    youtubeAllowed = bisa.granted("media");
    if (canStore) {
      bisa.storage.get("recent").then(function (r) { queue = P.readQueue(r.value); renderRecent(); }).catch(function () {});
      bisa.storage.get("youtube-agreed").then(function (r) { agreed = r.value === "1"; }).catch(function () {});
    }
  });
  bisa.on("theme", function (p) { scheme(p.scheme); });
  bisa.on("visibility", function (p) {
    visible = !!p.visible;
    if (visible) startDrawing();
  });
  // What YouTube's player said, through the platform.
  bisa.on("media", function (p) {
    if (!p || app.dataset.mode !== "youtube") return;
    if (p.title) { setTitle(p.title); if (current) current.name = p.title; }
    if (p.state === "error") {
      say(p.words || P.errorWords(p.code), true);
      if (p.code === "unavailable") embedded = false;
      return;
    }
    yt.hidden = p.state !== "ready" ? true : yt.hidden;
    say(p.state === "playing" ? "Playing " + (p.title || "on YouTube") : p.state === "paused" ? "Paused" : p.state === "ended" ? "Ended" : "");
  });
})();
