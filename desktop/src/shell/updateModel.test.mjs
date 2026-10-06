/**
 * Update, as words. Run with `node --test --import ./src/i18n/preload.mjs desktop/src/shell/updateModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { changelogUrl, compareVersions, notesOf, parseVersion, releaseLinks, releasesIndex, tagFor, updateState, updateWords, versionLine, waitWords } from "./updateModel.mjs";

const REPO = "https://github.com/mourad-ghafiri/Bisa";

const release = (over = {}) => ({
  tag: "v0.3.0",
  version: "0.3.0",
  name: "Bisa 0.3.0",
  published_at: 1_791_622_800,
  url: `${REPO}/releases/tag/v0.3.0`,
  notes: "### Added\n\n- The Update dialog.\n\n---\n\nBuilt from commit `abc`.\n",
  prerelease: false,
  assets: [{ name: "Bisa-0.3.0-macos-universal.dmg", url: `${REPO}/releases/download/v0.3.0/Bisa-0.3.0-macos-universal.dmg`, size: 1 }],
  ...over,
});
const latest = (over = {}) => ({ state: "latest", release: release(over), checked_at: 1_791_700_000 });

test("a version is three numbers, a v tolerated, a prerelease kept and build metadata dropped", () => {
  assert.deepEqual(parseVersion("0.2.0"), { major: 0, minor: 2, patch: 0, pre: [] });
  assert.deepEqual(parseVersion("v1.10.3"), { major: 1, minor: 10, patch: 3, pre: [] });
  assert.deepEqual(parseVersion("0.3.0-rc.1+build.7"), { major: 0, minor: 3, patch: 0, pre: ["rc", "1"] });
  for (const bad of ["", "v", "1.2", "01.2.3", "1.2.3-", "latest", 3, null, undefined, "1.2.3.4"]) {
    assert.equal(parseVersion(bad), null, String(bad));
  }
});

test("versions order as semver does — a prerelease before its release, identifiers numeric then lexical", () => {
  assert.equal(compareVersions("0.3.0", "0.2.0"), 1);
  assert.equal(compareVersions("0.2.0", "0.3.0"), -1);
  assert.equal(compareVersions("v0.2.0", "0.2.0"), 0);
  assert.equal(compareVersions("1.0.0", "0.99.99"), 1);
  assert.equal(compareVersions("0.2.10", "0.2.9"), 1);
  assert.equal(compareVersions("0.3.0-rc.1", "0.3.0"), -1, "a prerelease is older than its release");
  assert.equal(compareVersions("0.3.0", "0.3.0-rc.1"), 1);
  assert.equal(compareVersions("0.3.0-rc.2", "0.3.0-rc.10"), -1, "numeric identifiers compare by value");
  assert.equal(compareVersions("0.3.0-alpha", "0.3.0-beta"), -1);
  assert.equal(compareVersions("0.3.0-1", "0.3.0-alpha"), -1, "a number before a word");
  assert.equal(compareVersions("0.3.0-rc", "0.3.0-rc.1"), -1, "the shorter list is older when the shared part agrees");
  assert.equal(compareVersions("nope", "0.2.0"), 0, "what is not a version compares as equal; the caller parses first");
});

test("the tag is the release scripts' v-prefixed one, and the doors are https only", () => {
  assert.equal(tagFor("0.3.0"), "v0.3.0");
  assert.equal(changelogUrl(REPO, "v0.3.0"), `${REPO}/blob/v0.3.0/CHANGELOG.md`);
  assert.equal(changelogUrl(`${REPO}/`, "v0.3.0"), `${REPO}/blob/v0.3.0/CHANGELOG.md`, "a trailing slash is no second slash");
  assert.equal(changelogUrl("http://example.com/x", "v0.3.0"), null);
  assert.equal(changelogUrl(undefined, "v0.3.0"), null);
  assert.equal(releasesIndex(REPO), `${REPO}/releases`);
  assert.equal(releasesIndex(""), null);
  assert.deepEqual(releaseLinks(release(), REPO), [
    { id: "release", label: "Open release on GitHub", url: `${REPO}/releases/tag/v0.3.0` },
    { id: "changelog", label: "What changed", url: `${REPO}/blob/v0.3.0/CHANGELOG.md` },
  ]);
  assert.deepEqual(
    releaseLinks(release(), undefined).map((l) => l.id),
    ["release"],
    "no repository baked in: the release page alone",
  );
  assert.deepEqual(releaseLinks({ url: "http://insecure", tag: "v1" }, REPO).map((l) => l.id), ["changelog"]);
});

test("the notes stop at the first rule, so the provenance footer stays on the release page", () => {
  assert.equal(notesOf(release().notes), "### Added\n\n- The Update dialog.");
  assert.equal(notesOf("### Fixed\n\n- A thing.\n\n***\nfooter"), "### Fixed\n\n- A thing.");
  assert.equal(notesOf("Just words, no rule.\n"), "Just words, no rule.");
  assert.equal(notesOf("   \n---\nfooter"), null, "nothing before the rule is nothing");
  assert.equal(notesOf(null), null);
  assert.equal(notesOf(undefined), null);
});

test("the state is checking until the node answers, and the node's own failure is said as the node's", () => {
  assert.deepEqual(updateState({ app: "0.2.0", check: null, loading: true, error: null }), { kind: "checking", app: "0.2.0" });
  assert.deepEqual(updateState({ app: "0.2.0", check: null, loading: false, error: null }), { kind: "checking", app: "0.2.0" });
  assert.deepEqual(updateState({ app: "0.2.0", check: latest(), loading: true, error: null }), { kind: "checking", app: "0.2.0" }, "a read in flight is checking, whatever was held");
  assert.deepEqual(updateState({ app: "0.2.0", check: null, loading: false, error: "node unreachable" }), {
    kind: "failed",
    app: "0.2.0",
    reason: "node",
    retryInSecs: null,
    checkedAt: null,
  });
});

test("a newer release is available with its notes and doors; the same is current; an older is ahead", () => {
  const available = updateState({ app: "0.2.0", check: latest(), loading: false, error: null });
  assert.deepEqual(available, {
    kind: "available",
    app: "0.2.0",
    version: "0.3.0",
    tag: "v0.3.0",
    name: "Bisa 0.3.0",
    publishedAt: 1_791_622_800,
    url: `${REPO}/releases/tag/v0.3.0`,
    notes: "### Added\n\n- The Update dialog.",
    checkedAt: 1_791_700_000,
  });
  assert.deepEqual(updateState({ app: "0.3.0", check: latest(), loading: false, error: null }), {
    kind: "current",
    app: "0.3.0",
    version: "0.3.0",
    checkedAt: 1_791_700_000,
  });
  assert.deepEqual(updateState({ app: "0.4.0", check: latest(), loading: false, error: null }), {
    kind: "ahead",
    app: "0.4.0",
    latest: "0.3.0",
    checkedAt: 1_791_700_000,
  });
  assert.equal(updateState({ app: "0.2.0", check: latest({ name: null, published_at: null, notes: null }), loading: false, error: null }).publishedAt, null);
  assert.equal(
    updateState({ app: "0.2.0", check: latest({ version: "latest" }), loading: false, error: null }).reason,
    "unexpected",
    "a tag that is not a version is an unexpected answer",
  );
});

test("no release, off and each failure are their own states, a rate limit carrying its wait", () => {
  assert.deepEqual(updateState({ app: "0.2.0", check: { state: "no_release", checked_at: 7 }, loading: false, error: null }), { kind: "none", app: "0.2.0", checkedAt: 7 });
  assert.deepEqual(updateState({ app: "0.2.0", check: { state: "off" }, loading: false, error: null }), { kind: "off", app: "0.2.0" });
  const limited = updateState({ app: "0.2.0", check: { state: "failed", failure: { kind: "rate_limited", retry_in_secs: 42 }, checked_at: 7 }, loading: false, error: null });
  assert.deepEqual(limited, { kind: "failed", app: "0.2.0", reason: "rate_limited", retryInSecs: 42, checkedAt: 7 });
  const unreachable = updateState({ app: "0.2.0", check: { state: "failed", failure: { kind: "unreachable", reason: "connection refused" }, checked_at: 7 }, loading: false, error: null });
  assert.equal(unreachable.reason, "unreachable");
  assert.equal(unreachable.retryInSecs, null);
  assert.equal(updateState({ app: "0.2.0", check: { state: "failed", failure: { kind: "unexpected", status: 500 }, checked_at: 7 }, loading: false, error: null }).reason, "unexpected");
});

test("every state has words, the version line names the latest once it is known, and a wait is seconds or minutes", () => {
  const s = (check, app = "0.2.0") => updateState({ app, check, loading: false, error: null });
  assert.deepEqual(updateWords({ kind: "checking", app: "0.2.0" }), { line: "Asking GitHub for the latest release…", detail: null });
  assert.deepEqual(updateWords(s(latest())), {
    line: "Bisa 0.3.0 is out — this desktop is 0.2.0.",
    detail: "Open the release on GitHub to download it; what changed is below.",
  });
  assert.deepEqual(updateWords(s(latest(), "0.3.0")), { line: "This is the latest release.", detail: "Bisa 0.3.0 is the newest release on GitHub." });
  assert.equal(updateWords(s(latest(), "0.4.0")).line, "This desktop (0.4.0) is newer than the latest release (0.3.0) — a build from the source.");
  assert.equal(updateWords(s({ state: "no_release", checked_at: 1 })).line, "No release has been published yet.");
  assert.equal(updateWords(s({ state: "off" })).line, "This node was started without a place to ask, so nothing was checked.");
  assert.equal(updateWords(s({ state: "failed", failure: { kind: "unreachable", reason: "x" }, checked_at: 1 })).line, "GitHub did not answer — check the connection, then try again.");
  assert.equal(
    updateWords(s({ state: "failed", failure: { kind: "rate_limited", retry_in_secs: 42 }, checked_at: 1 })).line,
    "GitHub is not answering unsigned requests from this address for now — try again in 42 seconds.",
  );
  assert.equal(
    updateWords(s({ state: "failed", failure: { kind: "rate_limited", retry_in_secs: 600 }, checked_at: 1 })).line,
    "GitHub is not answering unsigned requests from this address for now — try again in 10 minutes.",
  );
  assert.equal(
    updateWords(s({ state: "failed", failure: { kind: "rate_limited" }, checked_at: 1 })).line,
    "GitHub is not answering unsigned requests from this address for now — try again later.",
  );
  assert.equal(updateWords(s({ state: "failed", failure: { kind: "unexpected", status: 500 }, checked_at: 1 })).line, "GitHub answered something unexpected.");
  assert.equal(updateWords(updateState({ app: "0.2.0", check: null, loading: false, error: "x" })).line, "The node did not answer the check.");
  assert.equal(versionLine(s(latest())), "Bisa 0.2.0 · latest 0.3.0");
  assert.equal(versionLine(s(latest(), "0.4.0")), "Bisa 0.4.0 · latest 0.3.0");
  assert.equal(versionLine({ kind: "checking", app: "0.2.0" }), "Bisa 0.2.0");
  assert.equal(versionLine(s({ state: "off" })), "Bisa 0.2.0");
  assert.equal(waitWords(1), "in 1 seconds".replace("1 seconds", "1 seconds"));
  assert.equal(waitWords(90), "in 90 seconds");
  assert.equal(waitWords(120), "in 2 minutes");
  assert.equal(waitWords(3_600), "in 60 minutes");
});
