/**
 * The words and the states of Settings › Git & code hosts › SSH keys (ide/04,
 * ide/13), pure over the node's `GET /git/ssh`: one card per public key with
 * whether ssh-agent holds it, the last check a person ran on it and the one
 * next step; ssh-agent itself with a remedy when it is down; the git hosts a
 * check or a Host block can name; and what a handshake, a resolution or a
 * new key came back as. Public material only — a private key never reaches
 * this file, a route or a screen.
 */

import { agoWords } from "./loadModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** Why the platform sets no passphrase, said beside *Generate key*. */
export const PASSPHRASE_NOTE =
  t("settings-ssh-key-written-without-passphrase-one-typed");

/** The command that starts ssh-agent in a terminal, offered when it is not running. */
export const AGENT_START_COMMAND = 'eval "$(ssh-agent -s)"'; // a shell command, never translated

/**
 * Where each git host the platform knows keeps a person's SSH keys — a
 * public page the browser opens; the platform never registers a key for
 * anyone. A self-hosted host has no page here and is pasted into by hand.
 */
const HOST_KEY_PAGES = Object.freeze({
  "github.com": "https://github.com/settings/ssh/new",
  "gitlab.com": "https://gitlab.com/-/user_settings/ssh_keys",
  "bitbucket.org": "https://bitbucket.org/account/settings/ssh-keys/",
  "codeberg.org": "https://codeberg.org/user/settings/keys",
});

/**
 * The SSH-keys page of a host, or `null` for one the platform has no page for.
 * @param {string} host
 */
export function hostKeyPage(host) {
  return HOST_KEY_PAGES[String(host ?? "").trim().toLowerCase()] ?? null;
}

/** Every host with a page, for the *Add to a host* menu. */
export function hostPages() {
  return Object.entries(HOST_KEY_PAGES).map(([host, url]) => ({ host, url }));
}

/**
 * `SHA256:abcd…wxyz` — the fingerprint short enough for a row, whole in the tooltip.
 * @param {string} fingerprint
 */
export function shortFingerprint(fingerprint) {
  const f = String(fingerprint ?? "");
  const body = f.startsWith("SHA256:") ? f.slice(7) : f;
  if (body.length <= 16) return f;
  return `${f.startsWith("SHA256:") ? "SHA256:" : ""}${body.slice(0, 8)}…${body.slice(-6)}`;
}

/** `ssh-ed25519` → `ed25519`, `ecdsa-sha2-nistp256` → `ecdsa`, `sk-…@openssh.com` → `… (security key)`. */
export function algorithmWords(algorithm) {
  const a = String(algorithm ?? "");
  const sk = a.startsWith("sk-");
  const base = a.replace(/^sk-/, "").replace(/@openssh\.com$/, "").replace(/^ssh-/, "").replace(/^ecdsa-sha2-.*/, "ecdsa");
  return sk ? t("settings-ssh-security-key", { base }) : base;
}

/**
 * The key rows, as the cards draw them.
 * @param {import("../../types").SshOverview | null | undefined} overview
 */
export function keyRows(overview) {
  if (!overview) return [];
  return overview.keys.map((k) => ({
    name: k.name,
    path: String(k.path),
    algorithm: algorithmWords(k.algorithm),
    fingerprint: k.fingerprint,
    shortFingerprint: shortFingerprint(k.fingerprint),
    comment: k.comment,
    loaded: k.loaded,
    publicLine: k.public_line,
    loadedWords: overview.agent.available ? (k.loaded ? "loaded in ssh-agent" : t("settings-ssh-loaded")) : "ssh-agent unavailable",
  }));
}

/**
 * The ssh-agent card: a tone, a sentence, and the command to copy when it is
 * not running — the one remedy the panel can offer, since nothing here starts
 * a process on a person's behalf.
 * @param {import("../../types").AgentState} agent
 * @returns {{ tone: "ok" | "warn" | "quiet", text: string, remedy: string | null }}
 */
export function agentWords(agent) {
  if (!agent.available) {
    return {
      tone: "warn",
      text: t("settings-ssh-ssh-agent-available-available-keys-cannot", { reason: agent.reason, flag: (agent.reason) ? "yes" : "no" }),
      remedy: AGENT_START_COMMAND,
    };
  }
  const n = agent.keys.length;
  return { tone: n > 0 ? "ok" : "quiet", text: t("settings-ssh-agent-holds-keys", { n }), remedy: null };
}

/**
 * The `Host` blocks for git hosts, one row each: the patterns, the host they
 * reach, the identity they name.
 * @param {readonly import("../../types").HostBlock[]} hosts
 */
export function hostRows(hosts) {
  return (hosts ?? []).map((h) => {
    const option = (name) => h.options.find(([k]) => k.toLowerCase() === name)?.[1] ?? null;
    return {
      patterns: h.patterns.join(" "),
      hostname: option("hostname"),
      user: option("user"),
      identity: option("identityfile"),
      identitiesOnly: (option("identitiesonly") ?? "").toLowerCase() === "yes",
    };
  });
}

/**
 * The hosts a check or a resolution can name: the git hosts the node knows,
 * then the hosts the person's own `Host` blocks reach — each once, in that
 * order. Free text is still allowed beside them.
 * @param {import("../../types").SshOverview | null | undefined} overview
 * @returns {string[]}
 */
export function hostChoices(overview) {
  const out = [];
  const add = (h) => {
    const host = String(h ?? "").trim().toLowerCase();
    if (host && !out.includes(host)) out.push(host);
  };
  for (const h of overview?.known_git_hosts ?? []) add(h);
  for (const r of hostRows(overview?.hosts ?? [])) add(r.hostname);
  return out;
}

/**
 * A handshake's answer, in words.
 * @param {import("../../types").HostGreeting} greeting
 * @param {string} host
 * @returns {{tone: "ok" | "warn" | "danger", text: string}}
 */
export function greetingWords(greeting, host) {
  switch (greeting.state) {
    case "authenticated":
      return { tone: "ok", text: greeting.login ? t("settings-ssh-knows-key", { host, login: greeting.login }) : t("settings-ssh-took-key", { host }) };
    case "refused":
      return { tone: "danger", text: t("settings-ssh-refused-every-key-offered-add-public", { host, reason: greeting.reason }) };
    case "host_key_unknown":
      return { tone: "warn", text: t("settings-ssh-s-host-key-known-hosts-connect", { host }) };
    default:
      return { tone: "warn", text: t("settings-ssh-unreachable", { host, reason: greeting.reason }) };
  }
}

/**
 * A check pinned to a key for this session: which host, what it said, when.
 * The platform's own memory, never ssh's — nothing is written on either side.
 * @template {Record<string, { host: string, greeting: import("../../types").HostGreeting, at: number }>} C
 * @param {C} checks
 * @param {string} name
 * @param {string} host
 * @param {import("../../types").HostGreeting} greeting
 * @param {number} at unix seconds
 */
export function rememberCheck(checks, name, host, greeting, at) {
  return { ...(checks ?? {}), [name]: { host, greeting, at } };
}

/**
 * A pinned check, in words: the greeting's sentence and tone, and when.
 * @param {{ host: string, greeting: import("../../types").HostGreeting, at: number }} check
 * @param {number} [now] unix seconds
 */
export function checkWords(check, now = Date.now() / 1000) {
  const g = greetingWords(check.greeting, check.host);
  return { tone: g.tone, text: g.text, when: t("settings-ssh-checked", { ago: agoWords(now - check.at) }) };
}

/**
 * The one next step for a key — a sentence and the action that does it.
 * The path is: put the public key on the host, check that the host knows
 * it, bind it to an organization in a profile. ssh-agent is beside the path,
 * not on it: a profile binds the key file itself.
 * @param {{ check: { host: string, greeting: import("../../types").HostGreeting } | null }} card
 * @returns {{ kind: "register" | "check" | "host_key" | "ready", text: string, action: "check" | "host" | "profile" | null }}
 */
export function nextStep(card) {
  const c = card.check;
  if (!c) return { kind: "register", text: t("settings-ssh-copy-public-key-account-host-then"), action: "check" };
  switch (c.greeting.state) {
    case "authenticated":
      return { kind: "ready", text: t("settings-ssh-knows-key-bind-organization-profile-every", { host: c.host }), action: "profile" };
    case "refused":
      return { kind: "register", text: t("settings-ssh-does-know-key-yet-add-public", { host: c.host }), action: "host" };
    case "host_key_unknown":
      return { kind: "host_key", text: t("settings-ssh-s-host-key-known-hosts-yet", { host: c.host }), action: null };
    default:
      return { kind: "check", text: t("settings-ssh-unreachable-check-again-when-online", { host: c.host }), action: "check" };
  }
}

/**
 * The cards: every key row with its pinned check and its next step.
 * @param {import("../../types").SshOverview | null | undefined} overview
 * @param {Record<string, { host: string, greeting: import("../../types").HostGreeting, at: number }> | null | undefined} checks
 */
export function keyCards(overview, checks) {
  return keyRows(overview).map((row) => {
    const card = { ...row, check: checks?.[row.name] ?? null };
    return { ...card, next: nextStep(card) };
  });
}

/**
 * What ssh would do for a host, offline, in one sentence — the key files by
 * the names this panel knows them under.
 * @param {string} host
 * @param {import("../../types").SshResolved} resolved
 * @param {readonly { name: string, path: string }[]} keys
 */
export function resolvedWords(host, resolved, keys) {
  const where = resolved.hostname && resolved.hostname !== host ? `${host} (${resolved.hostname})` : host;
  const names = (resolved.identity_files ?? []).map((p) => (keys ?? []).find((k) => k.path === p)?.name ?? p);
  const port = resolved.port && resolved.port !== 22 ? ` ${t("settings-ssh-port", { port: resolved.port })}` : "";
  if (names.length === 0) return t("settings-ssh-ssh-would-offer-key-file-ssh", { where, user: resolved.user, port });
  return t("settings-ssh-would-offer", { where, names: names.join(", "), only: resolved.identities_only ? "yes" : "no", user: resolved.user, port });
}

/** One busy mark per action per key, so loading one key never disables another's buttons. */
export function busyKey(action, name) {
  return `${action}:${name}`;
}

/**
 * A key name suggested for an owner: `id_ed25519_<owner>`, in the slug alphabet.
 * @param {string} owner
 */
export function suggestKeyName(owner) {
  const tail = String(owner ?? "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return tail ? `id_ed25519_${tail}` : "id_ed25519";
}

/**
 * The refusal for a key name, ahead of the round trip — the node's rule:
 * letters, digits, dots, dashes and underscores, not starting with a dash or
 * a dot, no separator, not ending in `.pub`.
 * @param {string} name
 * @param {readonly string[]} taken the names already in the directory
 */
export function validateKeyName(name, taken) {
  const n = String(name ?? "").trim();
  if (!n) return t("settings-ssh-file-name-key-pair");
  if (n.length > 100) return t("settings-ssh-most-100-characters");
  if (/^[-.]/.test(n)) return t("settings-ssh-cannot-start-dash-dot");
  if (n.endsWith(".pub")) return t("settings-ssh-name-private-key-s-pub-added");
  if (!/^[A-Za-z0-9._-]+$/.test(n)) return t("settings-ssh-letters-digits-dots-dashes-underscores-only");
  if ((taken ?? []).includes(n)) return t("settings-ssh-already-exists-here", { n });
  return null;
}

/**
 * The `Host` block a person pastes into their own `~/.ssh/config` — the same
 * text the node's `host_block_text` writes.
 */
export function hostBlockText(alias, hostname, user, key) {
  let text = `Host ${alias}\n    HostName ${hostname}\n    User ${user}\n`; // for the machine
  if (key) text += `    IdentityFile ${key}\n    IdentitiesOnly yes\n`; // for the machine
  return text;
}

/** The toasts. */
export function generatedWords(key) {
  return t("settings-ssh-generated-copy-public-key-account-code", { key: key.name, fingerprint: key.fingerprint });
}
export function loadedWords(name) {
  return t("settings-ssh-loaded-into-agent", { name });
}
export function copiedForHostWords(host) {
  return t("settings-ssh-public-key-copied-paste-then-check", { host });
}
