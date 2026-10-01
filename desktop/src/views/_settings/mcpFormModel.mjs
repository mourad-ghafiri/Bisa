/**
 * The MCP editor's form, as facts (06 § MCP servers): a draft for the three
 * transports — a process over stdio, Streamable HTTP, the older HTTP+SSE —
 * the transport the node takes for it, the words that refuse a bad one
 * before the node has to, and the mask rule: a secret value the node
 * answered as `••••••` is sent back as `••••••`, and the node keeps the
 * stored one. No DOM, no fetch.
 */

import { MASK, isMasked } from "../../ui/secretInputModel.mjs";
import { t as tr } from "../../i18n/l10n.mjs";

/** What every secret value reads as on the way out — `bisa_core::MCP_MASK`, word for word; the kit's one mask. */
export { MASK, isMasked };

/** The reserved server name: the platform's own. */
export const RESERVED = "bisa";

/** The three transports, in the order the picker draws them. */
export const KINDS = Object.freeze(["stdio", "http", "sse"]);

/** The picker's words for a kind: label and the one-line meaning. */
export function kindWords(kind) {
  switch (kind) {
    case "http":
      return { label: "HTTP", hint: tr("settings-mcp-form-streamable-http-one-url-harness-calls") };
    case "sse":
      return { label: "SSE", hint: tr("settings-mcp-form-2024-11-05-http-sse-transport") };
    default:
      return { label: tr("settings-mcp-form-stdio"), hint: tr("settings-mcp-form-command-machine-runs-harness-speaks-over") };
  }
}

/** A blank draft — a stdio server, nothing filled in. */
export function blank() {
  return { id: "", name: "", description: "", tags: [], kind: "stdio", command: "", args: "", env: [], cwd: "", url: "", headers: [] };
}

/** A map as the editor's rows. @param {Record<string, string> | undefined} map */
function rows(map) {
  return Object.entries(map ?? {}).map(([k, v]) => ({ k, v }));
}

/** A registered server as a draft — its secrets already masked by the node. @param {object} m */
export function fromDef(m) {
  const d = blank();
  d.id = m.id;
  d.name = m.transport.name;
  d.description = m.description ?? "";
  d.tags = m.tags ?? [];
  d.kind = m.transport.transport;
  if (m.transport.transport === "stdio") {
    d.command = m.transport.command;
    d.args = (m.transport.args ?? []).join("\n");
    d.env = rows(m.transport.env);
    d.cwd = m.transport.cwd ?? "";
  } else {
    d.url = m.transport.url;
    d.headers = rows(m.transport.headers);
  }
  return d;
}

/** The editor's rows as a map: blank keys dropped, keys trimmed. @param {{k: string, v: string}[]} pairs */
export function pairsToMap(pairs) {
  return Object.fromEntries(pairs.filter((p) => p.k.trim()).map((p) => [p.k.trim(), p.v]));
}

/**
 * One transport or the other, never both — the union is exclusive on the
 * wire. A value still `••••••` is sent as `••••••`: the node keeps the
 * stored one (`McpServerConfig::unmasked_from`).
 * @param {ReturnType<typeof blank>} d
 */
export function toTransport(d) {
  const name = d.name.trim();
  if (d.kind === "http" || d.kind === "sse") {
    const headers = pairsToMap(d.headers);
    return { transport: d.kind, name, url: d.url.trim(), ...(Object.keys(headers).length ? { headers } : {}) };
  }
  const args = d.args
    .split("\n")
    .map((a) => a.trim())
    .filter(Boolean);
  const env = pairsToMap(d.env);
  return {
    transport: "stdio",
    name,
    command: d.command.trim(),
    ...(args.length ? { args } : {}),
    ...(Object.keys(env).length ? { env } : {}),
    ...(d.cwd.trim() ? { cwd: d.cwd.trim() } : {}),
  };
}

/** An absolute http(s) URL with a host — what the node takes; the form says so first. @param {string} url */
export function isHttpUrl(url) {
  const m = /^https?:\/\/([^/?#\s]+)/.exec(url.trim());
  return !!m && m[1].length > 0;
}

/** A header name is a token (RFC 9110 §5.6.2). @param {string} name */
export function isHeaderName(name) {
  return /^[A-Za-z0-9!#$%&'*+\-.^_`|~]+$/.test(name);
}

/**
 * The form's refusals, by field — the same sentences the node answers, so
 * a person reads one vocabulary whichever side refused.
 * @param {ReturnType<typeof blank>} d
 * @param {boolean} editing
 */
export function validate(d, editing) {
  const e = {};
  if (!editing && !d.id.trim()) e.id = tr("settings-mcp-form-id-is-reference");
  if (!editing && d.id.trim() && !/^[a-z0-9][a-z0-9_.:-]{0,63}$/.test(d.id.trim())) e.id = tr("settings-mcp-form-id-shape");
  if (!d.name.trim()) e.name = tr("settings-mcp-form-name-needed");
  if (d.name.trim() === RESERVED) e.name = tr("settings-mcp-form-name-reserved", { reserved: RESERVED });
  if (d.kind === "stdio") {
    if (!d.command.trim()) e.command = tr("settings-mcp-form-nothing-to-run");
    if (d.cwd.trim() && !d.cwd.trim().startsWith("/")) e.cwd = tr("settings-mcp-form-cwd-absolute");
    const bad = d.env.find((p) => p.k.trim() && /\s|=/.test(p.k.trim()));
    if (bad) e.env = tr("settings-mcp-form-not-variable-name", { name: bad.k.trim() });
  } else {
    if (!d.url.trim()) e.url = tr("settings-mcp-form-which-url");
    else if (!isHttpUrl(d.url)) e.url = tr("settings-mcp-form-url-absolute");
    const bad = d.headers.find((p) => p.k.trim() && !isHeaderName(p.k.trim()));
    if (bad) e.headers = tr("settings-mcp-form-not-header-name", { name: bad.k.trim() });
    const ctl = d.headers.find((p) => /[\u0000-\u001f\u007f]/.test(p.v));
    if (ctl) e.headers = tr("settings-mcp-form-header-control-character", { name: ctl.k.trim() });
  }
  return e;
}

/**
 * The field each of the node's refusals is about, by the refusal's **id** —
 * the message a refusal travels as (`ErrorBody.text.id`), which is the same
 * whatever language its sentence is rendered in. The sentence itself is
 * never read: it is this window's language, and a word matched in English
 * would place nothing in any other.
 */
export const FIELD_OF_REFUSAL = Object.freeze({
  "error-core-mcp-empty-name": "name",
  "error-core-mcp-reserved-name": "name",
  "error-core-mcp-empty-command": "command",
  "error-core-mcp-relative-cwd": "cwd",
  "error-core-mcp-invalid-url": "url",
  "error-core-mcp-bad-header-name": "headers",
  "error-core-mcp-bad-header-value": "headers",
  "error-store-invalid-mcp-server-already-exists": "id",
});

/**
 * Put the node's refusal on the field it is about; one the form knows no
 * field for — or that carries no id — is the form's own.
 * @param {string | null | undefined} refusal the refusal's message id
 * @returns {"id" | "name" | "command" | "cwd" | "url" | "headers" | "form"}
 */
export function fieldForRefusal(refusal) {
  return (refusal && FIELD_OF_REFUSAL[refusal]) || "form";
}

/**
 * Whether a test's answer is still about the draft on screen: the draft is
 * counted each time it changes, and an answer is drawn only when the count
 * it was dialled at is the count now. A report over a draft edited since —
 * *connected*, for a URL that is no longer the one typed — would be a lie.
 * @param {number} dialled the draft's count when the test started
 * @param {number} now the draft's count when the answer landed
 */
export function probeStillAbout(dialled, now) {
  return dialled === now;
}

/** *2 variables* · *1 header* — a row's count of what it keeps on this machine. @param {object} t */
export function secretCount(t) {
  if (t.transport === "stdio") {
    const n = Object.keys(t.env ?? {}).length;
    return n === 0 ? "" : tr("settings-mcp-form-variables", { n });
  }
  const n = Object.keys(t.headers ?? {}).length;
  return n === 0 ? "" : tr("settings-mcp-form-headers", { n });
}

/** What the harness will reach, in one line — never a value. @param {object} t */
export function transportLine(t) {
  if (t.transport !== "stdio") return t.url;
  const args = (t.args ?? []).join(" ");
  const line = args ? `${t.command} ${args}` : t.command;
  return t.cwd ? tr("settings-mcp-form-line-in-cwd", { line, cwd: t.cwd }) : line;
}
