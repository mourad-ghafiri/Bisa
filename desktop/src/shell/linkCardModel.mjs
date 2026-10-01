/**
 * What the card at the pointer offers (ide/17 §The handler), as facts: the
 * roots a text is read against when its surface names none, the verbs a
 * path's card holds for each answer the resolver gives, the verbs a URL's
 * card holds, and the words once something was copied. The provider
 * (`linkHandler.tsx`) binds each verb to its act by `id` and draws; nothing
 * here opens, reveals or copies anything.
 *
 * A verb is `{id, label, primary?}` and, where its act needs one, the thing
 * it acts on — a resolved document (`doc`), an absolute path (`absolute`).
 *
 * Plain `.mjs`, so `node --test` reads it.
 */

import { joinPath } from "../ui/fileTreeModel.mjs";
import { addressWords, urlWords } from "../ui/linkModel.mjs";
import { t } from "../i18n/l10n.mjs";

/** How many roots a path is looked up in when the surface names none: enough for a workspace, bounded for a click. */
export const MAX_DEFAULT_ROOTS = 8;

/**
 * The roots a text is read against when its surface names none: the root on
 * screen first, then every checkout on disk, each once, at most
 * `MAX_DEFAULT_ROOTS`.
 * @param {{name: string, scope?: string, id?: string}} route
 * @param {readonly {exists: boolean, path?: string | null, workstream: {id: string}}[]} workstreams
 * @returns {{scope: string, id: string}[]}
 */
export function defaultRoots(route, workstreams) {
  const out = [];
  if (route?.name === "workbench" && route.scope && route.id) out.push({ scope: route.scope, id: route.id });
  for (const w of workstreams ?? []) {
    if (out.length >= MAX_DEFAULT_ROOTS) break;
    if (!w.exists || !w.path) continue;
    if (out.some((r) => r.scope === "workstream" && r.id === w.workstream.id)) continue;
    out.push({ scope: "workstream", id: w.workstream.id });
  }
  return out;
}

/**
 * What a root is called on a card: its project and its workstream's name,
 * else its id; a goal's root by the word.
 * @param {{scope: string, id: string}} ref
 * @param {{project_name?: string | null, workstream: {name?: string | null}} | null | undefined} workstream the workspace's row for it, when it is a workstream
 */
export function rootLabel(ref, workstream) {
  if (workstream) return [workstream.project_name, workstream.workstream.name].filter(Boolean).join(" › ") || ref.id;
  return ref.scope === "goal" ? t("shell-link-handler-goal") : ref.id;
}

/** An absolute path from a root and a path under it; the root itself for no path. */
function under(root, path) {
  return path ? joinPath(root, path) : String(root);
}

/**
 * A path's card: its title, the line under it, and its verbs in order — the
 * first the one Enter takes. One document opens; several checkouts holding
 * it are each a door; a path outside every root opens as a loose file, is
 * revealed or copied; one nobody vouches for is copied and nothing else.
 * @param {{path: string}} hit
 * @param {import("../ui/linkModel.d.mts").LinkResolution} res
 * @param {{reveal: string}} words the platform's word for revealing a file
 */
export function pathCard(hit, res, { reveal }) {
  const copy = { id: "copy", label: t("shell-link-handler-copy-path"), text: hit.path, what: "path" };
  switch (res.kind) {
    case "doc":
    case "dir": {
      const verbs = [{ id: "open", label: t("shell-link-handler-open-ide", { res: res.kind === "dir" ? res.label : addressWords(res.path, res.line, res.col) }), primary: true, doc: res }];
      if (res.root) verbs.push({ id: "reveal", label: reveal, absolute: under(res.root, res.path) });
      verbs.push(copy);
      return { title: res.path || res.label, subtitle: res.root ? `${res.label} · ${res.root}` : res.label, verbs };
    }
    case "choice":
      return {
        title: hit.path,
        subtitle: t("shell-link-handler-more-than-one-checkout"),
        verbs: [...res.candidates.map((c) => ({ id: "open", label: t("shell-link-handler-open", { c: c.label }), doc: c })), copy],
      };
    case "outside":
      return {
        title: res.absolute,
        subtitle: t("shell-link-handler-outside-every-checkout-desktop-knows"),
        verbs: [{ id: "open_loose", label: t("shell-link-handler-open-ide-2"), primary: true, absolute: res.absolute }, { id: "reveal", label: reveal, absolute: res.absolute }, copy],
      };
    default:
      return { title: res.raw, subtitle: t("shell-link-handler-found-any-checkout"), verbs: [copy] };
  }
}

/**
 * A URL's card: the host in bold, the URL beneath. Only `http` and `https`
 * open — in the embedded browser first where there is one, then the
 * machine's; any other scheme is copied and nothing else. The browser never
 * opens on a bare click: a URL an agent or a collaborator wrote is untrusted
 * text.
 * @param {string} url
 * @param {{embedded: boolean}} can whether the embedded browser can open a tab here
 */
export function urlCard(url, { embedded }) {
  const { host, scheme } = urlWords(url);
  const copy = { id: "copy", label: t("shell-link-handler-copy-url"), text: url, what: "url" };
  if (scheme !== "http" && scheme !== "https") return { title: url, subtitle: t("shell-link-handler-only-http-https-open-browser"), verbs: [copy] };
  const here = embedded ? [{ id: "open_here", label: t("shell-link-handler-open-bisa-s-browser"), primary: true, url }] : [];
  return { title: host, subtitle: url, verbs: [...here, { id: "open_machine", label: t("shell-status-bar-open-machine-s-browser"), primary: !embedded, url }, copy] };
}

/**
 * What is said once a copy was asked for: copied, or that the clipboard
 * refused.
 * @param {"path" | "url"} what @param {boolean} ok
 */
export function copiedWords(what, ok) {
  if (what === "url") return ok ? t("shell-link-handler-url-copied") : t("shell-link-handler-url-not-copied");
  return ok ? t("shell-link-handler-path-copied") : t("shell-link-handler-path-not-copied");
}
