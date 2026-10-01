/**
 * What the workbench saves per root so a restart puts you back (ide/03):
 * the open documents, the active one, the pane tree they sit in and which
 * are pinned. Furniture — local, never in the URL beyond the active document,
 * never synced.
 *
 * A saved layout that names a file which no longer exists is restored anyway:
 * the tab opens in a *missing* state with its path shown, because a tab you
 * had open is a thing you meant to come back to. A **preview** tab is not
 * saved — a restart brings back what you meant to keep — and neither is its
 * place in a pane, so a click through previews writes nothing; an
 * **untitled** document is left out the same way, since a layout that named
 * one would bring back an empty buffer with nothing of what was typed. A
 * **loose** file (a file from this machine by absolute path) is saved and
 * restored like a root file: a tab you had open is a thing you meant to come
 * back to, and its path says everything about it. The **strip's order** —
 * documents and terminals as they were opened or dragged — is saved too
 * (version 5); the terminal store restores its sessions under the same keys,
 * so the interleaving survives a restart. Version 3
 * added panes and pins; version 4 dropped the Git document, so a saved layout
 * that could name one is refused rather than reinterpreted.
 */

import { findLeaf, leafOfTab, leaves, parseTree, removeTab, setActiveTab, singleLeaf } from "../../shell/paneTreeModel.mjs";
import { tabId } from "./workbenchModel.mjs";

export const LAYOUT_VERSION = 5;

/**
 * From the model's tabs, the active id, the pane tree, the pins and the
 * preview ids to the JSON that is saved. A terminal tab is never saved: the
 * terminal store keeps its own sessions, and an active terminal is saved as
 * nothing active. A preview, and an untitled document, is left out of the
 * tabs and of its leaf; an active one is saved as the kept tab its pane
 * would show without it.
 */
export function serializeLayout(tabs, activeId, panes = null, pinned = [], previews = [], strip = []) {
  const hidden = new Set(previews ?? []);
  for (const t of tabs) if (t?.kind === "untitled") hidden.add(tabId(t));
  const docs = tabs.filter((t) => t && t.kind !== "terminal" && t.kind !== "browser" && !hidden.has(tabId(t)));
  const ids = new Set(docs.map(tabId));
  let tree = panes ?? singleLeaf("d1", [...ids]);
  for (const id of hidden) tree = removeTab(tree, id);
  const wanted = typeof activeId === "string" && activeId && !activeId.startsWith("terminal:") && !activeId.startsWith("browser:") ? activeId : null;
  let active = wanted;
  if (wanted !== null && hidden.has(wanted)) {
    const leaf = panes ? leafOfTab(panes, wanted) : null;
    active = (leaf ? findLeaf(tree, leaf.id)?.active : null) ?? leaves(tree)[0]?.active ?? null;
  }
  // The leaf holding the active document shows it — so a glance through
  // previews, which changed which tab a leaf last showed, serialises to the
  // same bytes as the layout without them.
  if (active !== null) {
    const leaf = leafOfTab(tree, active);
    if (leaf) tree = setActiveTab(tree, leaf.id, active);
  }
  return {
    version: LAYOUT_VERSION,
    tabs: docs.map((t) => (t.kind === "file" || t.kind === "loose" ? { kind: t.kind, path: t.path } : t.kind === "device" ? { kind: "device", id: t.id } : { kind: t.kind })),
    active,
    panes: tree,
    pinned: (pinned ?? []).filter((id) => ids.has(id)),
    // The saved documents and the terminals, in the strip's order; a preview
    // or an untitled document is not saved, so its place is not either.
    strip: (strip ?? []).filter((id) => ids.has(id) || id.startsWith("terminal:")),
  };
}

/**
 * From saved JSON to tabs, an active id, a pane tree and pins, dropping
 * anything that is not a tab. `null` for a layout that is unusable.
 */
export function restoreLayout(json) {
  if (!json || typeof json !== "object" || json.version !== LAYOUT_VERSION) return null;
  const tabs = [];
  for (const t of Array.isArray(json.tabs) ? json.tabs : []) {
    if (!t || typeof t !== "object") continue;
    if (t.kind === "file" && typeof t.path === "string" && t.path) tabs.push({ kind: "file", path: t.path });
    else if (t.kind === "loose" && typeof t.path === "string" && t.path.startsWith("/")) tabs.push({ kind: "loose", path: t.path });
    else if (t.kind === "diff") tabs.push({ kind: "diff" });
    else if (t.kind === "device" && typeof t.id === "string" && t.id && !t.id.includes("/")) tabs.push({ kind: "device", id: t.id });
  }
  const ids = tabs.map(tabId);
  const active = typeof json.active === "string" && json.active ? json.active : null;
  // A tree that lost every tab, or was never saved, is one pane holding them all.
  let panes = parseTree(json.panes, ids) ?? singleLeaf("d1", ids);
  const placed = new Set(allTabIds(panes));
  const orphans = ids.filter((id) => !placed.has(id));
  if (orphans.length > 0) panes = adopt(panes, orphans);
  const pinned = Array.isArray(json.pinned) ? json.pinned.filter((p) => typeof p === "string" && ids.includes(p)) : [];
  const strip = Array.isArray(json.strip) ? json.strip.filter((id) => typeof id === "string" && (ids.includes(id) || id.startsWith("terminal:"))) : [];
  return { tabs, active, panes, pinned, strip };
}

function allTabIds(n) {
  return n.kind === "leaf" ? n.tabs : [...allTabIds(n.a), ...allTabIds(n.b)];
}

/** Put tabs the saved tree forgot into its first leaf. */
function adopt(n, ids) {
  if (n.kind === "leaf") return { ...n, tabs: [...n.tabs, ...ids], active: n.active ?? ids[0] ?? null };
  return { ...n, a: adopt(n.a, ids) };
}

/** Two layouts that would save the same bytes. */
export function sameLayout(a, b) {
  return JSON.stringify(a) === JSON.stringify(b);
}
