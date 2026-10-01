/**
 * The router's facts (`router.ts` holds the window and the hooks): which
 * hash is which screen, which hash a screen is drawn at, which nav entry a
 * route lights, and how a query patch lands. `parse` and `href` are inverses
 * for every route — a link built here lands where it says. Plain `.mjs`, so
 * `node --test` reads it.
 *
 * Hash routing, not history: the app is served from a Tauri webview and a
 * plain file origin, so `#/…` needs no server rules and survives a reload.
 * The hash carries a query string of its own (`#/channels/abc?aux=thread`),
 * which is what makes the right-hand pane and a list's selection *places*.
 */

/**
 * What a workbench can be rooted at — the scopes `GET /tree/{scope}/{id}`
 * takes but `run`: a run of the workspace keeps a folder the Files API
 * answers, and the workbench reaches it through the run's work items. A
 * project's own tree is its primary workstream, whose id is the project's.
 */
export const WORKBENCH_SCOPES = Object.freeze(["workstream", "work_item", "goal"]);

/** Patterns in match order; `:name` captures one path segment. */
export const ROUTE_TABLE = Object.freeze(
  [
    ["/pulse", "pulse"],
    ["/inbox", "inbox"],
    ["/goals/:id", "goal"],
    ["/goals", "goals"],
    // The library, and one workflow's designer. A goal's run is drawn on the
    // same canvas but lives on the goal (`#/goals/:id?tab=workflow`).
    ["/workflows/:id", "workflow"],
    ["/workflows", "workflows"],
    // One run by its id alone — a run of the workspace's own page, the way
    // the wire addresses a run (`/runs/{rid}`); it belongs to Workflows.
    ["/runs/:id", "run"],
    // Three segments, the way the file routes are addressed: `{scope}/{id}`.
    ["/projects/:scope/:id", "workbench"],
    ["/projects", "projects"],
    ["/hosts/:host/channels/:id", "hosted_channel"],
    ["/hosts/:host/messages/:id", "hosted_dm"],
    ["/channels/:id", "channel"],
    ["/channels", "channels"],
    ["/messages/:id", "dm"],
    ["/messages", "messages"],
    ["/conversations/:id", "conversation"],
    ["/agents/:id", "agent"],
    ["/agents", "agents"],
    // Teams is an index screen only: a team is `?team=…`, so the palette can
    // jump at one before the screen that owns it exists. Settings does the
    // same with `?tab=`.
    ["/teams", "teams"],
    ["/settings", "settings"],
  ].map((row) => Object.freeze(row)),
);

/** Every route name — the index screens and the detail screens. */
export const ROUTE_NAMES = Object.freeze([...new Set(ROUTE_TABLE.map(([, name]) => name))]);

function matchPattern(pattern, path) {
  const p = pattern.split("/").filter(Boolean);
  const s = path.split("/").filter(Boolean);
  if (p.length !== s.length) return null;
  const params = {};
  for (let i = 0; i < p.length; i++) {
    const seg = p[i];
    if (seg.startsWith(":")) {
      try {
        params[seg.slice(1)] = decodeURIComponent(s[i]);
      } catch {
        // A malformed escape is not a place.
        return null;
      }
    } else if (seg !== s[i]) return null;
  }
  return params;
}

/** The hash's two halves: the path (`/` when empty) and the query after `?`. @param {string} hash */
export function splitHash(hash) {
  const raw = String(hash ?? "").replace(/^#/, "") || "/";
  const i = raw.indexOf("?");
  return i === -1 ? { path: raw, query: "" } : { path: raw.slice(0, i), query: raw.slice(i + 1) };
}

/**
 * The route a hash names; the caller's `home` when it names none — the empty
 * hash a launch starts with, the bare `#/`, a path outside the table, a
 * malformed escape. The home is the caller's because this model knows no
 * person: the router hands it the first destination of the sidebar order
 * (`navOrderStore.homeRoute`). A fresh copy each time — a caller may hold one.
 * A workbench scope the node cannot root at lands on the projects index
 * rather than being carried into every consumer as a typed hole.
 * @param {string} hash
 * @param {{name: string}} home
 */
export function parse(hash, home) {
  const { path } = splitHash(hash);
  for (const [pattern, name] of ROUTE_TABLE) {
    const params = matchPattern(pattern, path);
    if (!params) continue;
    if (name === "workbench" && !WORKBENCH_SCOPES.includes(params.scope)) return { name: "projects" };
    return { name, ...params };
  }
  return { ...home };
}

function pathOf(route) {
  const seg = (s) => encodeURIComponent(String(s));
  switch (route.name) {
    case "goal":
      return `/goals/${seg(route.id)}`;
    case "workflow":
      return `/workflows/${seg(route.id)}`;
    case "run":
      return `/runs/${seg(route.id)}`;
    case "workbench":
      return `/projects/${seg(route.scope)}/${seg(route.id)}`;
    case "channel":
      return `/channels/${seg(route.id)}`;
    case "dm":
      return `/messages/${seg(route.id)}`;
    case "hosted_channel":
      return `/hosts/${seg(route.host)}/channels/${seg(route.id)}`;
    case "hosted_dm":
      return `/hosts/${seg(route.host)}/messages/${seg(route.id)}`;
    case "conversation":
      return `/conversations/${seg(route.id)}`;
    case "agent":
      return `/agents/${seg(route.id)}`;
    default:
      return `/${route.name}`;
  }
}

/**
 * A query string from a patch: `undefined`, `null` and `""` leave a key out;
 * anything else is written as text. `""` when nothing remains, else `?k=v`.
 * @param {Record<string, string | number | null | undefined> | null | undefined} search
 */
export function queryOf(search) {
  return applySearchPatch("", search);
}

/**
 * A query with a patch landed on it: `undefined`, `null` and `""` remove a
 * key, anything else sets it as text; the rest of the query is kept.
 * @param {string} query the current query, with or without its `?`
 * @param {Record<string, string | number | null | undefined> | null | undefined} patch
 * @returns {string} `""`, or `?k=v&…`
 */
export function applySearchPatch(query, patch) {
  const q = new URLSearchParams(String(query ?? "").replace(/^\?/, ""));
  for (const [k, v] of Object.entries(patch ?? {})) {
    if (v === undefined || v === null || v === "") q.delete(k);
    else q.set(k, String(v));
  }
  const s = q.toString();
  return s ? `?${s}` : "";
}

/** The href for a route — the one place link targets are built. @param {object} route @param {object} [search] */
export function href(route, search) {
  return `#${pathOf(route)}${queryOf(search)}`;
}

/**
 * The index of a route's section — its list: where a screen leaves for when
 * what it showed is gone (a goal's page for Goals, a run's for Workflows);
 * the Inbox for a route whose section has no list.
 * @param {{name: string}} route
 */
export function indexRouteOf(route) {
  const name = section(route);
  // A conversation's door has no list of its own: what it showed is reached from the Inbox.
  const listed = ROUTE_TABLE.some(([pattern, n]) => n === name && !pattern.includes(":"));
  return { name: listed ? name : "inbox" };
}

/**
 * Which nav entry reads as active for a route — a detail screen keeps its
 * section lit (a goal belongs to Goals, a hosted channel to Channels).
 * @param {{name: string}} route
 */
export function section(route) {
  switch (route.name) {
    case "goal":
      return "goals";
    case "workflow":
    case "run":
      return "workflows";
    case "workbench":
      return "projects";
    case "channel":
    case "hosted_channel":
      return "channels";
    case "dm":
    case "hosted_dm":
      return "messages";
    case "agent":
      return "agents";
    default:
      return route.name;
  }
}
