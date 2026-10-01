/**
 * About Bisa, as words: the platform's name and the two versions that
 * matter — this desktop's, baked in at build time, and the node's, read from
 * `GET /health` — the workspace's data directory and the person's npub, each
 * a labelled row; a caution when the two versions disagree; and the two
 * doors out — the website and the source — from the URLs the build baked in
 * beside the version (`package.json`'s `homepage` and `repository`, the same
 * two the Cargo workspace declares), and the two files the app ships with —
 * the licence and the third-party notices — revealed where they are on this
 * machine (`licence_files`). `AboutDialog.tsx` draws; `node --test` checks.
 */

import { t } from "../i18n/l10n.mjs";

export const PRODUCT = "Bisa";

/** *Bisa 0.1.0 · node 0.1.0*, or *· node unreachable* while the node has not answered. */
export function versionLine(app, node) {
  return node ? t("shell-about-version-line", { product: PRODUCT, app, node }) : t("shell-about-version-line-unreachable", { product: PRODUCT, app });
}

/** The mismatch, when both are known and differ; `null` otherwise. */
export function versionCaution(app, node) {
  if (!node || node === app) return null;
  return t("shell-about-node-desktop-restart-desktop-so-both", { node, app });
}

/**
 * The doors out of About: the website, then the source. Each an `https://`
 * URL the build baked in; a blank one is no door.
 * @param {string} homepage
 * @param {string} repository
 * @returns {{id: "website" | "source", label: string, url: string}[]}
 */
export function aboutLinks(homepage, repository) {
  const links = [];
  if (isHttps(homepage)) links.push({ id: "website", label: t("shell-about-website"), url: homepage });
  if (isHttps(repository)) links.push({ id: "source", label: t("shell-about-source"), url: repository });
  return links;
}

function isHttps(url) {
  return typeof url === "string" && url.startsWith("https://");
}

/**
 * The files About reveals: the licence, then the third-party notices — each
 * only where the shell found it on this machine.
 * @param {{licence?: string | null, notices?: string | null} | null | undefined} files
 * @returns {{id: "licence" | "notices", label: string, path: string}[]}
 */
export function aboutFiles(files) {
  const out = [];
  if (files?.licence) out.push({ id: "licence", label: t("shell-about-licence"), path: files.licence });
  if (files?.notices) out.push({ id: "notices", label: t("shell-about-third-party-notices"), path: files.notices });
  return out;
}

/** The one sentence about the marks: ours is ours, the harnesses' are theirs, and where their licences are. */
export function marksWords() {
  return t("shell-about-marks-notices", { product: PRODUCT });
}

/**
 * @typedef {{label: string, value: string, copy: boolean}} AboutRow
 */

/**
 * The rows of the dialog, in order: the versions, the data directory, the
 * person's npub. A value a person may want elsewhere is marked `copy`.
 * @param {{app: string, node: string | null | undefined, dataDir: string | null | undefined, npub: string | null | undefined}} facts
 * @returns {AboutRow[]}
 */
export function aboutRows({ app, node, dataDir, npub }) {
  const rows = [
    { label: t("shell-about-desktop"), value: app, copy: false },
    { label: t("shell-node-overlay-node"), value: node ? node : "unreachable", copy: false },
  ];
  if (dataDir) rows.push({ label: t("shell-about-data-directory"), value: dataDir, copy: true });
  if (npub) rows.push({ label: t("shell-profile-menu-words"), value: npub, copy: true });
  return rows;
}
