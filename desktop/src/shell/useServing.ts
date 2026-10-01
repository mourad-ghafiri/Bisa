/**
 * The servers the node has up and the ports the rail attributes to a
 * checkout (ide/18), read once for every surface that names them: the
 * Browser launcher's menu, a browser tab's bar and its annotation rule, a
 * tab's context menu, the Details pane. One `GET /servers` read answers
 * every server — the checkouts' and the artifacts' — since an artifact's
 * page is one a tab may show and never one to annotate; `servers` is this
 * checkout's slice of it. Reloaded on every `server_changed` — a checkout's
 * folder and an artifact's page alike — and when the node comes back.
 */

import { useMemo } from "react";
import { api } from "../api";
import { useEngineEvents } from "../bus";
import { useReloadOnReconnect } from "../ui/useReloadOnReconnect";
import type { ServedFolder } from "../types";
import { attributePorts, portsOf } from "../views/_workbench/portsModel.mjs";
import { useAsync } from "../views/_work/useAsync";
import { usePorts } from "./portsStore";
import { useSessions } from "./sessionsStore";
import { useTerminals } from "./useTerminals";

const NONE: readonly ServedFolder[] = Object.freeze([]);

export interface Serving {
  /** The servers of this checkout, oldest first — none off a workstream. */
  readonly servers: readonly ServedFolder[];
  /** Every server the node has up, artifacts included. */
  readonly all: readonly ServedFolder[];
  /** Whether the read has answered once — until then nothing is known, not even that there is nothing. */
  readonly read: boolean;
  readonly reload: () => void;
  readonly ports: readonly { port: number; process: string }[];
}

/** @param wid the checkout, or `null` on a goal, a work item or the workspace */
export function useServing(wid: string | null): Serving {
  const read = useAsync((s) => api.allServers(s).then((r) => r.servers), []);
  useEngineEvents((e) => {
    if (e.payload.type === "server_changed") read.reload();
  });
  // A restarted node serves nothing, and says so in no frame.
  useReloadOnReconnect(read.reload);
  const all = read.data ?? NONE;
  const servers = useMemo(() => (wid ? all.filter((s) => s.owner.kind === "workstream" && s.owner.workstream === wid) : NONE), [all, wid]);
  const scanned = usePorts();
  const { sessions: terminals } = useTerminals();
  const sessions = useSessions();
  const ports = useMemo(() => (wid ? portsOf(attributePorts(scanned, terminals, sessions), wid).map((p) => ({ port: p.port, process: p.process })) : []), [scanned, terminals, sessions, wid]);
  return { servers, all, read: read.data !== null, reload: read.reload, ports };
}
