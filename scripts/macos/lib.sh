#!/usr/bin/env bash
# The macOS build, written once and shared by the two scripts that make a
# Bisa.app (docs/contributing/release.md):
#
#   scripts/bundle-macos.sh     the app for this Mac — signed ad hoc, dist/Bisa.app
#   scripts/release-macos.sh    the release — Developer ID, notarized, a disk image
#   scripts/publish-release.sh  borrows `say`, `refuse` and `version_of` alone
#
# Sourced, never run: this file defines the paths both builds share and one
# function per step — the node for both architectures joined with lipo, the
# Tauri shell built universal with its own signing skipped, the bundle
# assembled, the bundle signed inside-out — and calls nothing. A caller sets,
# before sourcing:
#
#   root           the repository's root (an absolute path)
#   previous_dir   where a path about to be written again is moved — never
#                  deleted — e.g. target/dist-previous or target/release-previous
#
# Nothing here reads a keychain or a password. An identity is a name handed
# to codesign, and `-` is the ad hoc one.

TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)
UNIVERSAL="universal-apple-darwin"
node_out="$root/target/dist/universal/bisa"
shell_bundle="$root/desktop/src-tauri/target/$UNIVERSAL/release/bundle/macos/Bisa.app"
entitlements="$root/desktop/src-tauri/Entitlements.plist"
tauri_conf="$root/desktop/src-tauri/tauri.conf.json"

# A line of progress; a refusal, naming the script that stopped.
say() { echo "▸ $*"; }
refuse() { echo "${0##*/}: $*" >&2; exit 2; }

# A path about to be written again — a previous bundle, a previous image, the
# zip of the last run — is moved under $previous_dir as `<name>-<stamp>.<ext>`;
# `ext` is `app` unless given. Nothing is ever removed.
moved_aside() {
    local path="$1" name="$2" ext="${3:-app}"
    if [ -e "$path" ]; then
        mkdir -p "$previous_dir"
        mv "$path" "$previous_dir/$name-$(date +%Y%m%dT%H%M%S).$ext"
    fi
}

# The platform's version, the one tauri.conf.json wears (platformIdentity.test.mjs
# holds every manifest to it).
version_of() { plutil -extract version raw -o - "$tauri_conf"; }

# --- 0. what the build needs ----------------------------------------------------
# The tools named, both Rust targets, the entitlements file that parses.
require_tools() {
    if [ "$(uname -s)" != "Darwin" ]; then
        refuse "this builds a macOS application; on $(uname -s) run \`just desktop-bundle\` and take the bundle from desktop/src-tauri/target/release/bundle/"
    fi
    local tool
    for tool in "$@"; do
        command -v "$tool" >/dev/null || refuse "$tool is not on PATH"
    done
    local installed t
    installed="$(rustup target list --installed)"
    for t in "${TARGETS[@]}"; do
        grep -qx "$t" <<<"$installed" || refuse "the Rust target $t is not installed — run \`rustup target add $t\` (rust-toolchain.toml names both; rustup adds them on the next cargo call as well)"
    done
    [ -f "$entitlements" ] || refuse "$entitlements is missing"
    plutil -lint "$entitlements" >/dev/null || refuse "$entitlements does not parse"
}

# --- 1. the node, universal -----------------------------------------------------
# `bisa` at the `dist` profile (throughput: it runs for weeks as the app's
# sidecar) for each architecture, joined into one Mach-O; both slices or a
# refusal.
build_node() {
    say "node (dist profile) for ${TARGETS[*]}"
    local thin=() t
    for t in "${TARGETS[@]}"; do
        # The binary is named: the package's other one is the tests' own.
        cargo build --profile dist -p bisa-cli --bin bisa --target "$t"
        thin+=("$root/target/$t/dist/bisa")
    done
    mkdir -p "$(dirname "$node_out")"
    lipo -create -output "$node_out" "${thin[@]}"
    local archs
    archs="$(lipo -archs "$node_out")"
    say "node: $archs"
    local want
    for want in x86_64 arm64; do
        grep -qw "$want" <<<"$archs" || refuse "the node is not universal: lipo -archs says \"$archs\""
    done
}

# --- 2. the shell, universal, unsigned for now ----------------------------------
# The Tauri CLI's own signing is skipped on purpose: a file is added after its
# build, so one signature pass over the finished bundle is the only right
# order.
build_shell() {
    say "desktop shell (release, $UNIVERSAL)"
    if [ ! -d "$root/desktop/node_modules" ]; then
        (cd "$root/desktop" && npm install)
    fi
    (cd "$root/desktop" && npm run tauri build -- --bundles app --target "$UNIVERSAL" --no-sign)
    [ -d "$shell_bundle" ] || refuse "expected $shell_bundle after the build"
}

# --- 3. one bundle --------------------------------------------------------------
# The shell copied to the stage; the node beside it (Contents/MacOS/, where
# desktop/src-tauri/src/sidecar.rs looks first); the licence and the
# third-party notices under Contents/Resources/, where About reveals them.
assemble() {
    local stage="$1"
    say "the node, the licence and the notices into the bundle"
    mkdir -p "$(dirname "$stage")"
    moved_aside "$stage" stage
    ditto "$shell_bundle" "$stage"
    cp "$node_out" "$stage/Contents/MacOS/bisa"
    chmod 755 "$stage/Contents/MacOS/bisa"
    cp "$root/LICENSE" "$stage/Contents/Resources/LICENSE"
    cp "$root/THIRD-PARTY-NOTICES.md" "$stage/Contents/Resources/THIRD-PARTY-NOTICES.md"
}

# --- 4. signed, inside-out ------------------------------------------------------
# The node, then the app, each with the hardened runtime; the app with its
# entitlements. One rule: a secure timestamp whenever the identity is not the
# ad hoc one — the notary service requires it, and an ad hoc signature cannot
# carry one. Never `--deep` for signing, which Apple deprecates; verification
# alone walks deep. Then a refusal if either carries the debugging entitlement
# notarization refuses.
sign_app() {
    local stage="$1" identity="$2"
    if [ "$identity" = "-" ]; then
        say "signing ad hoc — a build for this Mac alone; another Mac would refuse it"
        codesign --force --options runtime --sign - "$stage/Contents/MacOS/bisa"
        codesign --force --options runtime --entitlements "$entitlements" --sign - "$stage"
    else
        say "signing with the Developer ID identity: the node, then the app"
        codesign --force --options runtime --timestamp --sign "$identity" "$stage/Contents/MacOS/bisa"
        codesign --force --options runtime --timestamp --entitlements "$entitlements" --sign "$identity" "$stage"
    fi
    codesign --verify --deep --strict --verbose=2 "$stage"
    refuse_debuggable "$stage/Contents/MacOS/bisa"
    refuse_debuggable "$stage"
}

# A signed thing may not carry `com.apple.security.get-task-allow`: the notary
# service refuses it (Apple, "Resolving common notarization issues"). The
# entitlements file never names it; this holds the rule against a later edit.
refuse_debuggable() {
    if codesign -d --entitlements :- "$1" 2>/dev/null | grep -q "com.apple.security.get-task-allow"; then
        refuse "$1 carries com.apple.security.get-task-allow, which notarization refuses"
    fi
}
