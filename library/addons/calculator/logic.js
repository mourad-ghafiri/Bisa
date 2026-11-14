// The calculator's rules, pure: a state and a key in, a new state out —
// every operation applied left to right, as typed. Loaded before `main.js`;
// exposed as one global so a Node test can run this file alone.
(function (root) {
  "use strict";
  var OPS = { "÷": "/", "×": "*", "−": "-", "+": "+" };
  var SIGNS = { "/": "÷", "*": "×", "-": "−", "+": "+" };
  /** The most digits an entry holds: past this the display would cut the digits just typed. */
  var MAX_DIGITS = 15;
  var KEYS = { "/": "÷", "*": "×", "-": "−", "+": "+", Enter: "=", "=": "=", Escape: "C", Delete: "C", Backspace: "←", "%": "%" };

  function fresh() { return { entry: "0", acc: null, op: null, fresh: true, last: null }; }
  function fmt(n) {
    if (!isFinite(n)) return "Error";
    var s = String(Number(n.toPrecision(12)));
    // An exponent with no tail of zeroes: 1e+15, never 1.000000e+15.
    return s.length > 14 ? n.toExponential(6).replace(/\.?0+e/, "e") : s;
  }
  function apply(a, op, b) {
    switch (op) {
      case "/": return b === 0 ? NaN : a / b;
      case "*": return a * b;
      case "-": return a - b;
      case "+": return a + b;
      default: return b;
    }
  }
  function digits(entry) { return entry.replace(/[^0-9]/g, "").length; }
  function copy(state, over) {
    var next = { entry: state.entry, acc: state.acc, op: state.op, fresh: state.fresh, last: state.last };
    for (var k in over) if (Object.prototype.hasOwnProperty.call(over, k)) next[k] = over[k];
    return next;
  }

  /** The state after a key. The state handed in is never changed. */
  function press(state, k) {
    if (/^\d$/.test(k)) {
      if (state.fresh || state.entry === "0") return copy(state, { entry: k, fresh: false });
      return digits(state.entry) >= MAX_DIGITS ? state : copy(state, { entry: state.entry + k });
    }
    if (k === ".") {
      if (state.fresh) return copy(state, { entry: "0.", fresh: false });
      return state.entry.indexOf(".") === -1 ? copy(state, { entry: state.entry + "." }) : state;
    }
    if (k === "C") return fresh();
    if (k === "←") {
      if (state.fresh) return copy(state, { entry: "0" });
      var cut = state.entry.length > 1 ? state.entry.slice(0, -1) : "0";
      return copy(state, { entry: cut === "-" || cut === "-0" ? "0" : cut });
    }
    if (k === "±") {
      // A sign is a number's: nothing, and an error, has none.
      if (state.entry === "0" || !isFinite(parseFloat(state.entry))) return state;
      return copy(state, { entry: state.entry.charAt(0) === "-" ? state.entry.slice(1) : "-" + state.entry });
    }
    if (k === "%") {
      // A result: what is typed next is a new number.
      return copy(state, { entry: fmt(parseFloat(state.entry) / 100), fresh: true });
    }
    if (OPS[k]) {
      var v = parseFloat(state.entry);
      // An operator after an operator changes it; otherwise the pending
      // operation is applied first, left to right.
      var acc = state.acc === null ? v : state.fresh ? state.acc : apply(state.acc, state.op, v);
      return { entry: fmt(acc), acc: acc, op: OPS[k], fresh: true, last: null };
    }
    if (k === "=") {
      if (state.op !== null) {
        var b = parseFloat(state.entry);
        return { entry: fmt(apply(state.acc, state.op, b)), acc: null, op: null, fresh: true, last: { op: state.op, b: b } };
      }
      if (state.last) return copy(state, { entry: fmt(apply(parseFloat(state.entry), state.last.op, state.last.b)), fresh: true });
    }
    return state;
  }
  /** The line above the display: what is pending, or what = would do again. */
  function pendingWords(state) {
    if (state.op !== null) return fmt(state.acc) + " " + SIGNS[state.op];
    return state.last ? SIGNS[state.last.op] + " " + fmt(state.last.b) + " again" : "";
  }
  /** The calculator key a keyboard key means, or `null`. */
  function keyFor(key) {
    if (/^\d$/.test(key) || key === ".") return key;
    return Object.prototype.hasOwnProperty.call(KEYS, key) ? KEYS[key] : null;
  }

  root.Calculator = { OPS: OPS, MAX_DIGITS: MAX_DIGITS, fresh: fresh, fmt: fmt, press: press, pendingWords: pendingWords, keyFor: keyFor };
})(this);
