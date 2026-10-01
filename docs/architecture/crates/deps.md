# bisa-deps

The dependency-unification crate. It holds no code — `src/lib.rs` is empty on purpose — and exists
so that every third-party dependency is compiled **once, with one feature set**, whichever crate is
being checked or tested.

---

## Why it exists

Cargo unifies a dependency's features across the packages it is building. `cargo check --workspace`
builds tokio with the union of every crate's features; `cargo check -p bisa-store` builds it with
the store's alone — a different set, so a different artefact, so a rebuild of tokio and everything
above it. Switching between a single-crate check (the inner loop) and the workspace check (the gate)
therefore paid for the dependency graph twice. This crate names every third-party dependency with
the union of the features the workspace uses, and every member but `bisa-log` depends on it — the
desktop shell, a separate workspace, depends on that one crate by path and must not build the whole
table — so the set is the same whichever package Cargo starts from. The idea and the tool are
[cargo-hakari](https://docs.rs/cargo-hakari/latest/cargo_hakari/); the crate is generated, not
written.

## Where things live

| File | Owns |
|---|---|
| `Cargo.toml` | the generated dependency table between the `HAKARI SECTION` markers — one line per third-party crate with the unified feature list, for this platform; the header is ours |
| `src/lib.rs` | nothing — an empty library so the package builds |
| `build.rs` | nothing — an empty build script so the `[build-dependencies]` section unifies too |
| `.config/hakari.toml` (repository root) | the tool's configuration: the package name, the platforms it resolves for (`x86_64-apple-darwin` and `aarch64-apple-darwin`), the resolver version, and the one member it leaves out (`bisa-log`) |

## How it is kept true

`just hakari` regenerates the table, adds the dependency to any member that lacks it and verifies the
result; run it after a dependency or a feature changes anywhere in the workspace. `cargo hakari
verify` is the check that the committed table matches the graph. The tool is installed inside the
tree (`just install-hakari`, under `target/tools/`), so nothing is written outside the repository.

## What it is not

Not an edge of the architecture. [07 — Layering](../07-layering.md) is about who may call whom;
this crate is called by nobody and calls nothing, and `layering.rs` skips it by name when it reads
each member's dependencies. Nothing else may depend on being unified through it: a crate that needs
a feature names that feature itself, and the table follows.
