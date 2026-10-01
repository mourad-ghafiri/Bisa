# Addons

An addon is a folder of HTML, CSS and JavaScript with an `addon.json` beside it, installed into a
workspace and shown as a window that floats over the desktop — a clock, the machine's load, the
weather, a game. It reaches the platform only through one door, the bridge, with only what the person
granted. In code: the manifest's rules are `bisa-core`'s, the install and the thirteen built-ins
`bisa-store`'s, the token-less files route and its policy the node's, the network broker the
engine's, the frame, the bridge and the windows the desktop's, the navigation wall the Tauri shell's,
and the library an addon loads lives in `addons/sdk/`.

## Where it lives

- `crates/bisa-core/src/addon.rs` — the manifest, the window, the permissions, the record, and every pure rule (`validate`, the bundle listing, the served types).
- `crates/bisa-store/src/addons.rs` · `crates/bisa-store/build.rs` — a folder judged whole and then copied, the record and its snapshot (kind 33407); the built-ins embedded from `library/addons/`.
- `crates/bisa-engine/src/addons.rs` — the doors that announce `AddonsChanged`, and the network broker.
- `crates/bisa-node/src/addons.rs` · `crates/bisa-node/src/auth.rs` — the routes, the one files route that answers without the token, its Content-Security-Policy, the library served from the binary.
- `crates/bisa-cli/src/addons.rs` — `bisa addon`.
- `desktop/src/addons/` — the bridge (`addonBridge.ts`) and its registry (`addonBridgeModel.mjs`), the windows, the layer, the store.
- `desktop/src-tauri/src/navigation.rs` — the shell's wall: a frame navigates nowhere but its own files.
- `addons/sdk/` (`bisa-addon.js`, `bisa-addon.d.ts`) · `addons/template/` — the library and its types; the starter a developer copies.
- `library/addons/` — the thirteen built-ins.

## Read first

- [18 — Addons](../../architecture/18-addons.md) — the words, the three walls, the bridge, what a person decides, what travels, where each piece lives.
- [Addons](../../guide/addons.md) — using one, and [writing one](../../guide/addons.md#writing-an-addon): the folder, the manifest, the rules a bundle keeps.
- [The addon API](../../reference/addon-api.md) — every method, the permission it needs, the events, the refusals.
- [Add a built-in addon](../recipes.md#27-add-a-built-in-addon).

## Rules a change must keep

- Three walls, none depending on another: the frame's sandbox (`allow-scripts` and nothing more), the node's policy on every file (it may reach nothing), the shell's navigation wall ([18 § Three walls](../../architecture/18-addons.md#three-walls-none-depending-on-another)).
- The bridge is a closed registry: a method not in it is refused before its params are read, every method names its permission, every field has a cap, and nothing an addon says is rendered as HTML ([18 § The door: the bridge](../../architecture/18-addons.md#the-door-the-bridge)).
- The registry and [the addon API](../../reference/addon-api.md) page are one list: a method in one and not the other fails `desktop/src/scenarios/addons.test.mjs`.
- No token, path, program, secret, message body or file has a method ([18 § The door: the bridge](../../architecture/18-addons.md#the-door-the-bridge)).
- The network leaves only through the engine's broker: `https`, never this machine, the manifest's declared hosts, the person's lists, `GET` only — every refusal read before a socket opens.
- A folder is judged whole before a byte is copied; nothing is granted to it by default; a grant is exactly what the manifest declared, and one it never declared is refused by word ([18 § What a person decides, and where](../../architecture/18-addons.md#what-a-person-decides-and-where)).
- A built-in's `id` is its slug and it wears the platform's version; nothing remote, no `fetch`, `localStorage` or `eval`; its stylesheet fences `[hidden]`, and its pure rules sit in a `logic.js` a Node test runs alone ([Add a built-in addon](../recipes.md#27-add-a-built-in-addon), [guide § Writing an addon](../../guide/addons.md#writing-an-addon)).

## Testing a change

- `scripts/test module store addons` · `scripts/test module engine addons` · `scripts/test module node addons` — an install and its refusals, the broker refusing before a socket opens, the routes and the token-less files route.
- `scripts/test lib core addon` — the manifest's rules; `scripts/test lib store catalog` — `the_catalog_ships_thirteen_addons` and `every_addon_is_well_formed`.
- `scripts/test desktop addons`, then from `desktop/`: `node --test --import ./src/i18n/preload.mjs src/scenarios/addons.test.mjs src/scenarios/addonsLogic.test.mjs` — the bridge and the SDK held to the registry, the built-ins' logic.
- `scripts/test tauri` — the navigation wall's tests in `desktop/src-tauri/src/navigation.rs`.
- The journey `crates/bisa-cli/tests/it/e2e/addons_drawings_and_notes.rs` — a folder refused whole, imported, granted, switched, its files served to its window and to nobody once it is off — under `scripts/test module cli e2e`.
- One module at a time; `npm test`, `cargo test --workspace` and `just verify` at the end ([Testing rules § Running](../testing-rules.md#running)).

## Common changes

- [Add a built-in addon](../recipes.md#27-add-a-built-in-addon), then `just gen-catalog-docs`.
- A bridge method: its `METHODS` entry and params in `addonBridgeModel.mjs`, its arm in `addonBridge.ts`, its name and types in the SDK, its row in the API page ([crates/desktop § Extension points](../../architecture/crates/desktop.md#extension-points)).
- A refusal's sentence: [Say something to a person](../recipes.md#26-say-something-to-a-person).

## Compatibility

- Every method, permission and event of the bridge, and every field of the manifest, are public contract ([The addon API](../../reference/compatibility.md#the-addon-api)). A change grows by a new method, permission or event; removing or renaming one, or widening what an existing permission grants, waits for 1.0.0 ([Keeping compatibility](../compatibility.md)).
- The addon's record is kind 33407 on [the collaboration wire](../../reference/compatibility.md#the-collaboration-wire).
- The built-ins are catalog content: they change with a release and wear its version.
- Declare your change's compatibility in the pull request.

## Review focus

- Does the change widen what an addon can reach — a method, a field without a cap, a route without the token, a host? Each wall must hold alone ([Security review](../review/security.md)).
- The registry, the SDK's types and the API page agree, and a refusal is a code with a sentence in the person's language ([Code review](../review/code.md)).
- A built-in's files, fonts and images are ours or carry their licence ([Licences review](../review/licences.md)).
- The bridge only grows ([Compatibility review](../review/compatibility.md)); the word is *addon*, never another name for it ([Docs and language](../review/docs-and-language.md)).
