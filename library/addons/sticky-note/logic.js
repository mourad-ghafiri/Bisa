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

  root.StickyNote = { COLORS: COLORS, MAX_HTML_BYTES: MAX_HTML_BYTES, sanitize: sanitize, plainText: plainText, fits: fits, byteLength: byteLength, isColor: isColor };
})(this);
