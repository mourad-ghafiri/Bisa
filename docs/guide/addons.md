# Addons

An **addon** is a small window that floats over the app — a clock, the machine's load, the weather,
a game, a sticky note — written as a folder of HTML, CSS and JavaScript. Thirteen ship with the
platform; anyone can write one. This page is for the person who installs them and for the developer
who writes one. The model and the walls are [18 — Addons](../architecture/18-addons.md); every method
the library offers is [the addon API](../reference/addon-api.md).

---

## Using addons

**Settings › Library › Addons** is where they live. **From the catalog** lists the built-ins not yet
installed — *Install* opens a review that names, in plain words, everything the addon asks for
(*Read the machine's load*, *Fetch from the internet, only these hosts: api.open-meteo.com…*), and
only then installs it: a built-in lands on, granted what it declared, because the platform wrote it
and you just read the list. **Installed** shows every addon here — running or off, built in or
imported — with its switch, one switch per permission it declares, and *Remove*.

**Import an addon** brings in one of your own, or somebody else's: the folder picker, then the
platform reads the folder and names every problem before a byte is copied (a missing page, a file
a bundle may not carry, a permission spelled wrong, a host that is this machine). A clean folder
opens the same review — this time **nothing is granted** until you tick it, and *Turn it on at
once* is yours to choose.

Every running addon floats over the app at the pet's height: under the notes panel, under every
dialog. Drag it by its bar (or by the grip that appears on hover when it has no bar); resize it from
the bottom-right corner when its developer allowed that; close it with its ✕ when they allowed
that — a closed window is *put away*, not off. Where a window stands is this machine's memory, like
the pet's.

**The footer's Addons popover** — the addon glyph at the right of the footer, tinted while any
window shows, its tooltip *2 of 5 addons showing* — is the short way to everything above without
leaving the screen you are on. At the top, the layer's switch (`⌘⇧X`): off, every window is hidden
at once and nothing is forgotten. Under it, **every installed addon** in one list, running first:
its name, its state (*running · put away · off · files not on this machine*), an **eye** that shows
or puts away its window while it runs, and its **on/off switch** — the same switch as in Settings,
answered at once and put back with a word if the node refuses. Then *Reset positions*, which puts
every window back where its manifest opens it, and a door to Settings › Library › Addons. Switching
an addon on
from here brings its window back if it was put away: a person who turns an addon on wants to see it.

**The switch on this machine** — *Run addons on this machine* at the top of the panel
(`addons.enabled`) — hides every window and serves nothing while off; what each addon is and what it
was granted is kept. The footer follows it the moment it moves.

### The built-ins

Thirteen ship in the binary, each at the platform's own version. When the platform you run ships
one you installed at a different version, the workspace takes the new files at its next open and
keeps your switch and your grants (a permission
the new version no longer asks for is dropped; one it newly asks for is not granted until you tick it).

- **Clock** — one face at a time: analog (a dial with numerals and a sweeping second hand — a still
  one under reduced motion) or digital (tabular figures, am/pm, the date). The gear at the top-right
  appears on hover and opens the settings — the face, 24-hour, seconds, the date — in a small card;
  a click outside or `Esc` closes it. The choices are the addon's own store.
- **Tic-tac-toe** — you against the machine (it plays O; *Easy* picks a free cell, *Unbeatable* never
  loses), who starts alternating game by game, or *Two players* at one board; the tally is kept per
  mode; *New game* starts over.
- **Sticky note** — as many notes as you like on one pad: a dot per note in the strip at the top,
  `+` for another, the count beside them; six papers (yellow, rose, mint, sky, lilac, stone) from the
  swatches; bold, italic, underline, bulleted and numbered lists and *clear formatting* from the
  toolbar that appears on hover (`⌘B`, `⌘I`, `⌘U` too); *Copy* puts the note's text on the clipboard;
  *Delete* asks once. A note holds about 12 000 characters and says *the note is full* at the cap;
  pasted text arrives as text. Each note is one key in the addon's store.
- **Weather** — a city you type, from Open-Meteo through the platform's broker, refreshed every
  fifteen minutes while shown; the place's name is a button that opens the form again.
- **Pomodoro** — focus and break, a ring that empties as the phase runs, a notice at each turn where
  you allowed one (and while *Addons* is on under Settings › Capabilities › System ›
  Notifications); the lengths
  behind a gear.
- **Stopwatch** — start, lap, reset; every lap numbered with its split and its total; *Reset* asks
  once when laps would go.
- **CPU, GPU, Memory, Disk** — one figure with its unit, a sparkline of the last minute with its
  last point marked, a bar; GPU says once when the machine has no reader.
- **Needs you** — how many things wait on you, are under review, or are running; the first two are
  doors to the Inbox and the Goals.
- **Calculator** — four functions applied left to right as typed, `←` takes the last digit back,
  `=` again repeats the last operation; the keyboard works.
- **Unit converter** — length, mass and temperature; the swap between the pair turns the conversion
  around.

### What an addon can never do

Whatever you grant, an addon runs in a sandboxed frame with an origin of its own: it cannot see the
app, its token, your files, your messages or your agents, cannot open a window or a form, cannot
navigate anywhere but its own files, and cannot reach the internet itself — a `network` grant lets
it ask the platform to fetch from the hosts its manifest names, `https` only, never this machine,
and your `security.net.*` lists still win. What it may do is exactly the list you granted, each
line of which is one thing.

---

## Writing an addon

A folder:

```
my-addon/
  addon.json      the manifest
  index.html      the page (any name, named by `entry`)
  style.css
  main.js
  img/…           images, fonts, sounds — any file of an allowed kind
```

`addons/template/` in the repository is a working starter. The manifest:

```json
{
  "id": "acme.weather-pro",
  "name": "Weather Pro",
  "description": "What a person reads before installing it.",
  "version": "1.0.0",
  "author": "Acme",
  "homepage": "https://example.com/weather-pro",
  "repo": "https://github.com/acme/weather-pro",
  "license": "MIT",
  "tags": ["weather"],
  "window": {
    "width": 240, "height": 150,
    "min_width": 200, "min_height": 130, "max_width": 400, "max_height": 260,
    "resizable": true, "closable": true, "transparent": false,
    "frame": "bar", "default_dock": "bottom_right"
  },
  "permissions": [{ "network": { "hosts": ["api.open-meteo.com"] } }, "storage"],
  "entry": "index.html"
}
```

- **`id`** — lowercase letters, digits, `-`, `_`, `.`, `:`; up to 64. Namespace it (`acme.weather-pro`):
  a built-in's id is its catalog slug, and a folder claiming one is refused.
- **`version`** — `MAJOR.MINOR.PATCH`, with an optional `-pre` and `+build`.
- **`homepage`, `repo`** — `https://` only; shown, never fetched or cloned.
- **`window`** — every field but `width` and `height` may be left out; edges are 40 to 1600 px;
  `frame` is `bar` (a slim bar with the name, the grip and the ✕) or `none` (a grip on hover);
  `transparent` paints nothing behind your page, so your own `background: transparent` shows the
  app through it; `default_dock` is the corner it opens in.
- **`permissions`** — from the closed list: `platform_info`, `theme`, `system_load`,
  `workspace_summary`, `notify`, `clipboard_write`, `storage`, `network` (with `hosts`: `host[:port]`
  or `*.suffix`, never this machine), `open_url`, `navigate` (to one of the app's screens, which
  opens as the person left it — and opens nothing of its own: no browser tab). Declare what you need and no more: the
  person sees the list before installing and grants each line on its own.

The page loads the library and your script:

```html
<script src="bisa-addon.js"></script>
<script src="main.js"></script>
```

`bisa-addon.js` is served by the platform at your addon's root — do not ship a copy (a bundle that
carries one is refused). Then:

```js
bisa.ready().then(function (hello) {
  document.documentElement.dataset.scheme = hello.theme.scheme;
  bisa.on("theme", (t) => (document.documentElement.dataset.scheme = t.scheme));
  if (bisa.granted("storage")) bisa.storage.get("city").then(({ value }) => …);
  bisa.on("visibility", ({ visible }) => { /* stop a clock nobody sees */ });
});
```

Every method is a promise that rejects with an `AddonError` — its `code` says why (`permission`,
`bad_params`, `rate_limited`, `quota`…) and its `message` is a sentence in the person's language, so
show it. Ask `bisa.granted(word)` before you ask for something, and say what the person would gain
by granting it.

**The rules a bundle keeps** (the platform names each one before a byte is copied): every file's
extension is one of `html css js mjs json svg png jpg jpeg gif webp ico woff woff2 ttf mp3 ogg wav
mp4 webm txt md`; no symlinks, no dotfiles, at most eight folders deep, 256 files, 2 MiB a file,
8 MiB in all; the entry page exists; no file is named `bisa-addon.js`, and no `addon.json` sits
among the files. Your page may load its own files and inline what it carries; it may not load a
remote script, fetch, open a frame or post a form — the platform's policy on every file says so, and
the frame's sandbox says it again.

**Testing locally.** `bisa addon import ./my-addon --grant storage --enable` (or Settings ›
Library › Addons › *Import an addon*), then open the app: the window appears; the desktop's
diagnostic log (Settings › Node › Logging) records every refusal the bridge made — *a call was
refused*, with the addon, the method and the code. Edit, `bisa addon
remove acme.weather-pro`, import again — an installed bundle is a copy, never a link to your folder.

**Theme.** The `theme` permission is only for hearing changes; `hello.theme.scheme` comes with
every ready. Follow it with a `[data-scheme="dark"]` block, and keep `prefers-reduced-motion` in
mind — a clock's second hand stops sweeping under it. A page whose window is `transparent` should
not declare `color-scheme: light dark`: the platform's frame is `color-scheme: normal`, and a page
that disagrees can paint an opaque backdrop behind it.

**Hiding things.** An author `display` beats the browser's own `[hidden] { display: none }` — a
`<form hidden>` styled `display: flex` is shown regardless — so every page fences the attribute
first: `[hidden] { display: none !important; }`. An `<svg>` element has no `hidden` property at
all; switch faces or shapes with a `data-` attribute on a parent and a CSS rule, never `svg.hidden`.
The built-ins carry both rules and a test holds them.

**The testable half.** Rules with no DOM — a game's winner, a sanitiser, a conversion — belong in a
`logic.js` your page loads before `main.js`, written as `(function (root) { … root.MyAddon = api;
})(this)`: the page reads the global, and a Node test runs the file alone in a `vm`
(`desktop/src/scenarios/addonsLogic.test.mjs` does this for tic-tac-toe and the sticky note).
