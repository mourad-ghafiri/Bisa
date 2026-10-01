/**
 * The macOS build as the sources show it (contributing/release.md): one
 * library, `scripts/macos/lib.sh`, builds the node for both architectures and
 * joins it, builds the shell for `universal-apple-darwin` with the Tauri
 * CLI's signing skipped, assembles the bundle and signs it inside-out with
 * the hardened runtime and the one entitlement — a timestamp whenever the
 * identity is not ad hoc, never `--deep` for signing; `scripts/bundle-macos.sh`
 * is the app for this Mac — ad hoc, no notarization, no environment variable
 * — and `scripts/start-macos.sh` runs it plainly. The release is
 * `release.test.mjs`'s. Source assertions and the scripts parsed by bash; no
 * build. Run with `node --test desktop/src/scenarios/bundle.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, readdirSync, existsSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
const read = (rel) => readFileSync(join(root, rel), "utf8");
const lib = read("scripts/macos/lib.sh");
const bundle = read("scripts/bundle-macos.sh");
const start = read("scripts/start-macos.sh");
const justfile = read("Justfile");
/** The lines of a shell script that run: no comments, no blanks. */
const code = (text) => text.split("\n").filter((l) => l.trim() && !l.trim().startsWith("#"));
/** Every `moved_aside <path> <name> [ext]` call of a script, as its three words. */
const movedAside = (text) => [...text.matchAll(/^\s*moved_aside\s+(\S+)\s+(\S+)(?:\s+(\S+))?$/gm)].map((m) => [m[1], m[2], m[3] ?? "app"]);
const bash = (args, options = {}) => spawnSync("bash", args, { cwd: root, encoding: "utf8", ...options });

test("the toolchain and hakari speak both macOS architectures", () => {
  assert.match(read("rust-toolchain.toml"), /targets = \["aarch64-apple-darwin", "x86_64-apple-darwin"\]/);
  const hakari = read(".config/hakari.toml");
  assert.match(hakari, /^\s+"x86_64-apple-darwin",$/m);
  assert.match(hakari, /^\s+"aarch64-apple-darwin",$/m);
  const deps = read("crates/bisa-deps/Cargo.toml");
  for (const t of ["x86_64-apple-darwin", "aarch64-apple-darwin"]) assert.ok(deps.includes(`[target.${t}.dependencies]`), `bisa-deps unifies ${t}`);
});

test("the library builds the node for both targets and joins it, and the shell universal with the Tauri CLI's signing skipped", () => {
  assert.ok(lib.includes("TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)"));
  assert.ok(lib.includes('cargo build --profile dist -p bisa-cli --bin bisa --target "$t"'), "the node alone: the package's other binary is the tests' scripted agent");
  assert.ok(!/cp .*scripted-agent|scripted-agent.*Contents/.test(lib), "and it is never copied into the bundle");
  assert.ok(justfile.includes("cargo build --profile dist -p bisa-cli --bin bisa"), "`just dist` names the binary too");
  assert.ok(lib.includes('lipo -create -output "$node_out"') && lib.includes('lipo -archs "$node_out"'), "one universal node, checked");
  assert.ok(lib.includes("for want in x86_64 arm64; do"), "both slices or a refusal");
  assert.ok(lib.includes('npm run tauri build -- --bundles app --target "$UNIVERSAL" --no-sign') && lib.includes('UNIVERSAL="universal-apple-darwin"'));
  assert.ok(lib.includes("rustup target list --installed"), "a missing target is named before anything builds");
  assert.ok(lib.includes('plutil -lint "$entitlements"'), "the entitlements parse before anything builds");
});

test("the library signs inside-out with the hardened runtime and the entitlements; a timestamp whenever the identity is not ad hoc; never --deep for signing; the debugging entitlement refused", () => {
  const nodeSign = lib.indexOf('--timestamp --sign "$identity" "$stage/Contents/MacOS/bisa"');
  const appSign = lib.indexOf('--timestamp --entitlements "$entitlements" --sign "$identity" "$stage"');
  assert.ok(nodeSign > 0 && appSign > nodeSign, "the node before the app, with the identity");
  const adhocNode = lib.indexOf('--sign - "$stage/Contents/MacOS/bisa"');
  const adhocApp = lib.indexOf('--entitlements "$entitlements" --sign - "$stage"');
  assert.ok(adhocNode > 0 && adhocApp > adhocNode, "the node before the app, ad hoc");
  assert.ok(lib.includes('if [ "$identity" = "-" ]; then'), "one rule: the ad hoc identity is the switch");
  // Every signing line carries the hardened runtime; the identity's carry a
  // secure timestamp, which an ad hoc signature cannot and the notary needs.
  const signing = code(lib).filter((l) => l.includes("codesign") && l.includes("--sign"));
  assert.equal(signing.length, 4);
  for (const l of signing) assert.ok(l.includes("--options runtime"), `hardened runtime: ${l.trim()}`);
  for (const l of signing.filter((l) => l.includes('"$identity"'))) assert.ok(l.includes("--timestamp"), `a timestamp with the identity: ${l.trim()}`);
  for (const l of signing.filter((l) => l.includes("--sign -"))) assert.ok(!l.includes("--timestamp"), `no timestamp ad hoc: ${l.trim()}`);
  assert.ok(!lib.includes("--deep --sign"), "Apple deprecates --deep for signing; verification alone walks deep");
  assert.ok(lib.includes('codesign --verify --deep --strict --verbose=2 "$stage"'));
  assert.ok(lib.includes("com.apple.security.get-task-allow") && lib.includes("codesign -d --entitlements :-"), "the entitlement notarization refuses is looked for on what was signed");
  const signAt = lib.indexOf("sign_app() {");
  const refuseAt = lib.indexOf("refuse_debuggable() {");
  assert.ok(lib.indexOf("assemble() {") > 0 && signAt > lib.indexOf("assemble() {") && refuseAt > signAt, "assembled, then signed, then checked");
});

test("the app for this Mac is ad hoc and nothing else: no identity, no notary, no zip, no environment variable; the launcher and the recipe run it plainly", () => {
  assert.ok(bundle.includes('. "$root/scripts/macos/lib.sh"'), "the library is sourced");
  assert.ok(bundle.includes('previous_dir="$root/target/dist-previous"'), "what is moved aside goes under target/dist-previous");
  assert.ok(bundle.includes('sign_app "$stage" -'), "signed ad hoc");
  for (const word of ["APPLE_SIGNING_IDENTITY", "BISA_NOTARY_PROFILE", "BISA_SIGN", "notarytool", "stapler", "spctl", ".zip", "release/", "hdiutil"]) assert.ok(!bundle.includes(word), `the personal build knows no ${word}`);
  assert.ok(bundle.includes("--open") && bundle.includes("open \"$root/dist/Bisa.app\""), "--open launches it");
  assert.ok(bundle.includes('mv "$stage" "$root/dist/Bisa.app"') && bundle.includes("run it: scripts/start-macos.sh"));
  assert.ok(bundle.includes("scripts/release-macos.sh"), "the header says where a copy to give to someone comes from");
  assert.ok(start.includes("    scripts/bundle-macos.sh\n") && !start.includes("BISA_SIGN"), "start-macos.sh runs it plainly");
  assert.ok(justfile.includes("bundle-macos:\n    ./scripts/bundle-macos.sh") && !justfile.includes("bundle-macos-adhoc"), "one recipe, no switch");
  assert.ok(justfile.includes("start-macos:\n    ./scripts/start-macos.sh"));
});

test("the library is sourced, not run — it defines these functions and calls nothing — and every script parses", () => {
  for (const script of ["scripts/macos/lib.sh", "scripts/bundle-macos.sh", "scripts/start-macos.sh", "scripts/release-macos.sh", "scripts/publish-release.sh"]) {
    const parsed = bash(["-n", script]);
    assert.equal(parsed.status, 0, `${script} parses: ${parsed.stderr}`);
  }
  const sourced = bash(["-c", 'root=/nowhere; previous_dir=/nowhere; . scripts/macos/lib.sh; declare -F | sed "s/declare -f //"']);
  assert.equal(sourced.status, 0, sourced.stderr);
  assert.equal(sourced.stderr, "", "sourcing says nothing");
  assert.deepEqual(sourced.stdout.trim().split("\n").sort(), ["assemble", "build_node", "build_shell", "moved_aside", "refuse", "refuse_debuggable", "require_tools", "say", "sign_app", "version_of"]);
  for (const name of ["say()", "refuse()", "moved_aside()", "version_of()", "require_tools()", "build_node()", "build_shell()", "assemble()", "sign_app()", "refuse_debuggable()"]) assert.ok(lib.includes(name), `${name} is the library's`);
  // No line of the library outside a function body runs anything: every
  // top-level line is an assignment, a function's opening or its closing.
  let depth = 0;
  for (const line of code(lib)) {
    if (depth === 0) assert.ok(/^[A-Za-z_][A-Za-z0-9_]*=|^[a-z_]+\(\) \{|^\}$/.test(line), `a top-level line that is not a definition: ${line}`);
    depth += (line.match(/\{\s*$/) ? 1 : 0) - (line.match(/^\s*\}/) ? 1 : 0);
  }
});

test("moved_aside moves a path under the previous folder with a stamp, and deletes nothing", () => {
  const dir = mkdtempSync(join(tmpdir(), "bisa-moved-aside-"));
  const previous = join(dir, "previous");
  writeFileSync(join(dir, "thing.txt"), "the bytes");
  mkdirSync(join(dir, "Bisa.app"));
  writeFileSync(join(dir, "Bisa.app", "inside"), "a bundle's file");
  const run = bash(["-c", 'root=/nowhere; previous_dir="$1"; . scripts/macos/lib.sh; moved_aside "$2/thing.txt" thing txt; moved_aside "$2/Bisa.app" Bisa; moved_aside "$2/absent" absent', "_", previous, dir]);
  assert.equal(run.status, 0, run.stderr);
  assert.ok(!existsSync(join(dir, "thing.txt")) && !existsSync(join(dir, "Bisa.app")), "moved away");
  const names = readdirSync(previous).sort();
  assert.equal(names.length, 2, "two things moved, the absent one nothing");
  assert.match(names[0], /^Bisa-\d{8}T\d{6}\.app$/);
  assert.match(names[1], /^thing-\d{8}T\d{6}\.txt$/);
  assert.equal(readFileSync(join(previous, names[1]), "utf8"), "the bytes", "the bytes travelled");
  assert.equal(readFileSync(join(previous, names[0], "inside"), "utf8"), "a bundle's file");
});

test("the Tauri bundle carries the identifier, the macOS block and one entitlement; the library copies the node, the licence and the notices; nothing is deleted", () => {
  const conf = JSON.parse(read("desktop/src-tauri/tauri.conf.json"));
  assert.equal(conf.identifier, "dev.bisa.bisa", "the bundle identifier — the app's folders and its link claim are keyed by it");
  assert.deepEqual(conf.bundle.macOS, { minimumSystemVersion: "11.0", hardenedRuntime: true, entitlements: "Entitlements.plist" });
  assert.equal(conf.bundle.macOS.signingIdentity, undefined, "the identity is the person's, never the repo's");
  assert.ok(read("desktop/src-tauri/src/window_state.rs").includes('"dev.bisa.bisa"') && !read("desktop/src-tauri/src/window_state.rs").includes("dev.bisa.desktop"), "the shell's own test path follows the identifier");
  const ent = read("desktop/src-tauri/Entitlements.plist");
  assert.ok(ent.includes("<key>com.apple.security.device.audio-input</key>"), "the microphone, for voice mode");
  for (const forbidden of ["allow-jit", "allow-unsigned-executable-memory", "disable-library-validation", "automation.apple-events", "app-sandbox", "get-task-allow"]) assert.ok(!ent.includes(`<key>com.apple.security.${forbidden}</key>`) && !ent.includes(`<key>com.apple.security.cs.${forbidden}</key>`), `no ${forbidden}`);
  assert.ok(lib.includes('cp "$root/LICENSE" "$stage/Contents/Resources/LICENSE"') && lib.includes('cp "$root/THIRD-PARTY-NOTICES.md" "$stage/Contents/Resources/THIRD-PARTY-NOTICES.md"'), "the licence and the notices ride along");
  assert.ok(lib.includes('cp "$node_out" "$stage/Contents/MacOS/bisa"'), "the node where the sidecar looks first");
  // Nothing is ever removed: a previous bundle or stage is moved aside, never deleted.
  for (const [name, text] of [["lib.sh", lib], ["bundle-macos.sh", bundle], ["start-macos.sh", start]]) assert.ok(!/(^|[\s;&|])rm\s/m.test(text), `${name} never removes anything`);
  assert.deepEqual(movedAside(lib), [['"$stage"', "stage", "app"]], "the library moves the stage aside before it assembles");
  assert.deepEqual(movedAside(bundle), [['"$root/dist/Bisa.app"', "Bisa", "app"]], "the personal build moves the last bundle aside before it places");
});
