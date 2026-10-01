#!/usr/bin/env bash
# Publish a release on GitHub: the tag, a draft release on the repository the
# workspace declares, the two artefacts scripts/release-macos.sh left under
# release/, and the changelog's section as the notes (docs/contributing/
# release.md §Publishing). A person runs it, as themselves, through the `gh`
# they are signed in with; nothing runs it.
#
#   scripts/publish-release.sh --dry-run   every check run; the three commands that
#                                          would change something printed, not run
#   scripts/publish-release.sh             a draft release — read it on GitHub, publish there
#   scripts/publish-release.sh --publish   published at once, marked the latest
#
# What it checks before anything changes: gh signed in; the image and its
# .sha256 under release/, and the one matching the other; a clean tree; the
# repository — Cargo.toml's `[workspace.package] repository`, and a git remote
# whose URL is that repository (never gh's guess from the clone); HEAD already
# on that remote's default branch; no release of this version yet; the tag,
# if it exists here or there, at HEAD. Then: the tag made if absent, pushed by
# its one explicit refspec, the release created against it (`--verify-tag`).
#
# What it never does: move a tag, force anything, replace a release, read a
# token. The notes land under target/release-stage/ for you to read.
set -euo pipefail

cd "$(dirname "$0")/.."
root="$PWD"
previous_dir="$root/target/release-previous"
# shellcheck source=scripts/macos/lib.sh
. "$root/scripts/macos/lib.sh"

release_dir="$root/release"
notes_dir="$root/target/release-stage"
mode=draft
dry_run=no
for arg in "$@"; do
    case "$arg" in
        --dry-run) dry_run=yes ;;
        --publish) mode=publish ;;
        *) refuse "unknown argument $arg" ;;
    esac
done
version="" tag="" dmg="" sha="" slug="" remote="" default_branch="" notes=""

# A step that changes something — here or on the remote — runs, or under
# --dry-run is printed as it would run.
act() {
    if [ "$dry_run" = yes ]; then
        printf 'would run:'
        printf ' %q' "$@"
        echo
    else
        "$@"
    fi
}

require_tools_here() {
    local tool
    for tool in gh git node shasum plutil; do
        command -v "$tool" >/dev/null || refuse "$tool is not on PATH"
    done
    gh auth status >/dev/null 2>&1 || refuse "gh is not signed in: run \`gh auth login\` — the release is created as you"
}

require_artefacts() {
    version="$(version_of)"
    grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' <<<"$version" || refuse "the version \"$version\" in desktop/src-tauri/tauri.conf.json is not <major>.<minor>.<patch>"
    tag="v$version"
    dmg="$release_dir/Bisa-$version-macos-universal.dmg"
    sha="$dmg.sha256"
    [ -f "$dmg" ] || refuse "${dmg#"$root/"} is not there: build the release first (just release-macos)"
    [ -f "$sha" ] || refuse "${sha#"$root/"} is not there: build the release first (just release-macos)"
    (cd "$release_dir" && shasum -a 256 -c "${sha##*/}") || refuse "the image does not match its SHA-256: build the release again"
}

require_clean_tree() {
    [ -z "$(git status --porcelain)" ] || refuse "the tree holds uncommitted changes (git status): publish from the commit the release was built from"
}

# The repository is the declared one; the remote is whichever of this clone's
# points at it. The default branch is read from GitHub, not guessed.
find_repository() {
    slug="$(node "$root/scripts/release/repository.mjs")"
    remote="$(git remote -v | node "$root/scripts/release/repository.mjs" --remote)" || refuse "no git remote points at $slug — add one: git remote add bisa https://github.com/$slug"
    default_branch="$(gh repo view "$slug" --json defaultBranchRef --jq .defaultBranchRef.name)"
    [ -n "$default_branch" ] || refuse "GitHub named no default branch for $slug"
    say "repository $slug (remote $remote, default branch $default_branch)"
}

# A release points at a commit everybody can see: HEAD must already be on the
# default branch of the remote.
require_head_pushed() {
    git fetch --quiet "$remote" "$default_branch"
    git merge-base --is-ancestor HEAD "$remote/$default_branch" || refuse "HEAD is not on $remote/$default_branch yet: merge and push it first — a release is cut from the default branch"
}

require_no_release() {
    if gh release view "$tag" --repo "$slug" >/dev/null 2>&1; then
        refuse "a release $tag already exists on $slug; a release is never replaced — bump the version (just set-version)"
    fi
}

# The notes: the changelog's section, then where the release came from and how
# to verify the download.
write_notes() {
    local section
    section="$(node "$root/scripts/release/release-notes.mjs" "$version")" || refuse "CHANGELOG.md has no section for $version with words in it"
    mkdir -p "$notes_dir"
    notes="$notes_dir/notes-$version.md"
    {
        echo "$section"
        echo
        echo "---"
        echo
        echo "Built from commit \`$(git rev-parse HEAD)\`, tag \`$tag\`, for macOS 11 and later on Apple Silicon and Intel. Verify the download beside its hash:"
        echo
        echo '```sh'
        echo "shasum -a 256 -c ${sha##*/}"
        echo '```'
        echo
        echo '```'
        cat "$sha"
        echo '```'
    } > "$notes"
    say "notes: $notes"
}

# The tag `v<version>` at HEAD: made when absent; when it exists here or on the
# remote it must already be at HEAD — a tag never moves. Pushed by its one
# refspec, never `--tags`.
ensure_tag() {
    local here remote_at
    here="$(git rev-parse HEAD)"
    remote_at="$(git ls-remote --tags "$remote" "refs/tags/$tag" | cut -f1)"
    if [ -n "$remote_at" ]; then
        if ! git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
            git fetch --quiet "$remote" "refs/tags/$tag:refs/tags/$tag"
        fi
        [ "$(git rev-parse "refs/tags/$tag")" = "$remote_at" ] || refuse "the tag $tag on $remote is not the one here; a tag never moves — bump the version instead"
        [ "$(git rev-parse "$tag^{commit}")" = "$here" ] || refuse "the tag $tag on $remote is not at HEAD; a tag never moves — bump the version instead"
        say "the tag $tag is already on $remote, at HEAD"
        return
    fi
    if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
        [ "$(git rev-parse "$tag^{commit}")" = "$here" ] || refuse "the tag $tag exists here at $(git rev-parse --short "$tag^{commit}"), not at HEAD; a tag never moves — bump the version instead"
    else
        act git tag -a "$tag" -m "Bisa $version"
    fi
    act git push "$remote" "refs/tags/$tag"
}

# The release against the tag that exists (`--verify-tag`), a draft unless
# --publish, the two artefacts labelled.
create_release() {
    local state=--draft
    if [ "$mode" = publish ]; then
        state=--latest
    fi
    act gh release create "$tag" --repo "$slug" --verify-tag "$state" --title "Bisa $version" --notes-file "$notes" \
        "$dmg#Bisa $version for macOS (Apple Silicon and Intel)" \
        "$sha#SHA-256 of the disk image"
    if [ "$dry_run" = yes ]; then
        echo "the release would be at https://github.com/$slug/releases/tag/$tag ($mode)"
    else
        echo "release: $(gh release view "$tag" --repo "$slug" --json url --jq .url) ($mode)"
    fi
}

require_tools_here
require_artefacts
require_clean_tree
find_repository
require_head_pushed
require_no_release
write_notes
ensure_tag
create_release
