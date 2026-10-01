// Many notes on one pad — a dot per note, six papers, basic formatting —
// each kept in the addon's own store as you type: one key per note and an
// index of them; *Copy* puts the note's text on the clipboard. The rules
// live in `logic.js`.
(function () {
  "use strict";
  var N = window.StickyNote;
  var app = document.getElementById("app"), dots = document.getElementById("dots"), count = document.getElementById("count"), add = document.getElementById("add");
  var colors = document.getElementById("colors"), page = document.getElementById("page"), status = document.getElementById("status");
  var copy = document.getElementById("copy"), remove = document.getElementById("remove");
  var index = { order: [], current: null, colors: {} };
  var texts = {};
  var timer = null, armed = null, canStore = false;

  function scheme(s) { document.documentElement.dataset.scheme = s; }
  function say(text) { status.textContent = text || ""; }
  function newId() { return Date.now().toString(36) + Math.floor(Math.random() * 1e6).toString(36); }
  function colorOf(id) { return N.isColor(index.colors[id]) ? index.colors[id] : "yellow"; }
  function keyOf(id) { return "note." + id; }

  // The strip and the toolbar are drawn from the index; the page from the current note.
  function render() {
    dots.textContent = "";
    index.order.forEach(function (id, i) {
      var d = document.createElement("button");
      d.type = "button"; d.className = "dot"; d.setAttribute("role", "tab");
      d.setAttribute("aria-selected", id === index.current ? "true" : "false");
      d.setAttribute("aria-label", "Note " + (i + 1) + " of " + index.order.length);
      d.title = "Note " + (i + 1);
      d.addEventListener("click", function () { show(id); });
      dots.appendChild(d);
    });
    count.textContent = index.order.length > 1 ? (index.order.indexOf(index.current) + 1) + "/" + index.order.length : "";
    var color = colorOf(index.current);
    app.dataset.color = color;
    colors.querySelectorAll(".swatch").forEach(function (s) { s.setAttribute("aria-checked", s.dataset.color === color ? "true" : "false"); });
    remove.disabled = index.order.length === 0;
  }
  function show(id) {
    flush();
    index.current = id;
    page.innerHTML = N.sanitize(texts[id] || "");
    render();
    keepIndex();
  }
  N.COLORS.forEach(function (c) {
    var s = document.createElement("button");
    s.type = "button"; s.className = "swatch"; s.dataset.color = c;
    s.setAttribute("role", "radio"); s.setAttribute("aria-label", c); s.title = c;
    s.addEventListener("click", function () { index.colors[index.current] = c; render(); keepIndex(); });
    colors.appendChild(s);
  });

  // --- the store: one key per note, and the index -------------------------
  function keepIndex() {
    if (!canStore) return;
    bisa.storage.set("notes", JSON.stringify(index)).catch(function (e) { say(e.message); });
  }
  function save() {
    timer = null;
    var id = index.current;
    if (!id) return;
    var html = N.sanitize(page.innerHTML);
    if (!N.fits(html)) { say("the note is full"); return; }
    texts[id] = html;
    if (!canStore) { say("not kept: storage not granted"); return; }
    bisa.storage.set(keyOf(id), html).then(function () { say("kept"); }).catch(function (e) { say(e.code === "quota" ? "the pad is full — delete a note" : e.message); });
  }
  function flush() { if (timer) { clearTimeout(timer); save(); } }
  page.addEventListener("input", function () {
    // Past the cap, the last keystrokes come back out — the note stays whole.
    if (!N.fits(page.innerHTML)) { say("the note is full"); page.innerHTML = N.sanitize(texts[index.current] || ""); placeCaretAtEnd(); return; }
    say("…");
    if (timer) clearTimeout(timer);
    timer = setTimeout(save, 400);
  });
  page.addEventListener("blur", flush);
  function placeCaretAtEnd() {
    var range = document.createRange(), sel = window.getSelection();
    range.selectNodeContents(page); range.collapse(false);
    sel.removeAllRanges(); sel.addRange(range);
  }
  // Pasted HTML enters as text: formatting is the toolbar's to give.
  page.addEventListener("paste", function (e) {
    e.preventDefault();
    var text = (e.clipboardData || window.clipboardData).getData("text/plain");
    document.execCommand("insertText", false, text);
  });

  // --- formatting: the toolbar and ⌘B / ⌘I / ⌘U --------------------------
  document.querySelectorAll(".tools button[data-cmd]").forEach(function (b) {
    b.addEventListener("mousedown", function (e) { e.preventDefault(); });
    b.addEventListener("click", function () { page.focus(); document.execCommand(b.dataset.cmd, false, null); page.dispatchEvent(new Event("input")); });
  });
  page.addEventListener("keydown", function (e) {
    if (!(e.metaKey || e.ctrlKey)) return;
    var cmd = { b: "bold", i: "italic", u: "underline" }[e.key.toLowerCase()];
    if (!cmd) return;
    e.preventDefault();
    document.execCommand(cmd, false, null);
    page.dispatchEvent(new Event("input"));
  });

  // --- notes come and go ---------------------------------------------------
  function create() {
    flush();
    var id = newId();
    index.order.push(id);
    index.colors[id] = colorOf(index.current);
    texts[id] = "";
    show(id);
    page.focus();
  }
  add.addEventListener("click", create);
  function disarm() {
    if (armed) clearTimeout(armed);
    armed = null;
    remove.textContent = "Delete";
    remove.classList.remove("armed");
  }
  // The first press asks, the second deletes; two seconds and the question is withdrawn.
  remove.addEventListener("click", function () {
    if (!index.current) return;
    if (!armed) {
      remove.textContent = "Sure?";
      remove.classList.add("armed");
      armed = setTimeout(disarm, 2000);
      return;
    }
    disarm();
    var id = index.current;
    if (timer) { clearTimeout(timer); timer = null; }
    var at = index.order.indexOf(id);
    index.order.splice(at, 1);
    delete index.colors[id];
    delete texts[id];
    if (canStore) bisa.storage.remove(keyOf(id)).catch(function () {});
    if (index.order.length === 0) create();
    else show(index.order[Math.min(at, index.order.length - 1)]);
    say("deleted");
  });

  copy.addEventListener("click", function () {
    if (!bisa.granted("clipboard_write")) { say("copying needs the clipboard permission"); return; }
    bisa.clipboard.write({ text: N.plainText(page.innerHTML) }).then(function () { say("copied"); }).catch(function (e) { say(e.message); });
  });

  // --- start: the index, then every note it names ------------------------
  function readIndex(raw) {
    try {
      var i = JSON.parse(raw);
      if (i && Array.isArray(i.order)) {
        index.order = i.order.filter(function (id) { return typeof id === "string" && id.length > 0 && id.length <= 32; });
        index.current = index.order.indexOf(i.current) !== -1 ? i.current : index.order[0] || null;
        if (i.colors && typeof i.colors === "object") index.order.forEach(function (id) { if (N.isColor(i.colors[id])) index.colors[id] = i.colors[id]; });
      }
    } catch (_e) { /* a fresh pad */ }
  }
  bisa.ready().then(function (hello) {
    scheme(hello.theme.scheme);
    bisa.on("theme", function (t) { scheme(t.scheme); });
    bisa.on("visibility", function (v) { if (!v.visible) flush(); });
    canStore = bisa.granted("storage");
    if (!canStore) { say("not kept: storage not granted"); create(); return; }
    bisa.storage.get("notes").then(function (r) {
      if (r && r.value) readIndex(r.value);
      return Promise.all(index.order.map(function (id) {
        return bisa.storage.get(keyOf(id)).then(function (n) { texts[id] = typeof n.value === "string" ? N.sanitize(n.value) : ""; }).catch(function () { texts[id] = ""; });
      }));
    }).catch(function () {}).then(function () {
      if (index.order.length === 0) create(); else show(index.current);
    });
  });
})();
