#!/usr/bin/env bash
# Start the desktop application: dist/Bisa.app, built by
# scripts/bundle-macos.sh — built first when it is not there yet, as a build
# for this Mac alone (universal, signed ad hoc, not notarized). A copy to give
# to someone is scripts/release-macos.sh with your Developer ID
# (docs/contributing/release.md).
#
# The application carries its own node, so nothing else needs to be running
# or on PATH; the first launch creates ~/.bisa/ and your identity
# (docs/guide/getting-started.md §1).
#
#   scripts/start-macos.sh             open the app, building it if needed
#   scripts/start-macos.sh --rebuild   build it again first
set -euo pipefail

cd "$(dirname "$0")/.."

if [ "$(uname -s)" != "Darwin" ]; then
    echo "start-macos.sh: this opens a macOS application; on $(uname -s) run \`cd desktop && cargo tauri dev\`" >&2
    exit 2
fi

if [ "${1-}" = "--rebuild" ] || [ ! -d dist/Bisa.app ]; then
    scripts/bundle-macos.sh
fi

open dist/Bisa.app
