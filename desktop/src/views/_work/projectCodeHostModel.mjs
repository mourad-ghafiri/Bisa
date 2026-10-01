/**
 * The New Project dialog's code host line (ide/08 §Surfaces), pure over the
 * node's `RemoteInspection` (`POST /codehost/inspect`): what the URL or
 * folder a person named is on — *GitHub · acme/web · SSH via github-acme* —
 * which accounts could speak for it and which is suggested, and what to say
 * when nobody is signed in. The dialog draws; the account chosen becomes the
 * repository's `codehost.account` through the creation body's `git_config`.
 */

import { cliName, hostLabel } from "./codeHostWords.mjs";
import { protocolWords } from "./remoteModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/**
 * The one line: host, repository, protocol, alias, and who it speaks as.
 * `null` before anything is typed or when the source names no remote.
 * @param {import("../../types").RemoteInspection | null | undefined} inspection
 * @param {string | null | undefined} chosen the login the person picked, when they did
 * @returns {{tone: "ok" | "warn" | "quiet", text: string, hostLabel: string | null} | null}
 */
/**
 * What `POST /codehost/inspect` is asked about (`Inspect`): the URL about to
 * be cloned, or the folder about to be imported — one named key, never both.
 * `null` where there is nothing to inspect: a new project, or a field still
 * empty.
 * @param {"new" | "clone" | "import"} provenance
 * @param {string} url @param {string} path
 * @returns {{url: string} | {path: string} | null}
 */
export function inspectBody(provenance, url, path) {
  const typed = (v) => String(v ?? "").trim();
  if (provenance === "clone") return typed(url) ? { url: typed(url) } : null;
  if (provenance === "import") return typed(path) ? { path: typed(path) } : null;
  return null;
}

export function inspectionLine(inspection, chosen = null) {
  if (!inspection?.remote) return null;
  const r = inspection.remote;
  const protocol = protocolWords(r.protocol).label;
  const where = r.owner && r.name ? `${r.owner}/${r.name}` : r.summary;
  const via = r.alias ? t("work-project-code-host-via-alias", { alias: r.alias }) : "";
  if (!inspection.code_host) {
    return { tone: "quiet", text: t("work-project-code-host-not-code-host-build-knows-name", { host: r.host ?? t("work-project-code-host-remote"), where, protocol, via }), hostLabel: null };
  }
  const label = hostLabel(inspection.code_host.kind);
  const login = chosen ?? inspection.suggested;
  const who = login ? ` · as @${login}` : "";
  const tone = login || inspection.cautions.length === 0 ? "ok" : "warn";
  return { tone, text: `${label} · ${where} · ${protocol}${via}${who}`, hostLabel: label };
}

/**
 * The account choices as a select's options: the suggested one first, each
 * with where it comes from; *— the code host's own choice —* when the person
 * pins nothing.
 * @param {import("../../types").RemoteInspection | null | undefined} inspection
 * @returns {{login: string, words: string, suggested: boolean}[]}
 */
export function accountOptions(inspection) {
  if (!inspection) return [];
  return inspection.accounts.map((a) => ({
    login: a.login,
    words: `@${a.login} — ${sourceWords(a.source, inspection.code_host?.kind ?? null, a.note)}`,
    suggested: a.login === inspection.suggested,
  }));
}

function sourceWords(source, kind, note) {
  switch (source) {
    case "profile":
      return note || t("work-project-code-host-from-profile");
    case "default":
      return t("work-project-code-host-default-account", { kind: hostLabel(kind) });
    case "cli":
      return t("work-project-code-host-signed", { kind: cliName(kind)?.label ?? t("work-connection-cli") });
    case "stored":
      return t("work-project-code-host-stored-token");
    case "git_helper":
      return t("work-project-code-host-git-s-credential-helper");
    default:
      return note || source;
  }
}

/**
 * The `codehost.account` write the dialog's git-config form carries for a
 * chosen login, or nothing for *no pin* (the host's own choice).
 * @param {string | null | undefined} login
 * @returns {{key: string, value: string} | null}
 */
export function pinWrite(login) {
  const l = String(login ?? "").trim().toLowerCase();
  return l ? { key: "codehost.account", value: l } : null;
}

/**
 * What to say under the line when nobody is signed in: the inspection's
 * caution, and the Settings panel that fixes it.
 * @param {import("../../types").RemoteInspection | null | undefined} inspection
 * @returns {{text: string, tab: string | null} | null}
 */
export function signInCaution(inspection) {
  const text = inspection?.cautions?.[0];
  if (!text) return null;
  return { text, tab: inspection?.code_host?.kind ?? null };
}
