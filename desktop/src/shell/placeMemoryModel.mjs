/**
 * Where a person was (`crates/desktop.md` §Routing): the place the app was
 * last on, the place each section was last on, and the query each path was
 * left with — so a section's door returns to where they were in it, a bare
 * link to a goal opens it on the tab it was left on, and a launch opens
 * where the app closed.
 *
 * A **place** is what the address says: a path and the part of its query
 * that is state. Which keys are state is a table here, never a guess:
 *
 * - `REMEMBERED` — per route, the keys that say how the screen stands (a
 *   tab, a filter, the row picked, the document shown);
 * - `PANE_KEYS` — the Details pane's, which stands beside any screen;
 * - `ONE_SHOT` — what a link hands over once and the screen takes off the
 *   address (`panel`, `run`, `step`, `edit`), and what must never be kept:
 *   `join` is an invitation code.
 *
 * A key of neither table for the route it is on is carried by the link and
 * never remembered. An **arrival** — a link, a `navigate` — with no state of
 * its own is given what its path remembers; an **exact** entry — one the
 * router wrote, or Back and Forward onto one it resolved — is taken as it
 * is. Facts only: no window, no storage, no React. Plain `.mjs`, so
 * `node --test` reads it.
 */

import { href, parse, section, splitHash } from "../routeModel.mjs";

/** The storage key, and the shape's version — another version is refused. */
export const PLACES_KEY = "bisa.view.places";
export const PLACES_VERSION = 1;

/** How many paths keep their query — the newest kept. */
export const MAX_PATHS = 64;
/** The longest query a path keeps; a longer one is not remembered. */
export const MAX_QUERY = 512;

/** Per route, the query keys that are the screen's state. */
export const REMEMBERED = Object.freeze({
  pulse: Object.freeze(["concept"]),
  inbox: Object.freeze(["filter", "source", "item"]),
  goals: Object.freeze(["holder", "workflow", "q", "archived"]),
  goal: Object.freeze(["tab", "conversation", "conversations"]),
  workflows: Object.freeze(["view", "q", "status", "archived"]),
  workflow: Object.freeze(["conversation", "conversations"]),
  run: Object.freeze([]),
  projects: Object.freeze([]),
  workbench: Object.freeze(["doc"]),
  hosted_channel: Object.freeze([]),
  hosted_dm: Object.freeze([]),
  channels: Object.freeze([]),
  channel: Object.freeze([]),
  messages: Object.freeze([]),
  dm: Object.freeze([]),
  conversation: Object.freeze([]),
  agents: Object.freeze(["tab"]),
  agent: Object.freeze(["tab"]),
  teams: Object.freeze(["team"]),
  settings: Object.freeze(["tab", "kind"]),
});

/** The Details pane's keys: what stands beside a screen, whatever the screen. */
export const PANE_KEYS = Object.freeze(["aux", "auxId", "stage", "insp"]);

/**
 * What a link hands over once, and what is never kept. `conversation` on a
 * workbench is one of these by its route's table — it is read once there
 * and taken off the address — and `join` is an invitation code.
 */
export const ONE_SHOT = Object.freeze(["panel", "run", "step", "edit", "join"]);

/** Every key some table names — what the source guard holds the desktop's query keys to. */
export const CLASSIFIED = Object.freeze([...new Set([...Object.values(REMEMBERED).flat(), ...PANE_KEYS, ...ONE_SHOT])]);

/** What `parse` is handed as a home, to tell a hash that names no screen. */
const NOWHERE = Object.freeze({ name: "" });

/** A memory with nothing in it. */
export function emptyPlaces(owner = null) {
  return { v: PLACES_VERSION, owner, last: null, sections: {}, queries: [] };
}

/**
 * The route a hash names, or `null` for one that names none — the empty
 * hash, a path outside the table.
 * @param {string} hash
 */
export function routeOf(hash) {
  const route = parse(hash, NOWHERE);
  return route.name === "" ? null : route;
}

/**
 * The path a hash stands on, as the router writes it — the route's own
 * `href`, so two spellings of one place are one path; the hash's own path,
 * without a trailing slash, for one that names no screen.
 * @param {string} hash
 */
export function pathOf(hash) {
  const route = routeOf(hash);
  if (route) return splitHash(href(route)).path;
  const { path } = splitHash(hash);
  return path.length > 1 ? path.replace(/\/+$/, "") : path;
}

/** A query's pairs, in order, the empty ones left out. @param {string} query */
function pairsOf(query) {
  return [...new URLSearchParams(String(query ?? "").replace(/^\?/, ""))].filter(([, v]) => v !== "");
}

/** Pairs as a query, without its `?`. */
function queryFrom(pairs) {
  return new URLSearchParams(pairs).toString();
}

/**
 * A query split three ways for the route it is on: the screen's state, the
 * pane's, and what the link carries for itself.
 * @param {string} name the route's name
 * @param {string} query
 */
export function splitQuery(name, query) {
  const state = REMEMBERED[name] ?? [];
  const out = { state: [], pane: [], carried: [] };
  for (const pair of pairsOf(query)) {
    if (state.includes(pair[0])) out.state.push(pair);
    else if (PANE_KEYS.includes(pair[0])) out.pane.push(pair);
    else out.carried.push(pair);
  }
  return out;
}

/**
 * What a path keeps of a query: its state and its pane, never what a link
 * carried — so a one-shot key and an invitation code reach no memory.
 * @param {string} name the route's name
 * @param {string} query
 * @returns {string} without its `?`
 */
export function keptQuery(name, query) {
  const { state, pane } = splitQuery(name, query);
  return queryFrom([...state, ...pane]);
}

/** A path and a query as a hash. */
function hashOf(path, query) {
  return `#${path}${query ? `?${query}` : ""}`;
}

/** The query a path remembers, or `undefined` for a path never been on. */
export function rememberedQuery(places, path) {
  const found = places.queries.find(([p]) => p === path);
  return found ? found[1] : undefined;
}

/** Whether the memory has been on a path. */
export function knowsPath(places, path) {
  return rememberedQuery(places, path) !== undefined;
}

/** The memory with `path` keeping `query`, the newest, the oldest past the cap dropped. */
function withQuery(places, path, query) {
  const was = places.queries;
  const last = was[was.length - 1];
  if (last && last[0] === path && last[1] === query) return was;
  const next = was.filter(([p]) => p !== path);
  next.push([path, query]);
  return next.slice(-MAX_PATHS);
}

/**
 * A hash lands.
 *
 * `exact` is the router's word that the entry is to be taken as it is — it
 * wrote it, or Back and Forward came onto one it had resolved. Otherwise the
 * hash is an arrival:
 *
 * - with no state of its own, it is given the state its path remembers —
 *   and the pane, unless the link names one;
 * - with state of its own, that state is exact, and the pane is the
 *   remembered one when the link names none;
 * - what the link carried for itself rides along, last.
 *
 * What lands is recorded — without what was carried — as the path's query,
 * its section's place and the last place. A hash that names no screen lands
 * as it is and records nothing.
 *
 * @param {ReturnType<typeof emptyPlaces>} places
 * @param {string} hash
 * @param {{exact?: boolean}} [how]
 * @returns {{places: ReturnType<typeof emptyPlaces>, land: string, known: boolean}} the memory after, the hash to stand on, and whether the path had been on before
 */
export function arrive(places, hash, how = {}) {
  const route = routeOf(hash);
  if (!route) return { places, land: hash, known: false };
  const path = pathOf(hash);
  const { query } = splitHash(hash);
  const remembered = rememberedQuery(places, path);
  const known = knowsPath(places, path);
  const given = splitQuery(route.name, query);

  let state = given.state;
  let pane = given.pane;
  if (!how.exact) {
    const kept = splitQuery(route.name, remembered ?? "");
    if (given.state.length === 0) state = kept.state;
    if (given.pane.length === 0) pane = kept.pane;
  }
  const keeps = queryFrom([...state, ...pane]);
  const land = hashOf(path, queryFrom([...state, ...pane, ...given.carried]));
  // A query too long to keep is stood on and not remembered.
  if (keeps.length > MAX_QUERY) return { places, land, known };

  const place = hashOf(path, keeps);
  const key = section(route);
  const queries = withQuery(places, path, keeps);
  if (queries === places.queries && places.last === place && places.sections[key] === place) return { places, land, known };
  return { places: { ...places, last: place, sections: { ...places.sections, [key]: place }, queries }, land, known };
}

/** Where a launch with no hash opens: the last place, or `null` — the home. */
export function launchHash(places) {
  return places.last && routeOf(places.last) ? places.last : null;
}

/** The sections whose index is a door and not a list. */
const NO_LIST = Object.freeze(["projects"]);

/** A section's index — its list — as a hash, with the query the list was left with. */
export function indexHash(places, sectionKey) {
  const bare = href({ name: sectionKey });
  const path = pathOf(bare);
  return hashOf(path, rememberedQuery(places, path) ?? "");
}

/**
 * Where a section's door leads, from where the person stands.
 *
 * - From another section: the section's remembered place, else its index.
 * - From inside one of its details: its index, as the list was left.
 * - From its index: nowhere — the hash they stand on.
 *
 * The Projects section has no list — `#/projects` is a door to the IDE's
 * last root — so from inside it the door leads nowhere.
 *
 * @param {ReturnType<typeof emptyPlaces>} places
 * @param {string} sectionKey a section's route name: `goals`, `workflows`…
 * @param {string} current the hash the person stands on, resolved
 * @returns {string} a hash
 */
export function sectionHash(places, sectionKey, current) {
  const here = routeOf(current);
  if (!here || section(here) !== sectionKey) return places.sections[sectionKey] ?? indexHash(places, sectionKey);
  if (here.name === sectionKey || NO_LIST.includes(sectionKey)) return current;
  return indexHash(places, sectionKey);
}

/** Whether a hash stands on `path` or under it. */
function under(hash, path) {
  const p = pathOf(hash);
  return p === path || p.startsWith(`${path}/`);
}

/**
 * A thing is gone: its path — and what is under it — is forgotten, the last
 * place and a section's place that stood there fall away, so the section's
 * door leads to its index and a launch to the home. The same memory when
 * nothing stood there.
 * @param {ReturnType<typeof emptyPlaces>} places
 * @param {string} path
 */
export function forgetPath(places, path) {
  const at = pathOf(`#${path}`);
  const queries = places.queries.filter(([p]) => !(p === at || p.startsWith(`${at}/`)));
  const sections = Object.fromEntries(Object.entries(places.sections).filter(([, hash]) => !under(hash, at)));
  const last = places.last && under(places.last, at) ? null : places.last;
  const same = queries.length === places.queries.length && Object.keys(sections).length === Object.keys(places.sections).length && last === places.last;
  return same ? places : { ...places, last, sections, queries };
}

/**
 * The memory as another workspace's would not be: a memory stamped with
 * another owner is forgotten whole, one with no owner takes this one.
 * @param {ReturnType<typeof emptyPlaces>} places
 * @param {string | null | undefined} owner
 */
export function adoptPlaces(places, owner) {
  if (!owner || owner === places.owner) return places;
  return places.owner === null ? { ...places, owner } : emptyPlaces(owner);
}

/** A stored hash made safe: one that names a screen, with only what its path may keep. */
function placeOf(raw) {
  if (typeof raw !== "string") return null;
  const route = routeOf(raw);
  if (!route) return null;
  const keeps = keptQuery(route.name, splitHash(raw).query);
  return keeps.length > MAX_QUERY ? null : hashOf(pathOf(raw), keeps);
}

/**
 * A stored memory read back. Another version is refused — `undefined`, the
 * fallback; within this version only what still names a screen survives,
 * and only the keys its route may keep, so a table that changed remembers
 * nothing it no longer lists.
 * @param {unknown} raw
 * @returns {ReturnType<typeof emptyPlaces> | undefined}
 */
export function parsePlaces(raw) {
  if (!raw || typeof raw !== "object" || Array.isArray(raw) || raw.v !== PLACES_VERSION) return undefined;
  const out = emptyPlaces(typeof raw.owner === "string" && raw.owner ? raw.owner : null);
  out.last = placeOf(raw.last);
  if (raw.sections && typeof raw.sections === "object" && !Array.isArray(raw.sections)) {
    for (const [key, hash] of Object.entries(raw.sections)) {
      const place = placeOf(hash);
      if (place && section(routeOf(place)) === key) out.sections[key] = place;
    }
  }
  if (Array.isArray(raw.queries)) {
    for (const entry of raw.queries) {
      if (!Array.isArray(entry) || entry.length !== 2 || typeof entry[0] !== "string" || typeof entry[1] !== "string") continue;
      const place = placeOf(hashOf(entry[0], entry[1]));
      if (!place) continue;
      out.queries = withQuery(out, pathOf(place), splitHash(place).query);
    }
  }
  return out;
}
