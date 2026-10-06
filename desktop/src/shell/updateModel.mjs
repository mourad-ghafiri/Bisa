/**
 * Update, as words: what the node read about the latest release on GitHub
 * (`GET /updates` — facts, never a comparison) set against the version this
 * desktop was built as, and the sentences, doors and names each outcome
 * wears. `UpdateDialog.tsx` draws; `node --test` checks.
 *
 * The comparison is semver's — `MAJOR.MINOR.PATCH`, a leading `v`
 * tolerated, a prerelease ordered before its release — because the tag the
 * release scripts cut is `v<version>` (`scripts/release/releaseModel.mjs`,
 * `tagFor`) and the version baked into this desktop is the same number
 * without the `v`. The node's own version may differ from the desktop's;
 * About says so, and the update is about the app a person would download.
 */

import { t } from "../i18n/l10n.mjs";
import { PRODUCT } from "./aboutModel.mjs";

/**
 * A version's parts, or `null` for text that is not one. A leading `v` is
 * tolerated, build metadata after a `+` ignored, a prerelease after a `-`
 * kept as its dot-separated identifiers.
 * @param {unknown} text
 * @returns {{major: number, minor: number, patch: number, pre: string[]} | null}
 */
export function parseVersion(text) {
  if (typeof text !== "string") return null;
  let s = text.trim();
  if (s.startsWith("v") || s.startsWith("V")) s = s.slice(1);
  const plus = s.indexOf("+");
  if (plus >= 0) s = s.slice(0, plus);
  const dash = s.indexOf("-");
  const core = dash >= 0 ? s.slice(0, dash) : s;
  const pre = dash >= 0 ? s.slice(dash + 1).split(".").filter(Boolean) : [];
  const m = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.exec(core);
  if (!m) return null;
  if (dash >= 0 && pre.length === 0) return null;
  return { major: Number(m[1]), minor: Number(m[2]), patch: Number(m[3]), pre };
}

/** One prerelease identifier against another: numeric ones by value and before words, words lexically. */
function compareIdentifier(a, b) {
  const an = /^\d+$/.test(a);
  const bn = /^\d+$/.test(b);
  if (an && bn) return Math.sign(Number(a) - Number(b));
  if (an) return -1;
  if (bn) return 1;
  return a < b ? -1 : a > b ? 1 : 0;
}

/**
 * `-1`, `0` or `1` as `a` is older than, the same as, or newer than `b`. A
 * prerelease is older than its release; two prereleases compare identifier
 * by identifier, the shorter list older when every shared one agrees. Text
 * that is not a version compares as equal to anything — the caller parses
 * first and says so (`updateState` answers *unexpected*).
 * @param {string} a
 * @param {string} b
 * @returns {-1 | 0 | 1}
 */
export function compareVersions(a, b) {
  const x = parseVersion(a);
  const y = parseVersion(b);
  if (!x || !y) return 0;
  for (const part of ["major", "minor", "patch"]) {
    if (x[part] !== y[part]) return x[part] < y[part] ? -1 : 1;
  }
  if (x.pre.length === 0 && y.pre.length === 0) return 0;
  if (x.pre.length === 0) return 1;
  if (y.pre.length === 0) return -1;
  const n = Math.min(x.pre.length, y.pre.length);
  for (let i = 0; i < n; i++) {
    const c = compareIdentifier(x.pre[i], y.pre[i]);
    if (c !== 0) return c;
  }
  return Math.sign(x.pre.length - y.pre.length);
}

/** The tag a release is cut under — the release scripts' rule, mirrored (`releaseModel.mjs`'s `tagFor`). */
export function tagFor(version) {
  return `v${version}`;
}

function isHttps(url) {
  return typeof url === "string" && url.startsWith("https://");
}

/**
 * The changelog as GitHub renders it, at the release's own tag — so the page
 * a person reads is the one the release shipped with, not what moved since.
 * @param {string | undefined} repository the `https://` repository URL the build baked in
 * @param {string} tag
 * @returns {string | null}
 */
export function changelogUrl(repository, tag) {
  if (!isHttps(repository) || !tag) return null;
  return `${repository.replace(/\/+$/, "")}/blob/${tag}/CHANGELOG.md`;
}

/** The releases page, for when none is published yet. */
export function releasesIndex(repository) {
  if (!isHttps(repository)) return null;
  return `${repository.replace(/\/+$/, "")}/releases`;
}

/**
 * The notes a person reads: the release's body up to its first horizontal
 * rule — the changelog section — leaving the provenance footer (the commit,
 * the checksums) on the release page where it belongs. Blank is `null`.
 * @param {string | null | undefined} body
 * @returns {string | null}
 */
export function notesOf(body) {
  if (typeof body !== "string") return null;
  const lines = body.split(/\r?\n/);
  const rule = lines.findIndex((line) => /^\s*(-{3,}|\*{3,}|_{3,})\s*$/.test(line));
  const kept = (rule >= 0 ? lines.slice(0, rule) : lines).join("\n").trim();
  return kept ? kept : null;
}

/**
 * @typedef {{ tag: string, version: string, name?: string | null, published_at?: number | null, url: string, notes?: string | null, prerelease?: boolean, assets?: {name: string, url: string, size: number}[] }} LatestRelease
 * @typedef {{ state: "latest", release: LatestRelease, checked_at: number } | { state: "no_release", checked_at: number } | { state: "off" } | { state: "failed", failure: { kind: "unreachable", reason: string } | { kind: "rate_limited", retry_in_secs?: number | null } | { kind: "unexpected", status: number }, checked_at: number }} UpdateCheck
 * @typedef {"unreachable" | "rate_limited" | "unexpected" | "node"} FailureReason
 * @typedef {{ kind: "checking", app: string } | { kind: "current", app: string, version: string, checkedAt: number } | { kind: "available", app: string, version: string, tag: string, name: string | null, publishedAt: number | null, url: string, notes: string | null, checkedAt: number } | { kind: "ahead", app: string, latest: string, checkedAt: number } | { kind: "none", app: string, checkedAt: number } | { kind: "off", app: string } | { kind: "failed", app: string, reason: FailureReason, retryInSecs: number | null, checkedAt: number | null }} UpdateState
 */

/**
 * What the dialog is in, from what the node answered: nothing yet is
 * *checking*; the node's own refusal or absence is *failed* as `node`; a
 * release is *available*, *current* or *ahead* by the comparison with this
 * desktop's version; a tag that is not a version is *unexpected*.
 * @param {{ app: string, check: UpdateCheck | null | undefined, loading: boolean, error: string | null | undefined }} facts
 * @returns {UpdateState}
 */
export function updateState({ app, check, loading, error }) {
  if (error) return { kind: "failed", app, reason: "node", retryInSecs: null, checkedAt: null };
  if (!check || loading) return { kind: "checking", app };
  switch (check.state) {
    case "off":
      return { kind: "off", app };
    case "no_release":
      return { kind: "none", app, checkedAt: check.checked_at };
    case "failed": {
      const f = check.failure;
      const retryInSecs = f.kind === "rate_limited" && typeof f.retry_in_secs === "number" ? f.retry_in_secs : null;
      return { kind: "failed", app, reason: f.kind, retryInSecs, checkedAt: check.checked_at };
    }
    case "latest": {
      const r = check.release;
      if (!parseVersion(r.version) || !parseVersion(app)) {
        return { kind: "failed", app, reason: "unexpected", retryInSecs: null, checkedAt: check.checked_at };
      }
      const c = compareVersions(r.version, app);
      if (c > 0) {
        return {
          kind: "available",
          app,
          version: r.version,
          tag: r.tag,
          name: r.name ?? null,
          publishedAt: typeof r.published_at === "number" ? r.published_at : null,
          url: r.url,
          notes: notesOf(r.notes),
          checkedAt: check.checked_at,
        };
      }
      if (c < 0) return { kind: "ahead", app, latest: r.version, checkedAt: check.checked_at };
      return { kind: "current", app, version: r.version, checkedAt: check.checked_at };
    }
    default:
      return { kind: "failed", app, reason: "unexpected", retryInSecs: null, checkedAt: null };
  }
}

/** A wait in words — *in 42 seconds*, *in 3 minutes*. */
export function waitWords(secs) {
  if (secs >= 120) return t("shell-update-wait-minutes", { n: Math.ceil(secs / 60) });
  return t("shell-update-wait-seconds", { n: Math.max(1, Math.round(secs)) });
}

/**
 * The version line under the mark — *Bisa 0.2.0 · latest 0.3.0*, or the
 * desktop alone while the latest is not known.
 * @param {UpdateState} state
 */
export function versionLine(state) {
  const latest = state.kind === "available" || state.kind === "current" ? state.version : state.kind === "ahead" ? state.latest : null;
  return latest ? t("shell-update-version-line", { product: PRODUCT, app: state.app, latest }) : t("shell-update-version-line-unknown", { product: PRODUCT, app: state.app });
}

/**
 * The headline and, when there is one, the line under it.
 * @param {UpdateState} state
 * @returns {{ line: string, detail: string | null }}
 */
export function updateWords(state) {
  switch (state.kind) {
    case "checking":
      return { line: t("shell-update-checking"), detail: null };
    case "current":
      return { line: t("shell-update-current"), detail: t("shell-update-current-detail", { product: PRODUCT, app: state.app }) };
    case "available":
      return { line: t("shell-update-available", { product: PRODUCT, latest: state.version, app: state.app }), detail: t("shell-update-available-detail") };
    case "ahead":
      return { line: t("shell-update-ahead", { app: state.app, latest: state.latest }), detail: null };
    case "none":
      return { line: t("shell-update-none"), detail: null };
    case "off":
      return { line: t("shell-update-off"), detail: null };
    case "failed":
      switch (state.reason) {
        case "unreachable":
          return { line: t("shell-update-failed-unreachable"), detail: null };
        case "rate_limited":
          return {
            line: state.retryInSecs ? t("shell-update-failed-rate-limited", { wait: waitWords(state.retryInSecs) }) : t("shell-update-failed-rate-limited-later"),
            detail: null,
          };
        case "node":
          return { line: t("shell-update-failed-node"), detail: null };
        default:
          return { line: t("shell-update-failed-unexpected"), detail: null };
      }
    default:
      return { line: t("shell-update-failed-unexpected"), detail: null };
  }
}

/**
 * The doors out of the dialog for a known release: the release page on
 * GitHub, where the notes and every asset are (the download), then the
 * changelog at the release's tag. Each only when it is an `https://` URL.
 * @param {{ url: string, tag: string }} release
 * @param {string | undefined} repository
 * @returns {{id: "release" | "changelog", label: string, url: string}[]}
 */
export function releaseLinks(release, repository) {
  const links = [];
  if (isHttps(release?.url)) links.push({ id: "release", label: t("shell-update-open-release"), url: release.url });
  const changelog = changelogUrl(repository, release?.tag);
  if (changelog) links.push({ id: "changelog", label: t("shell-update-what-changed"), url: changelog });
  return links;
}
