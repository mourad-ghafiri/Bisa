// Everything an addon can do starts with `bisa.ready()`: what was granted,
// the person's language, the theme. Then it listens, asks, and shows.
bisa.ready().then(function (hello) {
  var greeting = document.getElementById("greeting");
  var colour = document.getElementById("colour");
  var note = document.getElementById("note");
  function scheme(s) { document.documentElement.dataset.scheme = s; }
  scheme(hello.theme.scheme);
  bisa.on("theme", function (t) { scheme(t.scheme); });
  greeting.textContent = "Hello from " + hello.addon.name + " — the app speaks " + hello.locale + ".";
  if (!bisa.granted("storage")) {
    note.textContent = "Grant storage and the colour is remembered.";
    return;
  }
  bisa.storage.get("colour").then(function (r) {
    if (r.value) { colour.value = r.value; greeting.style.color = r.value; }
  });
  colour.addEventListener("input", function () {
    greeting.style.color = colour.value;
    bisa.storage.set("colour", colour.value).then(function () { note.textContent = "kept"; }).catch(function (e) { note.textContent = e.message; });
  });
});
