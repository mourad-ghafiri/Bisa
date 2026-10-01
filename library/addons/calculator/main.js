// Four functions, a keyboard, and no surprises: every operation is applied
// left to right, as typed; `←` takes the last digit back, and `=` again
// repeats the last operation.
(function () {
  "use strict";
  var display = document.getElementById("display");
  var pending = document.getElementById("pending");
  var keys = document.getElementById("keys");
  var LAYOUT = ["C", "←", "%", "÷", "7", "8", "9", "×", "4", "5", "6", "−", "1", "2", "3", "+", "±", "0", ".", "="];
  var OPS = { "÷": "/", "×": "*", "−": "-", "+": "+" };
  var SIGNS = { "/": "÷", "*": "×", "-": "−", "+": "+" };
  var LABELS = { C: "Clear", "←": "Back", "%": "Per cent", "÷": "Divide", "×": "Multiply", "−": "Subtract", "+": "Add", "±": "Change sign", ".": "Point", "=": "Equals" };
  var state = fresh();

  function fresh() { return { entry: "0", acc: null, op: null, fresh: true, last: null }; }
  function scheme(s) { document.documentElement.dataset.scheme = s; }
  function fmt(n) {
    if (!isFinite(n)) return "Error";
    var s = String(Number(n.toPrecision(12)));
    return s.length > 14 ? n.toExponential(6) : s;
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
  function press(k) {
    if (/^\d$/.test(k)) {
      state.entry = state.fresh || state.entry === "0" ? k : state.entry + k;
      state.fresh = false;
    } else if (k === ".") {
      if (state.fresh) { state.entry = "0."; state.fresh = false; }
      else if (state.entry.indexOf(".") === -1) state.entry += ".";
    } else if (k === "C") {
      state = fresh();
    } else if (k === "←") {
      if (state.fresh) { state.entry = "0"; }
      else {
        state.entry = state.entry.length > 1 ? state.entry.slice(0, -1) : "0";
        if (state.entry === "-" || state.entry === "-0") state.entry = "0";
      }
    } else if (k === "±") {
      if (state.entry !== "0") state.entry = state.entry.charAt(0) === "-" ? state.entry.slice(1) : "-" + state.entry;
    } else if (k === "%") {
      state.entry = fmt(parseFloat(state.entry) / 100);
    } else if (OPS[k]) {
      var v = parseFloat(state.entry);
      // An operator after an operator changes it; otherwise the pending
      // operation is applied first, left to right.
      if (state.acc === null) state.acc = v;
      else if (!state.fresh) state.acc = apply(state.acc, state.op, v);
      state.op = OPS[k];
      state.fresh = true;
      state.last = null;
      state.entry = fmt(state.acc);
    } else if (k === "=") {
      if (state.op !== null) {
        var b = parseFloat(state.entry);
        var r = apply(state.acc, state.op, b);
        state = { entry: fmt(r), acc: null, op: null, fresh: true, last: { op: state.op, b: b } };
      } else if (state.last) {
        var again = apply(parseFloat(state.entry), state.last.op, state.last.b);
        state.entry = fmt(again);
        state.fresh = true;
      }
    }
    display.textContent = state.entry;
    pending.textContent = state.op !== null ? fmt(state.acc) + " " + SIGNS[state.op] : state.last ? SIGNS[state.last.op] + " " + fmt(state.last.b) + " again" : "";
  }
  LAYOUT.forEach(function (k) {
    var b = document.createElement("button");
    b.type = "button";
    b.textContent = k;
    if (LABELS[k]) b.setAttribute("aria-label", LABELS[k]);
    if (OPS[k] || k === "=") b.className = "op";
    if (k === "C" || k === "←" || k === "%" || k === "±") b.className = "quiet";
    b.addEventListener("click", function () { press(k); });
    keys.appendChild(b);
  });
  document.addEventListener("keydown", function (e) {
    var map = { "/": "÷", "*": "×", "-": "−", "+": "+", Enter: "=", "=": "=", Escape: "C", Delete: "C", Backspace: "←", "%": "%" };
    var k = /^\d$/.test(e.key) || e.key === "." ? e.key : map[e.key];
    if (!k) return;
    e.preventDefault();
    press(k);
  });
  bisa.ready().then(function (hello) {
    scheme(hello.theme.scheme);
    bisa.on("theme", function (t) { scheme(t.scheme); });
  });
})();
