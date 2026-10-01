/**
 * Remote URLs as people read them: `git@github.com:owner/repo.git`
 * and `https://github.com/owner/repo` are one repository, and the card
 * says `github.com · owner/repo` for both — while the **protocol** says how a
 * push will reach it, and so which credential it will hand over: a token or
 * git's helper over HTTPS, a key over SSH. Parsing is for display and for a
 * gentle check before a Save; git remains the authority on what a URL is, and
 * the node's `RemoteUrl` (`crates/bisa-codehost/src/lib.rs`) is the one
 * the engine reads — this mirror is checked against its protocol words.
 */

import { t } from "../../i18n/l10n.mjs";

/**
 * The protocol words, as the Rust `RemoteProtocol` spells them on the wire.
 * `scp` is the `git@host:owner/name` spelling — SSH too, without a scheme —
 * which is how an SSH `Host` alias appears in a remote.
 */
export const REMOTE_PROTOCOLS = Object.freeze(["https", "ssh", "scp", "local", "other"]);

/**
 * The protocol, host, owner and name in a remote URL, when it has that shape.
 * Accepts `https://host/owner/name[.git]`, `ssh://git@host[:port]/owner/name`,
 * `git@host:owner/name[.git]` — the owner every segment but the last, so a
 * subgroup is one repository — and a local path (host `""`, protocol `local`,
 * the last two components as owner/name when there are two).
 * @param {string | null | undefined} url
 * @returns {{protocol: string, host: string, owner: string, name: string} | null}
 */
export function parseRemote(url) {
  const raw = (url ?? "").trim();
  if (!raw) return null;
  let protocol = "local";
  let host = "";
  let path = "";
  const file = raw.match(/^file:\/\/(.+)$/i);
  const scheme = raw.match(/^([a-z][a-z0-9+.-]*):\/\/(?:[^@/]+@)?([^/:]+)(?::\d+)?\/(.+)$/i);
  const scp = raw.match(/^(?:[^@/]+@)?([^:/]+):(?!\/\/)(.+)$/);
  if (file) {
    path = file[1];
  } else if (scheme) {
    const s = scheme[1].toLowerCase();
    protocol = s === "https" || s === "http" ? "https" : s === "ssh" || s === "git+ssh" || s === "ssh+git" ? "ssh" : s === "file" ? "local" : "other";
    host = protocol === "local" ? "" : scheme[2].toLowerCase();
    path = scheme[3];
  } else if (raw.includes("://")) {
    // A URL with a scheme and no repository path — `https://github.com/` —
    // is not a local path either.
    return null;
  } else if (scp && !raw.startsWith("/") && !/^[a-zA-Z]:[\\/]/.test(raw)) {
    protocol = "scp";
    host = scp[1].toLowerCase();
    path = scp[2];
  } else {
    path = raw.replace(/\\/g, "/");
  }
  const parts = path
    .replace(/\.git\/?$/, "")
    .replace(/\/+$/, "")
    .split("/")
    .filter(Boolean);
  if (parts.length < 2) return parts.length === 1 && !host ? { protocol, host, owner: "", name: parts[0] } : null;
  const name = parts[parts.length - 1];
  // On a host the owner is the whole namespace, as the node's `RemoteUrl`
  // reads it — a GitLab group with its subgroups is one owner, so
  // `acme/platform/web` is `acme/platform` · `web`. A local path has no
  // namespace: its folder is enough to tell two apart.
  const owner = host ? parts.slice(0, -1).join("/") : parts[parts.length - 2];
  return { protocol, host, owner, name };
}

/** `github.com · owner/repo`, or the URL itself when it has no such shape. */
export function remoteSummary(url) {
  const p = parseRemote(url);
  if (!p) return (url ?? "").trim();
  const repo = p.owner ? `${p.owner}/${p.name}` : p.name;
  return p.host ? `${p.host} · ${repo}` : repo;
}

/**
 * The protocol as a chip reads it: the word, and whether it reaches a host
 * over SSH (a key), over HTTPS (a token or git's helper), or not at all.
 * @param {string | null | undefined} protocol
 * @returns {{label: string, transport: "ssh" | "https" | "local" | "other"}}
 */
export function protocolWords(protocol) {
  switch (protocol) {
    case "https":
      return { label: "HTTPS", transport: "https" };
    case "ssh":
    case "scp":
      return { label: "SSH", transport: "ssh" };
    case "local":
      return { label: t("work-remote-local"), transport: "local" };
    default:
      return { label: protocol ? String(protocol) : "—", transport: "other" };
  }
}

/**
 * Whether a string can be handed to `git remote set-url`: non-empty, one
 * line, not option-shaped (the node refuses the same). Git decides the rest.
 * @param {string | null | undefined} url
 */
export function isRemoteUrl(url) {
  const raw = (url ?? "").trim();
  return raw.length > 0 && !raw.startsWith("-") && !/[\s\0]/.test(raw);
}
