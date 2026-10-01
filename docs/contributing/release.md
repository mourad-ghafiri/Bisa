# Release

There is no release train; there is a gate a person runs before a commit, a release a person
builds and publishes with their own credentials, and nothing that publishes on its own.

## Versions and what each may change

Bisa follows Semantic Versioning, held to a stricter rule in the 0.x line
([Compatibility](../reference/compatibility.md)):

| Release | May | Never |
|---|---|---|
| **Patch** 0.y.**Z** | fix behaviour | change a shape on disk or on the wire; add a field, key, route, verb, kind or tool; change a default |
| **Minor** 0.**Y**.0 | add to the contract; deprecate; change a default (in the changelog's *Changed*) | rename, remove or retype anything of the contract |
| **Major** **1**.0.0 | change the contract incompatibly, with its migration ([Migrations](migrations.md)) | leave a person's workspace behind without a way across |

A release is a patch when *Unreleased* holds only *Fixed* and *Security* entries; a minor when it holds
*Added*, *Changed* or *Deprecated*. Nothing declared *breaks* is merged into 0.x
([Keeping compatibility](compatibility.md)), so a 0.x release never needs *Removed*.

## The gate

```sh
just verify   # build · test-rust · lint · check-types · check-api-docs · check-settings-docs · check-catalog-docs · check-keymap-docs · check-mermaid · licence-gate · check-notices · release-check · website-check · hakari-verify · desktop-coverage · desktop-build
```

`test-rust` runs the workspace's tests through `scripts/test rust` — under `cargo-nextest` when it is
installed in the tree (`just install-nextest`), under `cargo test` otherwise, `--workspace
--no-fail-fast` either way — and then `cargo test` in `desktop/src-tauri`; the desktop's
suite runs once, in `desktop-coverage`, which holds its models at 80 % of lines (`npm run
test:coverage`). `lint` runs `scripts/lint-terminology` first — it needs no toolchain, and a
reintroduced banned word fails in a second — with the lint's own fixture suite
(`scripts/lint-terminology-test`), then the desktop's `npm run lint` (`eslint .`), then
`clippy --all-targets -- -D warnings` and `fmt --check` for
the workspace and `desktop/src-tauri`. The six `check-*` targets before `licence-gate` fail when a generated file is stale
(`desktop/src/types.gen.ts` and `desktop/api-schema.json`, `docs/reference/http-api.md`,
`docs/reference/settings-keys.md`, `docs/reference/catalog.md`, `docs/reference/keymap.md`) or a
Mermaid block does not parse. `licence-gate` is `cargo deny` over `deny.toml` on both Cargo
workspaces plus the Node gate's own model tests and `scripts/licences/check-licences.mjs`
([§ Licence](#licence)); `check-notices` fails when `THIRD-PARTY-NOTICES.md` is stale;
`release-check` runs the release tooling's own model tests and parses every release script
([§ The version and the changelog](#the-version-and-the-changelog)); `website-check` holds the
website to what `scripts/website/build.mjs` builds from the repository, with its own tests
([the website's README](../../website/README.md)); `hakari-verify` proves the unification crate current, depended on by every member but `bisa-log`
and one feature set per dependency ([Testing rules § Faster builds](testing-rules.md#faster-builds)).

CI (`.github/workflows/verify.yml`) runs the same steps as eight jobs — `terminology` first (the
lint and its fixture suite), then `rust`, `desktop` (`npm run lint`, the coverage run, the build),
`types`, `licences` (the model tests, the gate, the notices, the release gate), `hakari` (the tool's three checks,
installed with `cargo install cargo-hakari --locked`), `benches` and `website` (node only) — with `RUSTFLAGS: -D warnings`
for every `cargo` step and `-D warnings` on clippy, so a rustc or clippy warning fails CI as it
fails `just lint`. The `types` job regenerates and diffs all four generated pages and the two type
files; the `benches` job compiles every bench and runs the graph bench at 20 000 commits, reporting
and never gating.

## Generated files travel with the change

A change to a route table, the settings registry, the keymap model or the catalog's TOML
regenerates its reference in the same commit: `just gen-types gen-api-docs gen-settings-docs
gen-catalog-docs gen-keymap-docs`. CI diffs them; a hand edit to a generated file is overwritten by
the next generation and refused by the check.

## Licence

The platform is **MIT** — `LICENSE`, `[workspace.package] license`, every crate, the desktop shell,
the addon SDK and the built-in addons. Nothing it depends on prevents that, and the audit that
established it (2026-09-26) is worth keeping in a sentence each: every GPL or LGPL term in either
Cargo graph is an *alternative* in an `OR` (`self_cell`, `r-efi`) and the permissive branch is
taken; the only copyleft actually linked is **MPL-2.0**, file-level, in two unmodified crates
(`attohttpc` in the `bisa` binary, `option-ext` in both), which an MIT combined work may carry as
long as those files stay MPL and recipients are told where their source is; every npm package the
desktop bundles is permissive or offers a permissive branch (`jszip` MIT, `dompurify` Apache);
the fonts are OFL-1.1, which binds the fonts and not the software; the only Creative Commons
material that ships is attribution-only, inside Monaco (the WHATWG DOM descriptions and the
Codicons). The platform is not, and will not be, under a CC licence: those appear only as
third-party attribution it reproduces.

**What the distribution owes, and where it is met.** Attribution for MIT, BSD, ISC, Zlib, Unicode
and Apache (with moka's NOTICE), the MPL source pointers, the CDLA text for the CA list, the OFL
texts for every bundled font, Monaco's notices and the CC-BY attributions — all in
`THIRD-PARTY-NOTICES.md`, generated by `just gen-notices` (`scripts/licences/`): `NOTICES.md`
verbatim (the hand-written part: the marks, the fonts, Monaco, the MPL files), then every Rust
crate the two binaries carry (the root workspace on every platform, the desktop shell on the macOS
targets, the platform shipped today) and every npm package in the desktop's runtime closure, by
licence, with copyright lines and source pointers, then one text per licence id and every NOTICE
file verbatim. `scripts/macos/lib.sh` puts `LICENSE` and the notices into
`Bisa.app/Contents/Resources` of every bundle it assembles, and About reveals both. `just check-notices` (and CI) fails when the
committed file is stale.

**The gate.** `deny.toml` is **the** allow list — permissive ids only, with a comment on the three
that deserve one (MPL-2.0, CDLA-Permissive-2.0, BSL-1.0); `desktop/src-tauri/deny.toml` is a copy a
test holds equal, since cargo-deny reads the file beside the manifest it checks. One list serves two
graphs, and each carries an id the other never meets (`0BSD` is the shell's, `CDLA-Permissive-2.0`
the node's), so an allowance a graph does not use is no fault of it
(`unused-allowed-license = "allow"`). `cargo deny check
licenses` runs on both workspaces in CI and locally when installed; `scripts/licences/check-licences.mjs`
judges the same graphs, and the npm runtime closure, against the same list on any machine — an
`OR` takes its first allowed alternative, an `AND` needs every term — and fails by name on a
package with no allowed branch, copyleft called copyleft. The build's npm tools do not ship and are
listed, never failed. **A new dependency whose licence is off the list is a decision, not a
warning**: read the licence, then either allow the id in both `deny.toml` files with a word on why,
or find another package — and regenerate the notices either way ([Recipes § Add a dependency](recipes.md#28-add-a-dependency)).

## One version

The version is the workspace's — `[workspace.package] version` in `Cargo.toml` — and everything
wears it: the 27 crates inherit it (with the edition and the licence — `bisa-deps`, whose table is generated, among them: `every_crate_wears_the_workspaces_version_edition_and_licence` in `crates/bisa-core/tests/it/layering.rs`), `desktop/package.json`, `desktop/src-tauri` and
`tauri.conf.json` repeat it, and **every built-in addon's manifest carries it** (a built-in ships
inside the binary, so its version is the platform's; a built-in changes with a release, and the store
re-copies an installed built-in's bundle when the shipped version differs from the installed one).
The same is true of the website (`homepage`) and the repository, declared once beside the version and
read by the Rust side through Cargo's `CARGO_PKG_HOMEPAGE` / `CARGO_PKG_REPOSITORY` and by the
desktop through `package.json`. Two guards hold all of it equal: `every_addon_is_well_formed`
(`crates/bisa-store/src/catalog.rs`) for the built-ins, and
`desktop/src/scenarios/platformIdentity.test.mjs` for every manifest and page. A release moves the
version in one command — [§ The version and the changelog](#the-version-and-the-changelog) — and
the guards say when a file was missed.

## The artefacts

`just dist` builds the node at its shipping profile for this machine (`cargo build --profile dist
-p bisa-cli --bin bisa`: `opt-level = 3` for throughput, not size, fat LTO, one codegen unit; it
unwinds — no `panic = "abort"` — so a panic in one request is a logged error, not a dead node). The
binary is named because the package holds a second one, `scripted-agent` — the harness the
journeys give the daemon to launch ([testing rules](testing-rules.md)) — which is the tests' own
and is never built for a release, nor copied into a bundle. On Linux and
Windows the desktop bundle is `just desktop-bundle` (`cargo tauri build`), unsigned, finding the node
on the `PATH` an application sees or under `BISA_BIN`. Nothing publishes on its own: the one script that reaches GitHub,
`scripts/publish-release.sh`, runs when a person runs it, as that person, and never by anything
else — not CI, not a hook, not another script.

## The macOS application: two builds, one library

Two scripts make a `Bisa.app`, and one library — `scripts/macos/lib.sh`, sourced by both and never
run — holds what they share: the node built for `aarch64-apple-darwin` and `x86_64-apple-darwin` at
the `dist` profile and joined with `lipo -create` (`lipo -archs` must say both slices; both targets
are in `rust-toolchain.toml`, so rustup adds a missing one on the next `cargo` call, and
`.config/hakari.toml` unifies both); the Tauri shell built with `tauri build --bundles app --target
universal-apple-darwin --no-sign` — the CLI's own signing skipped on purpose, since the node is
added *after* its build and one signature pass over the finished bundle is the only right order;
the bundle assembled with `ditto`, the node into `Contents/MacOS/bisa` where
`desktop/src-tauri/src/sidecar.rs` looks first, `LICENSE` and `THIRD-PARTY-NOTICES.md` into
`Contents/Resources/`; and the bundle signed **inside-out** — the node, then the app — with
`--options runtime` (the hardened runtime), the app with `--entitlements
desktop/src-tauri/Entitlements.plist` (one entitlement: the microphone, for voice mode — the file
says why the others are absent), never `--deep`, which Apple deprecates for signing; then
`codesign --verify --deep --strict`, and a refusal if what was signed carries
`com.apple.security.get-task-allow`, which the notary service refuses. One rule tells the two builds
apart: a secure timestamp (`--timestamp`) whenever the identity is not the ad hoc one — the notary
needs it, and an ad hoc signature cannot carry one. `tauri.conf.json` sets the identifier
`dev.bisa.bisa` — the app's folders under `~/Library` and its `bisa://` link claim are keyed by it
— and `bundle.macOS`: `minimumSystemVersion` 11.0, `hardenedRuntime`, the entitlements file; it
never names an identity. Nothing in either script deletes anything: a path about to be written again
is moved aside under `target/dist-previous/` or `target/release-previous/`, which `cargo clean`
empties with the rest.

### The app for this Mac

```sh
scripts/bundle-macos.sh          # or: just bundle-macos
scripts/start-macos.sh           # builds it when it is not there, then opens it
```

`dist/Bisa.app`, universal, the node inside, **signed ad hoc**: it opens on this Mac and on no other
— macOS puts an unsigned, un-notarized download straight in the Trash. The script reads no
environment variable, submits nothing to Apple and makes no zip; `--open` launches the result. A
copy to give to someone is the release below.

### The release

```sh
APPLE_SIGNING_IDENTITY="Developer ID Application: <name> (<team>)" BISA_NOTARY_PROFILE=<profile> just release-macos
```

`scripts/release-macos.sh` leaves exactly two files under `release/` (gitignored):
`Bisa-<version>-macos-universal.dmg`, a signed and notarized disk image, and its
`.dmg.sha256`. It is one list of steps, each one function, each refusal a sentence with the command
to run — and a test holds the list (`desktop/src/scenarios/release.test.mjs`):

1. **The inputs, first** — because notarization is the slow, rate-limited step: Apple's notary
   service takes about 75 submissions a day per team
   ([Customizing the notarization workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow)).
   The two variables; a clean tree (`git status --porcelain` empty — a release is built from a
   commit); a `<major>.<minor>.<patch>` version in `tauri.conf.json`; a section for it in
   `CHANGELOG.md` with words under it (`scripts/release/release-notes.mjs` reads it).
2. **The build and the signature**, through the library: the node, the shell, the bundle, signed
   with the Developer ID, the hardened runtime, a secure timestamp and the entitlements.
3. **The app notarized.** Zipped with `ditto -c -k --keepParent`, submitted with `xcrun notarytool
   submit --keychain-profile "$BISA_NOTARY_PROFILE" --wait --output-format json`; the submission's
   log is fetched **whatever the verdict** (`notarytool log`, into `target/release-stage/notary-app.json`)
   — Apple: *always check the log file, even if notarization succeeds*, it lists what a later macOS
   may refuse — and anything but `Accepted` stops the build with the log's path.
4. **Stapled and assessed.** `xcrun stapler staple` puts the ticket into the bundle so Gatekeeper
   accepts it with no network; `stapler validate` reads it back; `spctl --assess --type execute`
   is Gatekeeper's own word, and the script refuses to go on without it.
5. **The disk image**, from the stapled app and a link to `/Applications`, with `hdiutil create
   -volname "Bisa <version>" -srcfolder … -fs HFS+ -format UDZO -imagekey zlib-level=9` (read-only,
   compressed, mounting on every macOS the app runs on) and `hdiutil verify`. Never `-ov`: a
   previous image is moved aside first.
6. **The image signed, notarized and stapled in its turn.** `codesign --sign "$APPLE_SIGNING_IDENTITY"
   --timestamp` — a disk image is signed with the Developer ID *Application* certificate
   ([Resolving common notarization issues](https://developer.apple.com/documentation/security/resolving-common-notarization-issues));
   there is no runtime option on an image, the hardened runtime is a property of code — then the
   same submission and log (`notary-dmg.json`), `stapler staple`, `stapler validate`. A UDIF image
   takes a ticket where a zip cannot, which is why the release is an image.
7. **The app inside checked once more**, from the image mounted read-only (`hdiutil attach
   -nobrowse -readonly -mountpoint`, detached whatever happens): `stapler validate` and `spctl` on
   `Bisa.app` as a person will open it — the nested ticket travelled.
8. **The SHA-256, last** — after the staple appended the ticket, so the hash is of the bytes a
   person downloads: `shasum -a 256 <name>` run beside the file, the `<hash>  <name>` form
   `shasum -a 256 -c` reads, and checked at once.
9. **Placed** under `release/`, a previous pair moved aside; the hash and the next step printed.

The signature is the authenticity check and the hash the integrity check; the `.sha256` file is not
signed in its turn — a second key story with nobody to read it. The build is not reproducible
(timestamps, tickets and paths differ run to run); the release notes record the commit and the tag
it was built from instead. Re-signing anything after it was stapled invalidates the staple: the
script never touches a file after its staple, and neither should you.

**A rehearsal.** `just release-macos-rehearsal` (`--skip-notarize --allow-dirty`) builds, signs
and makes the image, prints its hash and stops: nothing is submitted, nothing stapled, nothing
under `release/`, and the tree may hold uncommitted changes. Gatekeeper is not asked — it refuses an
un-notarized Developer ID build for the wrong reason.

### Once, on the machine that releases

- **A Developer ID Application certificate** in the keychain: the one Apple issues for apps
  distributed outside the App Store, from the developer account Xcode already holds (Xcode ›
  Settings › Accounts › *Manage Certificates…*). Its name — *Developer ID Application: \<name\>
  (\<team\>)*, which `security find-identity -v -p codesigning` lists too — is what
  `APPLE_SIGNING_IDENTITY` carries: a name, not a key.
- **A notary profile** in the keychain: `xcrun notarytool store-credentials <profile> --apple-id
  <id> --team-id <team> --password <app-specific password>` — the password an app-specific one from
  appleid.apple.com, stored once under the profile's name, which is all `BISA_NOTARY_PROFILE`
  carries ([Customizing the notarization workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow)).
- The two variables set **in the shell, for the one command** — never in a file of this
  repository, never in a script.

### What never lands in this repository

No identity name, Team ID, Apple ID, e-mail, password or token — the pages and the scripts carry
placeholders alone (`<name>`, `<team>`, `<id>`, `<profile>`, `<app-specific password>`), and a guard
reads them for a real identity, an e-mail, a credential variable or a keychain read
(`desktop/src/scenarios/release.test.mjs`). The scripts read two environment variables and write
neither anywhere; none runs `security`; `tauri.conf.json` names no `signingIdentity`; `release/` and
`dist/` are gitignored; the notary logs stay under `target/`. The repository is public: whoever
clones it can build the app for their Mac, and only the holder of a Developer ID can build the
release — which is the point.

## The version and the changelog

`CHANGELOG.md` follows [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/): what is not
released sits under `## [Unreleased]`, in the six kinds — Added · Changed · Deprecated · Removed ·
Fixed · Security — and every release is `## [<version>] - <YYYY-MM-DD>`, newest first, with compare
links at the foot. Write there as you go; a release's section is its notes, read by the release
script before it builds and by the publishing script as the release's body.

To move to a new version:

```sh
node scripts/release/set-version.mjs 0.2.0            # a dry run: what would change
node scripts/release/set-version.mjs 0.2.0 --write    # or: just set-version 0.2.0 write=--write
```

`set-version` changes the one `version` line of every manifest that repeats the workspace's — the
root `Cargo.toml`, the shell's, `desktop/package.json`, `tauri.conf.json`, the addon SDK's package
and every built-in addon's `addon.json` — by targeted replacement, the file's formatting kept, and
cuts the changelog: Unreleased's body moves under the new dated heading, Unreleased is left empty,
the links move on. It refuses, leaving every file as it was, when Unreleased is empty (nothing to
release), when the version already has a section, or when a manifest has not exactly one version
line. Before a **minor** release, also move the `bisa-deps` requirement in every member crate's
`Cargo.toml` (`bisa-deps = { version = "0.1", path = "../bisa-deps" }`) to the new `0.y` — hakari writes
it that way, and `set-version` does not touch it yet. Then it prints what it leaves to you, each
rewritten by its own tool: `cargo update
--workspace` at the root and in `desktop/src-tauri` (the lockfiles' entries for the workspace's own
crates), `npm install --package-lock-only` in `desktop/`, `just gen-catalog-docs` (the addons'
versions on the generated page), `just verify`. The rules are pure and tested
(`scripts/release/releaseModel.mjs`, `releaseModel.test.mjs`); `just release-check` runs them with a
parse of every release script, in `verify` and in CI.

## Publishing

```sh
scripts/publish-release.sh --dry-run    # every check; the three commands that would change something, printed
scripts/publish-release.sh              # a draft release — read it on GitHub, publish it there
scripts/publish-release.sh --publish    # published at once, marked the latest
```

`just publish-release --dry-run` is the same. The script publishes on the repository the workspace
declares — `[workspace.package] repository` in `Cargo.toml`, read by `scripts/release/repository.mjs`
— never on what `gh` would guess from the clone's remote, and through a git remote of this clone
whose URL is that repository (the refusal prints the `git remote add` line when none is). Before
anything changes it checks, in order: `gh auth status` (you are signed in; no token is ever
printed); the image and its `.sha256` under `release/`, the one matching the other; a clean tree;
the repository and the remote; `HEAD` already on the remote's default branch (`git merge-base
--is-ancestor`) — a release points at a commit everybody can see; no release of this version yet —
a release is never replaced, the version moves; the tag `v<version>`, if it exists here or on the
remote, at `HEAD` — **a tag never moves**. Then it makes the annotated tag when absent, pushes it by
its one explicit refspec (`git push <remote> refs/tags/v<version>` — never `--tags`), and creates
the release against it (`gh release create --verify-tag`): a **draft** unless `--publish`, titled
*Bisa \<version\>*, its notes the changelog's section with a provenance foot (the commit, the tag,
the hash and how to check it — written to `target/release-stage/notes-<version>.md` for you to
read first), and the two artefacts labelled *Bisa \<version\> for macOS (Apple Silicon and
Intel)* and *SHA-256 of the disk image*
([`gh release create`](https://cli.github.com/manual/gh_release_create)). A draft is the default
because a release page is read by strangers: look at it once as they will, then publish from the
page or run again with `--publish`.

### Verifying a download

Beside the two files from the release's page:

```sh
shasum -a 256 -c Bisa-<version>-macos-universal.dmg.sha256   # the bytes are the published ones
xcrun stapler validate Bisa-<version>-macos-universal.dmg     # the notary's ticket is in it
```

Inside the mounted image, `codesign -dvv Bisa.app` names the team that signed it and shows a
`Timestamp` line. [`SECURITY.md`](../../SECURITY.md) says the same to a reader of the repository.

## The release checklist

Run in order; stop at the first failure.

1. **Read *Unreleased* whole.** Every entry is under the right kind; nothing declared *breaks*; the
   release type matches what it holds ([§ Versions](#versions-and-what-each-may-change)).
2. **Close the milestone.** Every issue in it is merged or moved; no `priority: critical` or
   `kind: regression` issue is open against the last release.
3. **Move the version** — [§ The version and the changelog](#the-version-and-the-changelog) — then the
   commands `set-version` prints, then `just verify`.
4. **Rehearse the upgrade by hand.** Copy a real workspace aside, open it with the previous release,
   then with this build: goals, a run, a channel, a project, settings and notes are all there, and
   `bisa logs` shows no record refused.
5. **Build and check the release** — `just release-macos` ([§ The release](#the-release)).
6. **Publish** — `just publish-release --dry-run`, then `just publish-release`; read the draft, then
   publish it ([§ Publishing](#publishing)).
7. **Say it** — the release in Discussions (*Announcements*), and the issues it closes answered with
   its version.

## Supported versions

The latest 0.x minor receives fixes, as patch releases; inside 0.x an upgrade is in place, so nobody
needs an older one. When 1.0.0 is released, the last 0.x minor keeps receiving security fixes for six
months ([SECURITY.md](../../SECURITY.md)).

## Security releases

A vulnerability is fixed in its advisory's private fork, reviewed with
[the security checklist](review/security.md), and released as a patch of the latest minor — the
changelog's *Security* entry, the advisory and the release published together. Nothing about it is
public before the release.

## The website

The website's source is in `scripts/website/` and its built pages in `website/` — any static host
serves the folder as it is ([the website's README](../../website/README.md)). It names no version:
it links to the latest release, so a release changes nothing there. Nothing publishes it on its own.

## Rollback

A change is rolled back with `git revert` and a patch release. A **person** who wants an earlier
release back restores the copy of their workspace they took before upgrading: going back to an earlier
release on the same workspace is not promised in 0.x, because a later release may write what an
earlier one does not read ([Compatibility](../reference/compatibility.md#not-promised-in-0x)). A
development workspace from before 0.1.0 is moved aside with `scripts/reset-dev-workspace`, which
renames and never deletes.
