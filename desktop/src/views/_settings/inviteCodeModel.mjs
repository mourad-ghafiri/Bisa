/**
 * The invite code as the desktop reads and writes it (14-collaboration):
 * the link form `bisa://join/<nprofile>/<secret>` a deep link carries and
 * the text form `<nprofile>:<secret>` a terminal pastes. The node parses
 * either on `POST /hosts/join`; this only tells a field whether what was
 * pasted looks like one, and turns one form into the other for display.
 */

import { t } from "../../i18n/l10n.mjs";

const LINK_PREFIX = "bisa://join/";

/**
 * What a pasted string is: a link, a text code, or nothing usable — with
 * the two halves when it is one.
 * @param {string} raw
 * @returns {{ kind: "link" | "code", nprofile: string, secret: string } | { kind: "none", reason: string }}
 */
export function parseInviteCode(raw) {
  const s = String(raw ?? "").trim();
  if (!s) return { kind: "none", reason: "" };
  const lower = s.toLowerCase();
  if (lower.startsWith(LINK_PREFIX)) {
    const rest = s.slice(LINK_PREFIX.length).replace(/\/+$/, "");
    const i = rest.indexOf("/");
    if (i <= 0 || i === rest.length - 1) return { kind: "none", reason: t("settings-invite-code-link-shape") };
    const nprofile = rest.slice(0, i);
    const secret = rest.slice(i + 1);
    return checked("link", nprofile, secret);
  }
  const i = s.lastIndexOf(":");
  if (i <= 0 || i === s.length - 1) return { kind: "none", reason: t("settings-invite-code-code-shape") };
  return checked("code", s.slice(0, i), s.slice(i + 1));
}

function checked(kind, nprofile, secret) {
  if (!/^nprofile1[a-z0-9]+$/i.test(nprofile)) return { kind: "none", reason: t("settings-invite-code-host-part-nprofile") };
  if (!/^[0-9a-f]+$/i.test(secret)) return { kind: "none", reason: t("settings-invite-code-secret-hex") };
  return { kind, nprofile, secret: secret.toLowerCase() };
}

/** The link form of a parsed code. */
export function inviteLink(parsed) {
  return `${LINK_PREFIX}${parsed.nprofile}/${parsed.secret}`;
}

/**
 * The code a deep link carried, when it did: `bisa://join/…` arrives as the
 * URL the OS opened the app with.
 * @param {readonly string[] | null | undefined} urls
 */
export function joinCodeFromUrls(urls) {
  for (const u of urls ?? []) {
    const parsed = parseInviteCode(u);
    if (parsed.kind === "link") return inviteLink(parsed);
  }
  return null;
}
