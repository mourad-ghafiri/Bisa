/**
 * The release as the sources show it (contributing/release.md §The release,
 * §Publishing): `scripts/release-macos.sh` checks its inputs before anything
 * slow, builds and signs through the shared library, notarizes the app and
 * staples it, makes the disk image from the stapled app, signs, notarizes and
 * staples the image in its turn, checks the app inside it from the mounted
 * image, takes the SHA-256 last and places exactly two files under
 * `release/`; a rehearsal does none of the notary's part and writes nothing
 * there. `scripts/publish-release.sh` tags, pushes one refspec and makes a
 * draft on the declared repository, never moving a tag or forcing anything.
 * Nothing sensitive in any of it. Source assertions; no build, no notary.
 * Run with `node --test desktop/src/scenarios/release.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { assetNames, repositorySlug, sectionFor } from "../../../scripts/release/releaseModel.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
const read = (rel) => readFileSync(join(root, rel), "utf8");
const release = read("scripts/release-macos.sh");
const publish = read("scripts/publish-release.sh");
const lib = read("scripts/macos/lib.sh");
const justfile = read("Justfile");
const VERSION = JSON.parse(read("desktop/src-tauri/tauri.conf.json")).version;

/** The lines of a shell script that run: no comments, no blanks. */
const code = (text) => text.split("\n").filter((l) => l.trim() && !l.trim().startsWith("#"));
/** The body of a shell function, by name. */
const fnBody = (text, name) => {
  const at = text.indexOf(`\n${name}() {\n`);
  assert.ok(at >= 0, `${name}() is defined`);
  const start = at + `\n${name}() {\n`.length;
  const end = text.indexOf("\n}\n", start);
  return text.slice(start, end);
};
/** The first word of each line of a function's body: the steps it takes. */
const steps = (text, name) => code(fnBody(text, name)).map((l) => l.trim().split(/\s+/)[0]);
/** Every `moved_aside <path> <name> [ext]` call, as its three words. */
const movedAside = (text) => [...text.matchAll(/^\s*moved_aside\s+(\S+)\s+(\S+)(?:\s+(\S+))?$/gm)].map((m) => [m[1], m[2], m[3] ?? "app"]);

test("the release is one list of steps in order, the rehearsal a shorter one; both end where they say", () => {
  assert.deepEqual(steps(release, "release"), ["require_tools", "require_release_inputs", "build_node", "build_shell", "assemble", "sign_app", "notarize_app", "staple_app", "assess_app", "make_dmg", "sign_dmg", "notarize_dmg", "staple_dmg", "assess_dmg", "checksum", "place"]);
  assert.deepEqual(steps(release, "rehearsal"), ["require_tools", "require_release_inputs", "build_node", "build_shell", "assemble", "sign_app", "make_dmg", "sign_dmg", "rehearsal_end"]);
  assert.ok(release.endsWith('if [ "$notarize" = yes ]; then release; else rehearsal; fi\n'), "the foot picks one of the two and nothing else runs");
  assert.ok(release.includes('. "$root/scripts/macos/lib.sh"'), "the build steps are the library's");
  assert.ok(release.includes('sign_app "$stage" "$APPLE_SIGNING_IDENTITY"'), "signed with the Developer ID");
  // `release/` is written in `place` and nowhere else; the rehearsal never reaches it.
  const releaseDirUses = code(release).filter((l) => l.includes("$release_dir") && !l.startsWith("release_dir="));
  assert.ok(releaseDirUses.length > 0 && releaseDirUses.every((l) => fnBody(release, "place").includes(l)), "release/ is place()'s alone");
  assert.ok(fnBody(release, "rehearsal_end").includes("nothing under release/") && !fnBody(release, "rehearsal_end").includes("$release_dir"));
});

test("the inputs are checked first — the two variables, a clean tree, the version's form, a changelog section — because notarization is the slow, rated step", () => {
  const inputs = fnBody(release, "require_release_inputs");
  assert.ok(inputs.includes("APPLE_SIGNING_IDENTITY is not set") && inputs.includes("Developer ID Application: <name> (<team>)"), "the identity's refusal says what to set, in placeholders");
  assert.ok(inputs.includes("BISA_NOTARY_PROFILE is not set") && inputs.includes("xcrun notarytool store-credentials <profile> --apple-id <id> --team-id <team> --password <app-specific password>"), "the one-time setup is in the refusal, in placeholders");
  assert.ok(inputs.includes("git status --porcelain"), "a clean tree");
  assert.ok(inputs.includes("--allow-dirty goes with --skip-notarize alone"), "a dirty tree only for a rehearsal");
  assert.ok(inputs.includes("grep -Eq '^[0-9]+\\.[0-9]+\\.[0-9]+$' <<<\"$version\""), "<major>.<minor>.<patch>");
  assert.ok(inputs.includes('node "$root/scripts/release/release-notes.mjs" "$version"'), "the changelog's section, through the one reader");
  assert.ok(release.includes("75 submissions a day"), "the header says why the inputs come first");
  assert.ok(fnBody(release, "release").includes("require_tools cargo rustup npm codesign ditto lipo plutil xcrun spctl hdiutil shasum git node"), "every tool named before anything runs");
});

test("the app is notarized through the keychain profile, its log always fetched, refused unless Accepted; then stapled, validated and assessed", () => {
  const notarize = fnBody(release, "notarize_file");
  assert.ok(notarize.includes('xcrun notarytool submit "$file" --keychain-profile "$BISA_NOTARY_PROFILE" --wait --output-format json'));
  assert.ok(notarize.includes('xcrun notarytool log "$id" --keychain-profile "$BISA_NOTARY_PROFILE" "$log"'), "the log, into a file");
  assert.ok(notarize.indexOf("xcrun notarytool log") < notarize.indexOf('[ "$status" = "Accepted" ]'), "the log is fetched before the verdict is judged — Apple: read it even on Accepted");
  assert.ok(notarize.includes("plutil -extract id raw -o - -") && notarize.includes("plutil -extract status raw -o - -"), "the answer read as JSON, not grepped");
  assert.ok(fnBody(release, "notarize_app").includes("notary-app.json") && fnBody(release, "notarize_dmg").includes("notary-dmg.json"), "one log per submission, under the stage");
  assert.ok(fnBody(release, "notarize_app").includes('ditto -c -k --keepParent "$stage" "$zip"'), "zipped as Apple says");
  assert.ok(fnBody(release, "staple_app").includes('xcrun stapler staple "$stage"') && fnBody(release, "staple_app").includes('xcrun stapler validate "$stage"'));
  assert.ok(fnBody(release, "assess_app").includes('spctl --assess --type execute --verbose=2 "$stage"'), "Gatekeeper's own word");
  assert.ok(!release.includes("notarytool history"), "the submission's own id, never a guess from the history");
});

test("the disk image: the stapled app and a link to Applications, HFS+, UDZO at zlib 9, a volume name, never -ov; verified; signed with the Developer ID and a timestamp — no runtime option on an image; notarized and stapled in its turn; the app inside checked from the mounted image", () => {
  const make = fnBody(release, "make_dmg");
  assert.ok(make.includes('ditto "$stage" "$folder/Bisa.app"') && make.includes('ln -s /Applications "$folder/Applications"'));
  const create = code(make).find((l) => l.includes("hdiutil create"));
  assert.ok(create, "hdiutil create");
  for (const flag of ['-volname "Bisa $version"', '-srcfolder "$folder"', "-fs HFS+", "-format UDZO", "-imagekey zlib-level=9", '"$dmg"']) assert.ok(create.includes(flag), `${flag} on the create line`);
  assert.ok(!create.includes("-ov"), "nothing overwrites: a previous image is moved aside first");
  assert.ok(make.indexOf("moved_aside \"$dmg\"") < make.indexOf("hdiutil create"), "moved aside before the image is made");
  assert.ok(make.includes('hdiutil verify "$dmg"'));
  const sign = fnBody(release, "sign_dmg");
  assert.ok(sign.includes('codesign --sign "$APPLE_SIGNING_IDENTITY" --timestamp "$dmg"') && !sign.includes("--options"), "the Developer ID Application certificate signs disk images; the hardened runtime is code's");
  assert.ok(sign.includes('codesign --verify --verbose=2 "$dmg"'));
  assert.ok(fnBody(release, "staple_dmg").includes('xcrun stapler staple "$dmg"') && fnBody(release, "staple_dmg").includes('xcrun stapler validate "$dmg"'));
  const assess = fnBody(release, "assess_dmg");
  assert.ok(assess.includes('hdiutil attach "$dmg" -nobrowse -readonly -mountpoint "$mount"'), "mounted read-only, out of the Finder");
  assert.ok(/trap 'hdiutil detach "\$mount"[^']*' EXIT/.test(assess) && assess.includes("trap - EXIT"), "detached whatever happens");
  assert.ok(assess.includes('xcrun stapler validate "$mount/Bisa.app"') && assess.includes('spctl --assess --type execute --verbose=2 "$mount/Bisa.app"'), "the nested ticket travelled and Gatekeeper accepts what a person opens");
});

test("the SHA-256 is of the stapled image, in the form shasum -c reads, and checked at once; the two files are placed and the next step named", () => {
  const list = steps(release, "release");
  assert.ok(list.indexOf("checksum") > list.indexOf("staple_dmg"), "after the staple appended the ticket");
  const checksum = fnBody(release, "checksum");
  assert.ok(checksum.includes('shasum -a 256 "$(basename "$dmg")" > "$dmg.sha256"'), "the bare name beside the file: `<hash>  <name>`");
  assert.ok(checksum.includes('shasum -a 256 -c "$(basename "$dmg").sha256"'), "self-checked");
  const place = fnBody(release, "place");
  assert.ok(place.includes('mv "$dmg" "$release_dir/"') && place.includes('mv "$dmg.sha256" "$release_dir/"'), "exactly the two files");
  assert.ok(place.includes("scripts/publish-release.sh --dry-run"), "the next step is named");
  assert.equal(release.match(/Bisa-\$version-macos-universal\.dmg/g).length, 1, "the image is named once");
  assert.deepEqual(assetNames(VERSION), { dmg: `Bisa-${VERSION}-macos-universal.dmg`, sha256: `Bisa-${VERSION}-macos-universal.dmg.sha256` }, "the model names what the script makes");
});

test("nothing is deleted by the release; what is written again is moved aside under target/release-previous; the notary logs stay under the stage", () => {
  assert.ok(!/(^|[\s;&|])rm\s/m.test(release) && !/(^|[\s;&|])rm\s/m.test(publish), "neither script removes anything");
  assert.ok(release.includes('previous_dir="$root/target/release-previous"'));
  assert.deepEqual(movedAside(release), [
    ['"$zip"', "Bisa-for-notary", "zip"],
    ['"$folder"', "dmg", "folder"],
    ['"$dmg"', '"Bisa-$version-macos-universal"', "dmg"],
    ['"$dmg.sha256"', '"Bisa-$version-macos-universal"', "dmg.sha256"],
    ['"$release_dir/${dmg##*/}"', '"Bisa-$version-macos-universal"', "dmg"],
    ['"$release_dir/${dmg##*/}.sha256"', '"Bisa-$version-macos-universal"', "dmg.sha256"],
  ], "the notary zip, the image's folder, the stage's image and hash, the release's image and hash");
  assert.ok(release.includes('stage_dir="$root/target/release-stage"') && release.includes('log="$stage_dir/$2"'), "the logs under the stage, never under release/");
});

test("publishing: every check, then one tag pushed by its refspec and one draft release on the declared repository — never a moved tag, never a force, never a token read", () => {
  assert.deepEqual(code(publish).slice(-9).map((l) => l.trim()), ["require_tools_here", "require_artefacts", "require_clean_tree", "find_repository", "require_head_pushed", "require_no_release", "write_notes", "ensure_tag", "create_release"], "the steps, in order, at the foot");
  assert.ok(publish.includes("gh auth status >/dev/null 2>&1 || refuse") && !publish.includes("show-token") && !publish.includes("gh auth token"), "signed in is asked; a token is never printed");
  assert.ok(fnBody(publish, "require_artefacts").includes('shasum -a 256 -c "${sha##*/}"'), "the image against its hash before anything");
  assert.ok(fnBody(publish, "require_clean_tree").includes("git status --porcelain"));
  const repo = fnBody(publish, "find_repository");
  assert.ok(repo.includes('node "$root/scripts/release/repository.mjs"') && repo.includes("repository.mjs\" --remote") && repo.includes("git remote add bisa https://github.com/$slug"), "the declared repository and a remote that is it; the fix in the refusal");
  assert.ok(repo.includes('gh repo view "$slug" --json defaultBranchRef'), "the default branch read, not guessed");
  assert.ok(fnBody(publish, "require_head_pushed").includes('git merge-base --is-ancestor HEAD "$remote/$default_branch"'), "a release points at a commit everybody can see");
  assert.ok(fnBody(publish, "require_no_release").includes('gh release view "$tag" --repo "$slug"'), "a release is never replaced");
  const tag = fnBody(publish, "ensure_tag");
  assert.ok(tag.includes('git ls-remote --tags "$remote" "refs/tags/$tag"') && tag.includes("a tag never moves"), "a tag that exists must be at HEAD, here and there");
  assert.ok(tag.includes('act git tag -a "$tag" -m "Bisa $version"'), "an annotated tag, made only when absent");
  const pushes = code(publish).filter((l) => /\bgit push\b/.test(l));
  assert.deepEqual(pushes.map((l) => l.trim()), ['act git push "$remote" "refs/tags/$tag"'], "one push, one explicit refspec — never --tags, never --all");
  for (const l of code(publish).filter((l) => l.includes("gh release"))) assert.ok(l.includes('--repo "$slug"'), `every gh call names the repository: ${l.trim()}`);
  const create = fnBody(publish, "create_release");
  assert.ok(create.includes('act gh release create "$tag" --repo "$slug" --verify-tag "$state" --title "Bisa $version" --notes-file "$notes"'), "against the tag that exists, with the notes");
  assert.ok(create.includes("state=--draft") && create.includes("state=--latest") && create.includes('if [ "$mode" = publish ]; then'), "a draft unless --publish");
  assert.ok(create.includes('"$dmg#Bisa $version for macOS (Apple Silicon and Intel)"') && create.includes('"$sha#SHA-256 of the disk image"'), "the two artefacts, labelled");
  assert.ok(publish.includes("--dry-run") && fnBody(publish, "act").includes('if [ "$dry_run" = yes ]; then') && fnBody(publish, "act").includes("would run:"), "a dry run prints what would run");
  for (const step of ["git tag", "git push", "gh release create"]) assert.ok(code(publish).some((l) => l.trim().startsWith(`act ${step}`)), `${step} goes through act, so a dry run prints it`);
  const notes = fnBody(publish, "write_notes");
  assert.ok(notes.includes('node "$root/scripts/release/release-notes.mjs" "$version"') && notes.includes("git rev-parse HEAD") && notes.includes('cat "$sha"'), "the changelog's section, the commit, the hash");
  assert.ok(notes.includes('notes="$notes_dir/notes-$version.md"') && publish.includes('notes_dir="$root/target/release-stage"'), "the notes land under the stage");
});

test("nothing sensitive in the scripts or the pages: no identity, Team ID, Apple ID, e-mail or password — placeholders alone — and no keychain read", () => {
  const files = ["scripts/macos/lib.sh", "scripts/bundle-macos.sh", "scripts/release-macos.sh", "scripts/publish-release.sh", "scripts/start-macos.sh", "scripts/release/release-notes.mjs", "scripts/release/set-version.mjs", "scripts/release/repository.mjs", "Justfile", "docs/contributing/release.md", "docs/guide/getting-started.md", "docs/contributing/setup.md", "SECURITY.md", "CHANGELOG.md"];
  for (const rel of files) {
    const text = read(rel);
    assert.ok(!/Developer ID Application: [^()<\n]+\([0-9A-Z]{10}\)/.test(text), `${rel}: no real identity — the example is placeholders`);
    assert.ok(!/[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/.test(text), `${rel}: no e-mail address`);
    for (const [flag, placeholder] of [["--team-id", "<team>"], ["--apple-id", "<id>"], ["--password", "<app-specific password>"]]) {
      for (const m of text.matchAll(new RegExp(`${flag} (\\S+(?: \\S+)?)`, "g"))) assert.ok(m[1].startsWith(placeholder), `${rel}: ${flag} is followed by its placeholder, not a value: ${m[0]}`);
    }
    assert.ok(!text.includes("APPLE_PASSWORD") && !text.includes("APPLE_TEAM_ID") && !text.includes("APPLE_ID="), `${rel}: no credential variable`);
    assert.ok(!/(^\s*|\$\()security /m.test(text), `${rel}: no keychain read`);
  }
  assert.ok(lib.includes("Nothing here reads a keychain or a password"));
  assert.equal(JSON.parse(read("desktop/src-tauri/tauri.conf.json")).bundle.macOS.signingIdentity, undefined);
  assert.ok(read(".gitignore").includes("\n/release/\n"), "release/ never lands in the repository");
});

test("the changelog is Keep a Changelog, with a section with words for the version the tree wears; the security policy names the repository; the recipes and CI carry the release gate", () => {
  const changelog = read("CHANGELOG.md");
  assert.ok(changelog.startsWith("# Changelog\n") && changelog.includes("keepachangelog.com/en/1.1.0") && changelog.includes("\n## [Unreleased]\n"));
  assert.notEqual(sectionFor(changelog, "Unreleased"), null);
  assert.ok(sectionFor(changelog, VERSION), `a section for ${VERSION} with words in it`);
  assert.match(changelog, new RegExp(`^## \\[${VERSION.replaceAll(".", "\\.")}\\] - \\d{4}-\\d{2}-\\d{2}$`, "m"), "a dated heading");
  assert.ok(changelog.includes("dev.bisa.bisa") && changelog.includes("dev.bisa.desktop"), "the identifier change is said");
  const slug = repositorySlug(read("Cargo.toml"));
  assert.ok(changelog.includes(`[Unreleased]: https://github.com/${slug}/compare/v${VERSION}...HEAD`), "the compare links at the foot");
  const security = read("SECURITY.md");
  assert.ok(security.startsWith("# Security policy") && security.includes(`https://github.com/${slug}/security/advisories/new`) && security.includes("shasum -a 256 -c"));
  for (const recipe of ["release-macos:\n    ./scripts/release-macos.sh", "release-macos-rehearsal:\n    ./scripts/release-macos.sh --skip-notarize --allow-dirty", "publish-release *ARGS:\n    ./scripts/publish-release.sh {{ARGS}}", "set-version VERSION", "release-check:\n    node --test scripts/release/releaseModel.test.mjs"]) assert.ok(justfile.includes(recipe), `the recipe ${recipe.split(":")[0]}`);
  assert.match(justfile, /^verify: .*\brelease-check\b/m, "the release gate is in verify");
  const ci = read(".github/workflows/verify.yml");
  assert.ok(ci.includes("node --test scripts/release/releaseModel.test.mjs") && ci.includes("bash -n scripts/macos/lib.sh scripts/bundle-macos.sh scripts/release-macos.sh scripts/publish-release.sh"), "CI runs the same gate");
});
