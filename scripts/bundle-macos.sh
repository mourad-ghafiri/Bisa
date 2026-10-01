#!/usr/bin/env bash
# Build the desktop application for this Mac: dist/Bisa.app — one bundle that
# runs on Apple Silicon and on Intel, the node inside it, signed ad hoc so it
# opens here and nowhere else (docs/contributing/release.md §The app for this
# Mac).
#
# Steps, each one function of scripts/macos/lib.sh, in order: the node at the
# `dist` profile for both architectures, joined with lipo; the Tauri shell at
# its release profile for universal-apple-darwin with its own signing skipped;
# the bundle assembled — the node beside the shell, the licence and the
# notices under Resources; signed inside-out, ad hoc, with the hardened
# runtime and the app's entitlements; placed at dist/Bisa.app.
#
# Another Mac refuses an ad hoc build — macOS puts an un-notarized download in
# the Trash. A copy to give to someone is scripts/release-macos.sh: the same
# build signed with your Developer ID, notarized, stapled, and shipped as a
# disk image with its SHA-256.
#
# Nothing here deletes anything: a previous dist/Bisa.app and a previous stage
# are moved aside under target/dist-previous/, which `cargo clean` empties
# with the rest. Nothing here reads an identity, a keychain or a password.
#
#   scripts/bundle-macos.sh          build, sign ad hoc, place
#   scripts/bundle-macos.sh --open   …then launch it
set -euo pipefail

cd "$(dirname "$0")/.."
root="$PWD"
previous_dir="$root/target/dist-previous"
# shellcheck source=scripts/macos/lib.sh
. "$root/scripts/macos/lib.sh"

stage="$root/target/dist-stage/Bisa.app"
open_after=no
for arg in "$@"; do
    case "$arg" in
        --open) open_after=yes ;;
        *) refuse "unknown argument $arg" ;;
    esac
done

# --- 5. placed ------------------------------------------------------------------
place() {
    say "dist/Bisa.app"
    mkdir -p "$root/dist"
    moved_aside "$root/dist/Bisa.app" Bisa
    mv "$stage" "$root/dist/Bisa.app"
    echo "ready: $root/dist/Bisa.app ($(lipo -archs "$root/dist/Bisa.app/Contents/MacOS/bisa-desktop"))"
    echo "run it: scripts/start-macos.sh"
    if [ "$open_after" = yes ]; then
        open "$root/dist/Bisa.app"
    fi
}

require_tools cargo rustup npm codesign ditto lipo plutil
build_node
build_shell
assemble "$stage"
sign_app "$stage" -
place
