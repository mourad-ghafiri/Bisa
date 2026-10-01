# Build and release

How the platform is built, checked and released: the `Justfile`'s targets, the gate a person runs
before handing a change over (`just verify`), the CI that runs the same steps, the licence gate and
the third-party notices, the one version every manifest wears, the changelog, and the macOS
application — built ad hoc for this Mac, or signed, notarized and published by a person with their own
credentials. There is no release train, and nothing publishes on its own.

## Where it lives

- `Justfile` — every target (`just` alone lists them); `scripts/test` — the narrowed test runner.
- `.github/workflows/verify.yml` — CI: eight jobs running the same steps as `just verify`.
- `Cargo.toml` (the workspace's version, licence, repository and profiles) · `rust-toolchain.toml` · `.cargo/config.toml` · `.config/nextest.toml` · `.config/hakari.toml` · `crates/bisa-deps/` (the unification crate).
- `deny.toml` · `desktop/src-tauri/deny.toml` — the one licence allow list, twice; `scripts/licences/` — the gate and the notices' generator; `NOTICES.md` (written by hand) and `THIRD-PARTY-NOTICES.md` (generated).
- `scripts/macos/lib.sh` — the build both macOS scripts share; `scripts/bundle-macos.sh` · `scripts/start-macos.sh` — the app for this Mac; `scripts/release-macos.sh` — the release; `scripts/publish-release.sh` — a draft release on GitHub, run by a person.
- `scripts/release/` — `set-version.mjs`, `release-notes.mjs`, `repository.mjs`, and the pure rules in `releaseModel.mjs`.
- `CHANGELOG.md` — Keep a Changelog, written under *Unreleased* as you go.
- `desktop/src-tauri/tauri.conf.json` · `desktop/src-tauri/Entitlements.plist` — the bundle's identifier and its one entitlement.

## Read first

- [Release](../release.md) — the gate, generated files, the licence, one version, the artefacts, the macOS application, the version and the changelog, publishing, rollback.
- [Setup](../setup.md) — the toolchain and every `just` target.
- [Testing rules § Running](../testing-rules.md#running) and [§ Faster builds](../testing-rules.md#faster-builds) — the test forms, and the build settings each with its reason.
- [Performance § The node is built for speed](../performance.md#the-node-is-built-for-speed).
- [SECURITY.md](../../../SECURITY.md) — the supported version, and how a download is verified.

## Rules a change must keep

- `just verify` is the whole gate, run once before handing over; while working, one module at a time ([Release § The gate](../release.md#the-gate), [Testing rules § Running](../testing-rules.md#running)).
- Generated files travel with the change that moved them, and CI diffs them ([Release § Generated files travel with the change](../release.md#generated-files-travel-with-the-change)).
- One version: `[workspace.package] version` in `Cargo.toml`, repeated by every manifest — the desktop's, the shell's, the addon SDK's, every built-in addon's — moved by `set-version` alone, and held equal by two guards ([Release § One version](../release.md#one-version)).
- One licence list: a dependency whose licence is not on `deny.toml`'s list is a decision, not a warning; the two `deny.toml` files stay equal; MIT everywhere ([Release § Licence](../release.md#licence)).
- Nothing publishes on its own: `scripts/publish-release.sh` runs only when a person runs it — never from CI, a hook or another script; a release is a draft by default, a tag never moves, a release is never replaced ([Release § Publishing](../release.md#publishing)).
- No identity name, Team ID, Apple ID, e-mail, password or token lands in the repository; the two signing variables are set in the shell for the one command ([Release § What never lands in this repository](../release.md#what-never-lands-in-this-repository)).
- No script deletes: a path about to be written again is moved aside under `target/` ([Release § The macOS application](../release.md#the-macos-application-two-builds-one-library)); no test, fixture or CI step contains a destructive command ([Testing rules § What a test may never do](../testing-rules.md#what-a-test-may-never-do)).
- Rollback is `git revert`; a change to the on-disk shape has no backward path ([Release § Rollback](../release.md#rollback)).

## Testing a change

- `just release-check` — `node --test scripts/release/releaseModel.test.mjs` and a parse of every release script.
- `just licence-gate` and `just check-notices`; the gate's own rules: `node --test scripts/licences/licencesModel.test.mjs`.
- From `desktop/`: `node --test --import ./src/i18n/preload.mjs src/scenarios/bundle.test.mjs src/scenarios/release.test.mjs src/scenarios/licences.test.mjs src/scenarios/platformIdentity.test.mjs` — the macOS build and the release's steps as the sources show them, the licence files, one version and one identity.
- `scripts/test module core layering` — every crate wears the workspace's version, edition and licence; no test source contains a destructive command.
- `just hakari-verify` after a dependency change; `just bundle-macos` builds an ad hoc `dist/Bisa.app` to open on this Mac.
- `just verify` once, at the end.

## Common changes

- [Add a dependency](../recipes.md#28-add-a-dependency) — the licence read first, then `just licence-gate` and `just gen-notices`.
- [Regenerate and verify](../recipes.md#22-regenerate-and-verify) · [Change the platform's mark](../recipes.md#19-change-the-platforms-mark).
- A `just` target or a CI step: the same step in the `Justfile`'s `verify` and in `.github/workflows/verify.yml`, and its row in [Setup](../setup.md).
- A version move and the changelog's cut: [Release § The version and the changelog](../release.md#the-version-and-the-changelog).

## Compatibility

- Releases follow Semantic Versioning, held stricter in 0.x: a patch fixes behaviour, a minor adds, and 1.0.0 is the first release allowed to change the contract incompatibly — with a migration tool ([Compatibility](../../reference/compatibility.md), [The first major release](../../reference/compatibility.md#the-first-major-release), [Migrations](../migrations.md)).
- The changelog carries the contract's movement: what was added under *Added*, a default that moved under *Changed*, what is retired under *Deprecated* ([Deprecation](../../reference/compatibility.md#deprecation)).
- The build profiles, the CI and the scripts are not contract. Declare your change's compatibility in the pull request ([Keeping compatibility](../compatibility.md)).

## Review focus

- A new dependency: its licence read, both lists, the notices regenerated ([Licences review](../review/licences.md)).
- Nothing that signs, pushes or publishes on its own; no credential, identity or personal path in a file ([Security review](../review/security.md)).
- `just verify` and CI stay the same steps ([Code review](../review/code.md)).
- The node's profile keeps throughput — `opt-level` is not lowered for size ([Performance review](../review/performance.md)).
- The changelog's entry under the right kind ([Docs and language](../review/docs-and-language.md)).
