# The addon API — `window.bisa`

What an addon may ask the platform, through the library the node serves at every addon's root
(`<script src="bisa-addon.js"></script>`; `addons/sdk/bisa-addon.d.ts` for your editor). Every
method is a promise; a refusal rejects with an `AddonError` whose `code` is one of the codes below
and whose `message` is a sentence in the person's language. The registry is
`desktop/src/addons/addonBridgeModel.mjs`, and `desktop/src/scenarios/addons.test.mjs` holds this
page to it — a method here and not there, or there and not here, fails. The model and the walls are
[18 — Addons](../architecture/18-addons.md).

## Before anything

`bisa.ready()` resolves once the platform said it is ready, with what it said: `{addon: {id, name,
version}, granted: [word…], locale, theme: {scheme}, window: {width, height, resizable, closable,
transparent, frame}}`. Every call waits for this on its own; `ready()` never rejects. A page that
says no hello within eight seconds of loading is told so by its window — *has not started*, with
Retry — and the log. `bisa.granted(word)` answers whether
the person granted a permission; ask before you ask. `frame` is `bar`, `none` or `scene` — a scene
is no window but the whole desktop under the chrome, see-through, never dragged or resized; it
hears the `scene` event and owns the three `scene.*` methods.

## Methods

| Method | Needs | Params | Answers |
|---|---|---|---|
| `platform.info` | `platform_info` | — | `{version, locale}` |
| `theme.get` | `theme` | — | `{scheme}` — `light` or `dark` |
| `system.load.subscribe` | `system_load` | — | `{subscribed: true}`; from then on the `system.load` event at the footer's cadence |
| `system.load.unsubscribe` | `system_load` | — | `{subscribed: false}` |
| `workspace.summary` | `workspace_summary` | — | `{waiting, review, working}` |
| `notify.show` | `notify` | `{title ≤ 100, body? ≤ 200}` | `{shown: true}`; at most one every ten seconds, else `rate_limited` |
| `clipboard.write` | `clipboard_write` | `{text ≤ 64 KiB}` | `{written: true}` |
| `storage.get` | `storage` | `{key ≤ 128}` | `{value}` — a string, or `null` |
| `storage.set` | `storage` | `{key ≤ 128, value: string}` | `{stored: true}`; 256 KiB in all per addon, else `quota` |
| `storage.remove` | `storage` | `{key}` | `{removed: true}` |
| `storage.keys` | `storage` | — | `{keys: [...]}` |
| `network.fetch` | `network` | `{url: https://…, accept?}` | `{status, contentType, body ≤ 1 MiB, truncated}` — through the node's broker: a host the manifest names and the person's `security.net.*` lists allow, never this machine, `GET` only, no redirect followed |
| `url.open` | `open_url` | `{url: http(s)://…}` | `{opened: true}` when the person said yes; `refused` when they did not |
| `navigate` | `navigate` | `{route}` — one of `inbox` · `goals` · `pulse` · `workflows` · `projects` · `channels` · `messages` · `agents` · `teams` · `settings` | `{navigated: true}` |
| `window.resize` | the manifest's `resizable` | `{width, height}` — clamped to the manifest's bounds | `{asked: true}` |
| `window.close` | the manifest's `closable` | — | `{closed: true}`; the window is put away, the footer brings it back |
| `window.setTitle` | — | `{title ≤ 60}` | `{set: true}` |
| `media.embed` | `media`, declared with `youtube` | `{kind: "youtube", video, box: {x, y, width, height}}` — an eleven-character video id; the box from the window's top-left, in its pixels, no side under 200 | `{embedded: true}`; YouTube's own player appears over the box, beside the window, and says its state through the `media` event; one per window, a second replaces the first; `not_allowed` when the grant names no player |
| `media.box` | `media` | `{box}` | `{moved: true}` |
| `media.remove` | `media` | — | `{removed: true}`; the player goes |
| `scene.mode` | the manifest's `frame: "scene"` | `{mode}` — `work` or `hunt` | `{mode}`; the host decides and says the mode in the `scene` event: in `work` the app has the pointer and only the scene's solid regions take it, in `hunt` the scene has it everywhere under the chrome; `Esc` leaves a hunt |
| `scene.solid` | the manifest's `frame: "scene"` | `{rects: [{x, y, width, height, radius?}]}` — at most 8, from the scene's top-left, in its pixels, a corner of at most 64 | `{set: true}`; the regions the pointer reaches in `work` mode (the scene's own strip), cut around over a browser tab |
| `scene.release` | the manifest's `frame: "scene"` | — | `{released: true}`; the pointer left the solid regions, and the app has it again |

The sugar wears the same names: `bisa.storage.get(key)`, `bisa.notify.show({title, body})`,
`bisa.network.fetch(url)`, `bisa.window.close()`; `bisa.call(method, params)` is every one of them
by name, and `bisa.methods()` lists them.

The `media` permission is declared as `{media: {hosts: [...], youtube?: true}}`: the `https://`
hosts the page's own `<audio>` and `<video>` elements may load from (`*` for any — the node widens
that addon's `media-src` and nothing else of its walls), and whether YouTube's player may be shown.
The player is the platform's own view, not a frame of the page: it stands over the box the addon
names, above every overlay of the app, hides with the window, and is gone when the addon removes
it, when its page reloads, or when the window is put away. A file from the machine is the page's
own `<input type="file">` or a drop, played as a `blob:` URL; nothing of it crosses the bridge.

## Events

`bisa.on(topic, fn)` listens; the return value unsubscribes.

| Topic | Payload | When |
|---|---|---|
| `theme` | `{scheme}` | the theme moved between light and dark |
| `system.load` | `{cpu_percent, load, mem_used, mem_total, swap_used, swap_total, uptime_secs, gpu?, disk?}` — `disk` is `{used, total, mount, workspace_bytes}` or `null` | each sample, while subscribed |
| `workspace.summary` | `{waiting, review, working}` | the counts moved, when `workspace_summary` is granted |
| `visibility` | `{visible}` | the window came on screen or left it **while its page is mounted**: off macOS, under a browser tab the layer cannot cut around (on macOS a tab is cut around the window, which stays visible). Put away, the layer off and the switch off unmount the page, and *Show* is a fresh page — keep what you must in your store; a scene is always on screen while mounted |
| `media` | `{state, title, code?, words?}` — `state` one of `ready` · `unstarted` · `playing` · `paused` · `buffering` · `ended` · `cued` · `error` | the embedded player moved, or failed: `code` is YouTube's (or `unavailable` when no player can be shown here) and `words` say it |
| `scene` | `{mode, viewport: {width, height}, obstacles: [{x, y, width, height}], cuts}` — the mode the host decided, the scene's size, the browser tabs' boxes to keep off (native views that paint over the page), whether the host cuts around the solid regions | a scene's first word after ready, every mode change, and every move of the layout |

## Refusals

| Code | Means |
|---|---|
| `not_ready` | a call before the platform said it is ready — the library waits for you, so this is a wire-level fault |
| `unknown_method` | not a method the platform offers |
| `permission` | the person did not grant what the method needs |
| `not_allowed` | the manifest does not allow this window that move (`resizable`, `closable`), the `media` grant names no player (`youtube`), or a `scene.*` call came from a window that is not a scene |
| `bad_params` | not the shape the method takes — a route not in the list, a URL that is not `http(s)`, a value that is not a string, a video id of another shape, a player box with a side under 200 |
| `too_large` | past a cap: params over 16 KiB, a clipboard write over 64 KiB |
| `rate_limited` | a notice too soon after the last |
| `quota` | the addon's storage is full |
| `unavailable` | the platform did not answer in ten seconds — or never said it was ready: the library says hello again at growing waits (250 ms to 4 s) and, past the last with no answer, fails every call waiting and every call made until a ready arrives; `ready()` itself keeps waiting |
| `refused` | the node or the person said no — a host outside the grant, a link not opened |

## What never has a method

The token, a path, a program, a file, a message's body, an agent, a secret, the app's own storage,
another addon's storage, a direct network request. An addon that needs one of these needs to be
something other than an addon.
