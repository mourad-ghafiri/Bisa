/**
 * The footer's node read-out, as facts — no React. The dot's tone and the
 * sentence for the connection's three states, and the overlay's sections:
 * the connection (the address, who runs the node, how long up, restarts),
 * the node (its version beside the desktop's, paused or running, the
 * sessions live, the socket, where it listens, the data and logs folders),
 * its sync, and its own load. The version caution is `aboutModel.mjs`'s, so
 * the About dialog and the overlay never disagree.
 */

import { connected } from "../busModel.mjs";
import { formatSize } from "../ui/fileTreeModel.mjs";
import { versionCaution } from "./aboutModel.mjs";
import { t } from "../i18n/l10n.mjs";
import { percent } from "../i18n/format.mjs";

/** The dot's tone, the one-word state and the sentence for a connection state. */
export function statWords(conn) {
  // A lagged pulse is a connected node that dropped events; it reads as open.
  if (connected(conn)) return { tone: "ok", word: t("shell-node-stat-word-connected"), title: t("shell-node-stat-node-connected") };
  if (conn === "connecting") return { tone: "warn", word: t("shell-node-stat-word-connecting"), title: t("shell-node-stat-connecting-node") };
  return { tone: "danger", word: t("shell-node-stat-word-unreachable"), title: t("shell-node-stat-node-unreachable") };
}

/** A span of seconds in the unit that fits: `42 s`, `3 min`, `2 h 13 min`, `3 d 2 h`. */
export function sinceWords(secs) {
  const s = Math.max(0, Math.floor(Number(secs) || 0));
  if (s < 60) return t("shell-node-stat-seconds", { s });
  const min = Math.floor(s / 60);
  if (min < 60) return t("shell-node-stat-min", { min });
  const h = Math.floor(min / 60);
  const restMin = min % 60;
  if (h < 24) return restMin > 0 ? t("shell-node-stat-h-min", { h, restMin }) : t("shell-node-stat-hours", { h });
  const d = Math.floor(h / 24);
  const restH = h % 24;
  return restH > 0 ? t("shell-node-stat-d-h", { d, restH }) : t("shell-node-stat-days", { d });
}

/** The seconds from a unix moment to `now`, never negative. */
export function sinceOf(at, now) {
  return Math.max(0, Math.floor(Number(now) - Number(at)));
}

const pct = (n) => percent((Number(n) || 0) / 100);

/**
 * The overlay's sections, in order: the connection, the node, its sync, its
 * load. Nothing read yet is said as such; an unreachable node keeps the
 * last read under its sentence.
 * @param {"connecting" | "open" | "closed" | "lagged"} conn
 * @param {{
 *   info: import("../types").NodeInfo | null,
 *   status: import("./nodeApi").NodeStatus | null,
 *   workspace: import("../types").WorkspaceInfo | null,
 *   sync: import("../types").SyncReport | null,
 *   checks: readonly import("../types").RelayCheck[] | null,
 *   paused: boolean | null,
 *   share: import("./statsApi").ProcessShare | null,
 *   appVersion: string,
 *   apiBase: string,
 *   now: number,
 * }} facts
 */
export function overlaySections(conn, { info, status, workspace, sync, checks, paused, share, appVersion, apiBase, now }) {
  const sections = [];
  const state = statWords(conn);

  // The connection.
  const runBy = !status ? "—" : status.external ? t("shell-node-stat-node-started-outside-app") : status.pid != null ? t("shell-node-stat-app-pid", { pid: status.pid }) : t("shell-node-stat-app");
  const up = info ? sinceWords(sinceOf(info.started_at, now)) : status?.healthy_secs != null ? sinceWords(status.healthy_secs) : null;
  const connection = [{ label: t("shell-browser-bar-address"), value: apiBase }, { label: t("shell-node-stat-run"), value: runBy }];
  if (up) connection.push({ label: t("shell-node-stat-up"), value: up });
  if (status && status.restarts > 0) connection.push({ label: t("shell-node-stat-restarts"), value: String(status.restarts) });
  sections.push({
    key: "connection",
    title: t("shell-node-stat-connection"),
    tone: state.tone,
    label: state.word,
    sentence: conn === "closed" ? t("shell-node-stat-node-unreachable-what-follows-last-read") : null,
    rows: connection,
  });

  // The node.
  const engine = paused == null ? { tone: "quiet", label: t("shell-node-stat-read") } : paused ? { tone: "warn", label: t("shell-node-stat-engine-paused") } : { tone: "ok", label: t("shell-node-stat-engine-running") };
  if (!info) {
    sections.push({ key: "node", title: t("shell-node-overlay-node"), tone: engine.tone, label: engine.label, sentence: t("shell-node-stat-node-has-been-read-yet"), rows: [] });
  } else {
    const caution = versionCaution(appVersion, info.version);
    sections.push({
      key: "node",
      title: t("shell-node-overlay-node"),
      tone: engine.tone,
      label: engine.label,
      sentence: caution,
      rows: [
        { label: t("shell-node-stat-version"), value: t("shell-node-stat-desktop", { version: info.version, appVersion }) },
        { label: t("shell-node-stat-sessions"), value: t("shell-node-stat-live", { live_sessions: info.live_sessions }) },
        { label: t("shell-node-stat-socket"), value: info.socket },
        { label: t("shell-node-stat-listens"), value: info.listen ?? t("shell-node-stat-socket-only") },
        { label: t("shell-node-stat-data"), value: info.data_dir },
        { label: t("shell-node-stat-logs"), value: info.logs_dir },
      ],
    });
  }

  // The wire (14-collaboration): what `GET /sync` said, when a workspace
  // was read at all — every configured relay with its state, then the counts.
  if (workspace) {
    if (!sync) {
      sections.push({ key: "sync", title: t("shell-node-stat-relays"), tone: "quiet", label: t("shell-node-stat-read"), sentence: t("shell-node-stat-wire-has-been-read-yet"), rows: [] });
    } else {
      const rows = relayRows(sync.relays ?? [], checks);
      if (!sync.enabled) {
        sections.push({ key: "sync", title: t("shell-node-stat-relays"), tone: "quiet", label: "off", sentence: t("shell-node-stat-relays-off-turn-them-settings-relays"), rows });
      } else if (!sync.running) {
        sections.push({ key: "sync", title: t("shell-node-stat-relays"), tone: "quiet", label: t("shell-node-stat-pump"), sentence: t("shell-node-stat-collaboration-pump-runs-node"), rows });
      } else {
        const relays = sync.relays?.length ?? 0;
        rows.push(
          { label: t("shell-node-stat-people"), value: t("shell-node-stat-hosted-here", { people: sync.people }) },
          { label: t("shell-node-stat-hosts"), value: t("shell-node-stat-joined", { hosts: sync.hosts }) },
          { label: t("shell-node-stat-published"), value: String(sync.published) },
          { label: t("shell-node-stat-ingested"), value: String(sync.ingested) },
        );
        if (sync.iroh_node_id) rows.push({ label: t("shell-node-stat-direct-sessions"), value: String(sync.iroh_peers_connected ?? 0) });
        if (sync.last_catchup != null) rows.push({ label: t("shell-node-stat-last-catch-up"), value: t("shell-node-stat-ago", { since: sinceWords(sinceOf(sync.last_catchup, now)) }) });
        const connected = sync.connected_relays > 0;
        sections.push({ key: "sync", title: t("shell-node-stat-relays"), tone: connected ? "ok" : relays > 0 ? "danger" : "quiet", label: connected ? t("shell-node-stat-connected", { connected_relays: sync.connected_relays, relays }) : relays > 0 ? t("shell-node-stat-relay-answers") : t("shell-node-stat-none-configured"), sentence: null, rows });
      }
    }
  }

  // Its load — the node's own process, as the resource overlays count it.
  sections.push({
    key: "load",
    title: t("shell-node-stat-load"),
    sentence: share ? null : t("shell-node-stat-read-yet"),
    rows: share
      ? [
          { label: "CPU", value: pct(share.cpu_percent) },
          { label: t("shell-node-stat-memory"), value: formatSize(share.mem_bytes) || "0 B" },
          { label: t("shell-node-stat-running"), value: sinceWords(share.run_secs) },
        ]
      : [],
  });
  return sections;
}

/**
 * One row per configured relay: its state as the pool measures it — or the
 * last check's answer, when a check was run since — with the latency.
 * @param {readonly import("../types").RelayHealth[]} relays
 * @param {readonly import("../types").RelayCheck[] | null | undefined} checks
 */
function relayRows(relays, checks) {
  return relays.map((r) => {
    const check = (checks ?? []).find((c) => c.url === r.url) ?? null;
    if (check) {
      if (check.ok) return { label: r.url, value: check.latency_ms != null ? t("shell-node-stat-reachable-ms", { ms: check.latency_ms }) : t("shell-node-stat-reachable") };
      return { label: r.url, value: check.error ? t("shell-node-stat-not-reachable-error", { error: check.error }) : t("shell-node-stat-not-reachable") };
    }
    const state = r.connected ? "connected" : r.status === "off" ? "off" : r.status === "connecting" || r.status === "pending" || r.status === "initialized" ? "connecting" : r.status === "banned" ? "refused" : r.status;
    const word = t("shell-node-stat-relay-state", { state });
    return { label: r.url, value: r.latency_ms != null ? t("shell-node-stat-word-ms", { word, ms: r.latency_ms }) : word };
  });
}

/** The node's own process among the shares — the one whose root is the node. */
export function nodeShare(processes) {
  return (processes ?? []).find((p) => p.root?.kind === "node") ?? null;
}

/** The overlay's footnote: when the read landed. */
export function footnote(readAt, now = Date.now() / 1000) {
  if (readAt == null) return t("shell-node-stat-read-yet-2");
  return t("shell-node-stat-read-ago", { since: sinceWords(sinceOf(readAt, now)) });
}
