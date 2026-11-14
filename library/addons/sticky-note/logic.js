// The note's rules, pure: the papers, the cap, and a whitelist sanitiser
// that keeps basic formatting and nothing else. Loaded before `main.js`;
// exposed as one global so a Node test can run this file alone.
(function (root) {
  "use strict";
  /** The six papers; each has a dark twin in the stylesheet. */
  var COLORS = ["yellow", "rose", "mint", "sky", "lilac", "stone"];
  /** One note's HTML, at most, in UTF-8 bytes — well under the platform's 16 KiB per call. */
  var MAX_HTML_BYTES = 12000;
  /** The tags a note may hold; `strong`/`em` fold into `b`/`i`. Nothing carries an attribute. */
  var ALLOWED = { b: "b", strong: "b", i: "i", em: "i", u: "u", s: "s", strike: "s", ul: "ul", ol: "ol", li: "li", p: "p", div: "div", br: "br" };
  var VOID = { br: true };
  /** Tags whose whole content goes with them. */
  var DROP_WITH_CONTENT = { script: true, style: true, template: true, iframe: true, object: true, embed: true, svg: true, math: true };

  function byteLength(s) {
    if (typeof TextEncoder !== "undefined") return new TextEncoder().encode(s).length;
    var n = 0;
    for (var i = 0; i < s.length; i++) { var c = s.charCodeAt(i); n += c < 0x80 ? 1 : c < 0x800 ? 2 : c >= 0xd800 && c <= 0xdbff ? (i++, 4) : 3; }
    return n;
  }
  function escapeText(t) { return t.replace(/&(?![a-zA-Z]+;|#\d+;|#x[0-9a-fA-F]+;)/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;"); }

  /**
   * HTML in, safe HTML out: only the allowed tags, no attributes, every
   * open tag closed, every stray close dropped, scripts and their bodies
   * gone, text escaped where it needs to be.
   */
  function sanitize(html) {
    if (typeof html !== "string") return "";
    var out = [], stack = [], skip = null;
    var re = /<!--[\s\S]*?-->|<\/?([a-zA-Z][a-zA-Z0-9-]*)(?:\s[^>]*)?\/?>|[^<]+|</g;
    var m;
    while ((m = re.exec(html)) !== null) {
      var token = m[0];
      if (token.charAt(0) === "<" && token.charAt(1) === "!") continue;
      var name = m[1] ? m[1].toLowerCase() : null;
      if (name === null) { if (!skip) out.push(escapeText(token)); continue; }
      var closing = token.charAt(1) === "/";
      if (skip) { if (closing && name === skip) skip = null; continue; }
      if (DROP_WITH_CONTENT[name]) { if (!closing) skip = name; continue; }
      var tag = ALLOWED[name];
      if (!tag) continue;
      if (VOID[tag]) { if (!closing) out.push("<br>"); continue; }
      if (closing) {
        var at = stack.lastIndexOf(tag);
        if (at === -1) continue;
        while (stack.length > at) out.push("</" + stack.pop() + ">");
      } else { stack.push(tag); out.push("<" + tag + ">"); }
    }
    while (stack.length) out.push("</" + stack.pop() + ">");
    return out.join("");
  }
  /** The text a note reads as, for the clipboard: tags gone, lines kept. */
  function plainText(html) {
    return sanitize(html)
      .replace(/<br>/g, "\n").replace(/<\/(p|div|li)>/g, "\n").replace(/<li>/g, "• ").replace(/<[^>]+>/g, "")
      .replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&nbsp;/g, " ").replace(/&amp;/g, "&")
      .replace(/\n{3,}/g, "\n\n").trim();
  }
  function fits(html) { return byteLength(html) <= MAX_HTML_BYTES; }
  function isColor(c) { return COLORS.indexOf(c) !== -1; }

  // --- the pad: an index of notes, each under its own key -------------------
  /** The key a note is kept under, and the longest id one may have. */
  var NOTE_KEY = "note.";
  var MAX_ID_CHARS = 32;
  function isId(id) { return typeof id === "string" && id.length > 0 && id.length <= MAX_ID_CHARS; }
  function emptyIndex() { return { order: [], current: null, colors: {} }; }

  /** The stored index as read — or `null` when it is no index at all (absent, cut short, another shape). */
  function readIndex(raw) {
    var i;
    try { i = JSON.parse(raw); } catch (_e) { return null; }
    if (!i || typeof i !== "object" || !Array.isArray(i.order)) return null;
    var index = emptyIndex();
    index.order = i.order.filter(isId);
    index.current = index.order.indexOf(i.current) !== -1 ? i.current : index.order[0] || null;
    if (i.colors && typeof i.colors === "object") index.order.forEach(function (id) { if (isColor(i.colors[id])) index.colors[id] = i.colors[id]; });
    return index;
  }
  /** The ids of the notes the store holds, from its keys, in a steady order. */
  function noteIds(keys) {
    var out = [];
    (Array.isArray(keys) ? keys : []).forEach(function (k) {
      if (typeof k !== "string" || k.indexOf(NOTE_KEY) !== 0) return;
      var id = k.slice(NOTE_KEY.length);
      if (isId(id) && out.indexOf(id) === -1) out.push(id);
    });
    return out.sort();
  }
  /**
   * How the pad starts, from what the store answered — the one rule that
   * keeps a person's notes:
   *
   * - **not read** (the store did not answer): nothing is known, so nothing
   *   may be written. Writing a fresh index here replaced the stored one and
   *   left every note it named unreachable.
   * - **read**: the index as stored; one that is absent or is no index is
   *   rebuilt from the notes themselves. A note the index does not name —
   *   left behind by that old overwrite — comes back at the end (`found`).
   */
  function startPad(answer) {
    if (!answer || !answer.read) return { index: emptyIndex(), mayWrite: false, found: 0 };
    var index = readIndex(answer.raw) || emptyIndex();
    var found = 0;
    noteIds(answer.keys).forEach(function (id) {
      if (index.order.indexOf(id) !== -1) return;
      index.order.push(id);
      found += 1;
    });
    if (index.current === null) index.current = index.order[0] || null;
    return { index: index, mayWrite: true, found: found };
  }

  root.StickyNote = { NOTE_KEY: NOTE_KEY, readIndex: readIndex, noteIds: noteIds, startPad: startPad, COLORS: COLORS, MAX_HTML_BYTES: MAX_HTML_BYTES, sanitize: sanitize, plainText: plainText, fits: fits, byteLength: byteLength, isColor: isColor };
})(this);
