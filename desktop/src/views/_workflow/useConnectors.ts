/**
 * The connectors installed here, read once per mount, again whenever the
 * engine says one changed and when the bus comes back after the node was
 * away (`useAsync`) — the one list the connector step form, the input
 * editor's account kind and Settings › Connectors read, so they cannot
 * disagree about what exists.
 */

import { api } from "../../api";
import { useEngineEvents } from "../../bus";
import type { ConnectorDetail, ConnectorRow } from "../../types";
import { useAsync } from "../_work/useAsync";

export function useConnectors() {
  const list = useAsync((s) => api.connectors(null, s), []);
  useEngineEvents((e) => {
    if (e.payload.type === "connectors_changed") list.reload();
  });
  const rows: ConnectorRow[] = list.data?.connectors ?? [];
  return { rows, loading: list.loading, error: list.error, reload: list.reload };
}

/** One connector whole — its definition and this machine's accounts — or nothing while none is chosen. */
export function useConnectorDetail(id: string | null | undefined) {
  const detail = useAsync(async (s) => (id ? api.connector(id, s) : null), [id]);
  useEngineEvents((e) => {
    if (e.payload.type === "connectors_changed") detail.reload();
  });
  const data: ConnectorDetail | null = detail.data && id && detail.data.connector.id === id ? detail.data : null;
  return { data, loading: detail.loading, error: detail.error, reload: detail.reload };
}
