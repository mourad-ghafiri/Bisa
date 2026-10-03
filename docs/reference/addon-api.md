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
transparent, frame}}`. Every call waits for this on its own. `bisa.granted(word)` answers whether
the person granted a permission; ask before you ask.

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

The sugar wears the same names: `bisa.storage.get(key)`, `bisa.notify.show({title, body})`,
`bisa.network.fetch(url)`, `bisa.window.close()`; `bisa.call(method, params)` is every one of them
by name, and `bisa.methods()` lists them.

## Events

`bisa.on(topic, fn)` listens; the return value unsubscribes.

| Topic | Payload | When |
|---|---|---|
| `theme` | `{scheme}` | the theme moved between light and dark |
| `system.load` | `{cpu_percent, load, mem_used, mem_total, swap_used, swap_total, uptime_secs, gpu?, disk?}` — `disk` is `{used, total, mount, workspace_bytes}` or `null` | each sample, while subscribed |
| `workspace.summary` | `{waiting, review, working}` | the counts moved, when `workspace_summary` is granted |
| `visibility` | `{visible}` | the window came on screen or left it — put away, the layer off, or (off macOS) under a browser tab; on macOS a browser tab is cut around the window, which stays visible |

## Refusals

| Code | Means |
|---|---|
| `not_ready` | a call before the platform said it is ready — the library waits for you, so this is a wire-level fault |
| `unknown_method` | not a method the platform offers |
| `permission` | the person did not grant what the method needs |
| `not_allowed` | the manifest does not allow this window that move (`resizable`, `closable`) |
| `bad_params` | not the shape the method takes — a route not in the list, a URL that is not `http(s)`, a value that is not a string |
| `too_large` | past a cap: params over 16 KiB, a clipboard write over 64 KiB |
| `rate_limited` | a notice too soon after the last |
| `quota` | the addon's storage is full |
| `unavailable` | the platform did not answer in ten seconds |
| `refused` | the node or the person said no — a host outside the grant, a link not opened |

## What never has a method

The token, a path, a program, a file, a message's body, an agent, a secret, the app's own storage,
another addon's storage, a direct network request. An addon that needs one of these needs to be
something other than an addon.
