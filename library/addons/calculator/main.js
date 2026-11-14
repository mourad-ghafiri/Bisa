// Four functions, a keyboard, and no surprises: every operation is applied
// left to right, as typed; `←` takes the last digit back, and `=` again
// repeats the last operation. The rules live in `logic.js`.
(function () {
  "use strict";
  var C = window.Calculator;
  var display = document.getElementById("display");
  var pending = document.getElementById("pending");
  var keys = document.getElementById("keys");
  var LAYOUT = ["C", "←", "%", "÷", "7", "8", "9", "×", "4", "5", "6", "−", "1", "2", "3", "+", "±", "0", ".", "="];
  var LABELS = { C: "Clear", "←": "Back", "%": "Per cent", "÷": "Divide", "×": "Multiply", "−": "Subtract", "+": "Add", "±": "Change sign", ".": "Point", "=": "Equals" };
  var state = C.fresh();

  function scheme(s) { document.documentElement.dataset.scheme = s; }
  // Before the app has said its scheme: the machine's own preference, so a dark app is not shown a light flash.
  scheme(window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");
  function render() {
    display.textContent = state.entry;
    // The whole entry is there for a pointer when a long result is cut.
    display.title = state.entry;
    pending.textContent = C.pendingWords(state);
  }
  function press(k) {
    state = C.press(state, k);
    render();
  }
  LAYOUT.forEach(function (k) {
    var b = document.createElement("button");
    b.type = "button";
    b.textContent = k;
    if (LABELS[k]) b.setAttribute("aria-label", LABELS[k]);
    if (C.OPS[k] || k === "=") b.className = "op";
    if (k === "C" || k === "←" || k === "%" || k === "±") b.className = "quiet";
    b.addEventListener("click", function () { press(k); });
    keys.appendChild(b);
  });
  document.addEventListener("keydown", function (e) {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    // Enter and Space on a focused key press that key: Enter is `=` only when no key has the keyboard.
    if ((e.key === "Enter" || e.key === " ") && e.target && e.target.closest && e.target.closest("button")) return;
    var k = C.keyFor(e.key);
    if (!k) return;
    e.preventDefault();
    press(k);
  });
  render();
  bisa.ready().then(function (hello) {
    scheme(hello.theme.scheme);
    bisa.on("theme", function (t) { scheme(t.scheme); });
  });
})();
