/**
 * Read again when the bus comes back after the node was away. A read kept
 * fresh by frames alone is stale across a restart: what the node forgot (an
 * open ask lives as long as its session) and what it did at boot reach the
 * page by no frame (`workspaceLoadModel.reloadOnReconnect`).
 */
import { useEffect, useRef } from "react";
import { watchConnection } from "../bus";
import { reloadOnReconnect } from "../shell/workspaceLoadModel.mjs";

export function useReloadOnReconnect(reload: () => void): void {
  const ref = useRef(reload);
  ref.current = reload;
  useEffect(() => reloadOnReconnect(watchConnection, () => ref.current()), []);
}
