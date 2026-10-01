/**
 * What a server's health and a probe's report read as (06 § MCP servers):
 * the chip's tone and words for a row, the sentence for a failure by the
 * stage it stopped at, and the lines the editor's *Test connection* draws.
 * The facts are the node's (`McpHealthView`, `McpProbeReport`); this only
 * puts them in words. No DOM, no fetch.
 */

import { t } from "../../i18n/l10n.mjs";

/** The kit's tone for a health state. @param {{state: string} | null | undefined} health */
export function healthTone(health) {
  switch (health?.state) {
    case "ok":
      return "ok";
    case "failing":
      return "warn";
    default:
      return "quiet";
  }
}

/** What a failure at a stage means, for a person. @param {string | undefined} stage */
export function stageWords(stage) {
  switch (stage) {
    case "spawn":
      return t("settings-mcp-health-could-start-command-reach-url");
    case "initialize":
      return t("settings-mcp-health-reached-server-but-mcp-handshake-did");
    case "ping":
      return t("settings-mcp-health-answered-handshake-but-ping");
    case "tools":
      return t("settings-mcp-health-answered-handshake-but-tool-list");
    default:
      return t("settings-mcp-health-did-answer");
  }
}

/** The era's word. @param {string | undefined} era */
export function eraWords(era) {
  return era === "discover" ? "discover (2026-07-28 onwards)" : era === "handshake" ? "handshake (2024-11-05 – 2025-11-25)" : "";
}

/**
 * The row's words: *server 1.4.2 · 2026-07-28 · 12 tools*, the failure's
 * stage, or *not checked yet*.
 * @param {{state: string, server?: {name: string, version: string} | null, protocol_version?: string | null, tool_count?: number | null, stage?: string | null, error?: string | null} | null | undefined} health
 */
export function healthWords(health) {
  if (!health || health.state === "unknown") return t("settings-mcp-health-checked-yet");
  if (health.state === "failing") return stageWords(health.stage ?? undefined);
  const parts = [];
  if (health.server) parts.push(health.server.version ? `${health.server.name} ${health.server.version}` : health.server.name); // a name and its version number: no words
  if (health.protocol_version) parts.push(health.protocol_version);
  const n = health.tool_count ?? 0;
  parts.push(t("settings-mcp-health-tools", { n }));
  return parts.join(" · ");
}

/** *checked 2 min ago* — how old the answer is. @param {number | null | undefined} checkedAt seconds @param {number} [now] seconds */
export function checkedWords(checkedAt, now = Math.floor(Date.now() / 1000)) {
  if (!checkedAt) return "";
  const s = Math.max(0, now - checkedAt);
  if (s < 60) return t("settings-mcp-health-checked-just-now");
  if (s < 3600) return t("settings-mcp-health-checked-min-ago", { s: Math.floor(s / 60) });
  if (s < 86400) return t("settings-mcp-health-checked-h-ago", { s: Math.floor(s / 3600) });
  return t("settings-mcp-health-checked-d-ago", { s: Math.floor(s / 86400) });
}

/** The capabilities a report advertises, as chip words in a fixed order. @param {{tools?: boolean, resources?: boolean, prompts?: boolean, logging?: boolean, completions?: boolean} | undefined} caps */
export function capabilityWords(caps) {
  return ["tools", "resources", "prompts", "logging", "completions"].filter((k) => caps?.[k]);
}

/**
 * The editor's lines for a report — what *Test connection* draws.
 * @param {object} report a `McpProbeReport`
 * @returns {{ ok: boolean, headline: string, detail: string[], tools: {name: string, description?: string | null}[], more: number }}
 */
export function reportLines(report) {
  if (!report.ok) {
    return {
      ok: false,
      headline: stageWords(report.stage),
      detail: [report.error ?? "", t("settings-mcp-health-stopped-at", { stage: report.stage }), t("settings-mcp-health-ms", { elapsed_ms: report.elapsed_ms })].filter(Boolean),
      tools: [],
      more: 0,
    };
  }
  const who = report.server ? (report.server.version ? `${report.server.name} ${report.server.version}` : report.server.name) : t("settings-mcp-health-server");
  const detail = [];
  if (report.protocol_version) detail.push(t("settings-mcp-health-protocol", { protocol_version: report.protocol_version, era: eraWords(report.era), flag: (report.era) ? "yes" : "no" }));
  const caps = capabilityWords(report.capabilities);
  detail.push(caps.length ? t("settings-mcp-health-capabilities", { capabilities: caps.join(", ") }) : t("settings-mcp-health-advertises-capability"));
  if (typeof report.resource_count === "number") detail.push(t("settings-mcp-health-resources", { resource_count: report.resource_count }));
  if (typeof report.prompt_count === "number") detail.push(t("settings-mcp-health-prompts", { prompt_count: report.prompt_count }));
  detail.push(t("settings-mcp-health-ms", { elapsed_ms: report.elapsed_ms }));
  return {
    ok: true,
    headline: t("settings-mcp-health-answered-tool-tools", { who, tool_count: report.tool_count }),
    detail,
    tools: report.tools ?? [],
    more: Math.max(0, report.tool_count - (report.tools?.length ?? 0)),
  };
}

/** How many rows *Check all* dials at once — the engine joins a second ask to a running probe, so more buys nothing. */
export const CHECK_ALL_AT_ONCE = 4;
