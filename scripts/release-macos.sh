#!/usr/bin/env bash
# Build the release of the desktop application for macOS: a signed, notarized
# disk image and its SHA-256, under release/ (docs/contributing/release.md
# §The release).
#
#   release/Bisa-<version>-macos-universal.dmg
#   release/Bisa-<version>-macos-universal.dmg.sha256
#
# Steps, each one function, in order — the build ones from scripts/macos/lib.sh:
#   the inputs checked first, because notarization is the slow, rate-limited
#   step (Apple names 75 submissions a day): the two variables below, a clean
#   tree, a <major>.<minor>.<patch> version, a changelog section for it;
#   the node for both architectures joined with lipo; the Tauri shell built
#   universal with its own signing skipped; the bundle assembled; signed
#   inside-out with the Developer ID, the hardened runtime, a secure timestamp
#   and the app's entitlements;
#   the app notarized (zipped with ditto, submitted through the keychain
#   profile, the log kept beside the stage and read whatever the verdict),
#   stapled, and assessed by Gatekeeper's own spctl;
#   the disk image made from the stapled app and a link to /Applications,
#   verified, signed with the Developer ID Application certificate — the one
#   that signs disk images — notarized in its turn, stapled, and the app inside
#   it checked once more from the mounted image;
#   the SHA-256 taken last, after the staple appended the ticket, in the form
#   `shasum -a 256 -c` reads, and checked;
#   both placed under release/.
#
# What it reads from the environment, and nothing else:
#   APPLE_SIGNING_IDENTITY   the name of your Developer ID Application
#                            certificate, as Xcode's account holds it:
#                            "Developer ID Application: <name> (<team>)"
#   BISA_NOTARY_PROFILE      the keychain profile stored once with
#                            `xcrun notarytool store-credentials`
# Neither is written anywhere; no keychain is read by this script.
#
# A rehearsal — `--skip-notarize` — builds, signs and makes the image, prints
# its hash and stops: nothing is submitted, nothing stapled, nothing under
# release/. `--allow-dirty`, allowed beside it alone, lets the tree hold
# uncommitted changes.
#
# Nothing here deletes anything: a previous image, stage, zip or hash is moved
# aside under target/release-previous/, which `cargo clean` empties with the
# rest. Notary logs stay under target/release-stage/.
#
#   APPLE_SIGNING_IDENTITY="…" BISA_NOTARY_PROFILE=<profile> scripts/release-macos.sh
#   APPLE_SIGNING_IDENTITY="…" scripts/release-macos.sh --skip-notarize [--allow-dirty]
set -euo pipefail

cd "$(dirname "$0")/.."
root="$PWD"
previous_dir="$root/target/release-previous"
# shellcheck source=scripts/macos/lib.sh
. "$root/scripts/macos/lib.sh"

stage_dir="$root/target/release-stage"
stage="$stage_dir/Bisa.app"
mount="$stage_dir/mnt"
release_dir="$root/release"
notarize=yes
allow_dirty=no
for arg in "$@"; do
    case "$arg" in
        --skip-notarize) notarize=no ;;
        --allow-dirty) allow_dirty=yes ;;
        *) refuse "unknown argument $arg" ;;
    esac
done
version=""   # set by require_release_inputs
dmg=""       # the image under the stage, named once the version is known

# --- 0. the inputs, before anything slow ----------------------------------------
require_release_inputs() {
    [ -n "${APPLE_SIGNING_IDENTITY-}" ] || refuse "APPLE_SIGNING_IDENTITY is not set. The release is signed by your Developer ID Application certificate — the one Xcode holds for your developer account; set the variable to its name, \"Developer ID Application: <name> (<team>)\" (docs/contributing/release.md §Once, on the machine that releases)."
    if [ "$notarize" = yes ]; then
        [ -n "${BISA_NOTARY_PROFILE-}" ] || refuse "BISA_NOTARY_PROFILE is not set. Notarization uses a keychain profile you store once — \`xcrun notarytool store-credentials <profile> --apple-id <id> --team-id <team> --password <app-specific password>\` — then name the profile here. For a rehearsal that submits nothing, --skip-notarize."
        [ "$allow_dirty" = no ] || refuse "--allow-dirty goes with --skip-notarize alone: a release is built from a commit"
    fi
    if [ "$allow_dirty" = no ] && [ -n "$(git status --porcelain)" ]; then
        refuse "the tree holds uncommitted changes (git status); a release is built from a commit. For a rehearsal, --skip-notarize --allow-dirty."
    fi
    version="$(version_of)"
    grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' <<<"$version" || refuse "the version \"$version\" in desktop/src-tauri/tauri.conf.json is not <major>.<minor>.<patch>"
    dmg="$stage_dir/Bisa-$version-macos-universal.dmg"
    say "release $version"
    node "$root/scripts/release/release-notes.mjs" "$version" >/dev/null || refuse "CHANGELOG.md has no section for $version with words in it: \`just set-version $version\` cuts one from Unreleased"
}

# --- 5. notarized ---------------------------------------------------------------
# One file submitted through the keychain profile, the verdict waited for,
# the log kept beside the stage — Apple: read it even when the status is
# Accepted, it lists what a later macOS may refuse. Refused unless Accepted.
notarize_file() {
    local file="$1" log="$stage_dir/$2"
    local answer id status
    answer="$(xcrun notarytool submit "$file" --keychain-profile "$BISA_NOTARY_PROFILE" --wait --output-format json)" || refuse "the notary service could not be reached, or refused the submission of ${file##*/}"
    id="$(plutil -extract id raw -o - - <<<"$answer")"
    status="$(plutil -extract status raw -o - - <<<"$answer")"
    [ -n "$id" ] || refuse "the notary service answered with no submission id: $answer"
    xcrun notarytool log "$id" --keychain-profile "$BISA_NOTARY_PROFILE" "$log"
    say "notary log: $log"
    [ "$status" = "Accepted" ] || refuse "notarization of ${file##*/} ended \"$status\", not Accepted — read $log"
}

notarize_app() {
    say "notarizing the app through the keychain profile $BISA_NOTARY_PROFILE"
    local zip="$stage_dir/Bisa-for-notary.zip"
    moved_aside "$zip" Bisa-for-notary zip
    ditto -c -k --keepParent "$stage" "$zip"
    notarize_file "$zip" notary-app.json
}

# The ticket into the bundle, so Gatekeeper needs no network to accept it.
staple_app() {
    say "stapling the app's ticket"
    xcrun stapler staple "$stage"
    xcrun stapler validate "$stage"
}

assess_app() {
    say "asking Gatekeeper about the app"
    spctl --assess --type execute --verbose=2 "$stage" || refuse "Gatekeeper did not accept the app"
}

# --- 6. the disk image ----------------------------------------------------------
# A folder with the stapled app and a link to /Applications, imaged read-only
# and compressed (UDZO, zlib at its highest; HFS+ mounts on every macOS the
# app runs on), then verified. Never `-ov`: nothing here overwrites — a
# previous image is moved aside.
make_dmg() {
    say "the disk image"
    local folder="$stage_dir/dmg"
    moved_aside "$folder" dmg folder
    mkdir -p "$folder"
    ditto "$stage" "$folder/Bisa.app"
    ln -s /Applications "$folder/Applications"
    moved_aside "$dmg" "Bisa-$version-macos-universal" dmg
    moved_aside "$dmg.sha256" "Bisa-$version-macos-universal" dmg.sha256
    hdiutil create -volname "Bisa $version" -srcfolder "$folder" -fs HFS+ -format UDZO -imagekey zlib-level=9 "$dmg"
    hdiutil verify "$dmg"
}

# A disk image is signed with the Developer ID Application certificate, with a
# secure timestamp; the hardened runtime is a property of code, not of an image.
sign_dmg() {
    say "signing the disk image"
    codesign --sign "$APPLE_SIGNING_IDENTITY" --timestamp "$dmg"
    codesign --verify --verbose=2 "$dmg"
}

notarize_dmg() {
    say "notarizing the disk image"
    notarize_file "$dmg" notary-dmg.json
}

staple_dmg() {
    say "stapling the disk image's ticket"
    xcrun stapler staple "$dmg"
    xcrun stapler validate "$dmg"
}

# The image mounted read-only and the app inside it asked about once more:
# the nested ticket travelled, and Gatekeeper accepts what a person will open.
assess_dmg() {
    say "mounting the image and asking Gatekeeper about the app inside"
    mkdir -p "$mount"
    hdiutil attach "$dmg" -nobrowse -readonly -mountpoint "$mount" >/dev/null
    trap 'hdiutil detach "$mount" >/dev/null 2>&1 || true' EXIT
    xcrun stapler validate "$mount/Bisa.app"
    spctl --assess --type execute --verbose=2 "$mount/Bisa.app" || refuse "Gatekeeper did not accept the app inside the image"
    hdiutil detach "$mount" >/dev/null
    trap - EXIT
}

# --- 7. the SHA-256, of the stapled image ---------------------------------------
# Taken last — the staple appended the ticket — in the form `shasum -a 256 -c`
# reads (`<hash>  <name>`, the name bare so the check runs beside the file),
# and checked at once.
checksum() {
    say "SHA-256"
    (cd "$(dirname "$dmg")" && shasum -a 256 "$(basename "$dmg")" > "$dmg.sha256")
    (cd "$(dirname "$dmg")" && shasum -a 256 -c "$(basename "$dmg").sha256")
}

# --- 8. placed ------------------------------------------------------------------
place() {
    say "release/"
    mkdir -p "$release_dir"
    moved_aside "$release_dir/${dmg##*/}" "Bisa-$version-macos-universal" dmg
    moved_aside "$release_dir/${dmg##*/}.sha256" "Bisa-$version-macos-universal" dmg.sha256
    mv "$dmg" "$release_dir/"
    mv "$dmg.sha256" "$release_dir/"
    echo "ready: $release_dir/${dmg##*/}"
    cat "$release_dir/${dmg##*/}.sha256"
    echo "next: scripts/publish-release.sh --dry-run, then without the flag for a draft release on GitHub"
}

# A rehearsal ends here: the image and its hash, said to be nobody's release.
rehearsal_end() {
    say "a rehearsal: nothing was submitted to Apple, nothing stapled, nothing under release/"
    (cd "$(dirname "$dmg")" && shasum -a 256 "$(basename "$dmg")")
    echo "the image: $dmg — signed, not notarized; another Mac would refuse it"
}

# The release, in order. A test holds this list (desktop/src/scenarios/release.test.mjs).
release() {
    require_tools cargo rustup npm codesign ditto lipo plutil xcrun spctl hdiutil shasum git node
    require_release_inputs
    build_node
    build_shell
    assemble "$stage"
    sign_app "$stage" "$APPLE_SIGNING_IDENTITY"
    notarize_app
    staple_app
    assess_app
    make_dmg
    sign_dmg
    notarize_dmg
    staple_dmg
    assess_dmg
    checksum
    place
}

# A rehearsal: the same build and signature, the image made and signed, its
# hash printed; nothing submitted, nothing stapled, nothing under release/.
rehearsal() {
    require_tools cargo rustup npm codesign ditto lipo plutil xcrun hdiutil shasum git node
    require_release_inputs
    build_node
    build_shell
    assemble "$stage"
    sign_app "$stage" "$APPLE_SIGNING_IDENTITY"
    make_dmg
    sign_dmg
    rehearsal_end
}

if [ "$notarize" = yes ]; then release; else rehearsal; fi
