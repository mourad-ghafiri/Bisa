/**
 * About's settings as one draft (ide/04 §The repository under About › Settings):
 * everything a person changes under *Checkout* and *Settings* — the project's
 * publishing policy, its own layer of the settings keys (a value, or
 * `INHERIT` to put the workspace's back), its workstream scripts, the
 * repository's account pin, and the local git config the form edits — waits
 * in one draft until the toolbar's **Save** writes it all, in one order.
 * Nothing on these views saves on change. The draft is session memory, so a
 * tab switch loses nothing and the toolbar says how many changes wait.
 * `useProjectSettingsDraft.ts` holds the draft and applies the writes;
 * `SettingsToolbar.tsx` draws the count; `node --test` checks the rules here.
 */

import { PHASES, TIMEOUT_KEY, validateScript } from "./workstreamScripts.mjs";
import { t } from "../../i18n/l10n.mjs";

/** A drafted *Inherit*: the project's value goes, the workspace's applies. Compared by identity. */
export const INHERIT = Object.freeze({ inherit: true });

/**
 * @typedef {{
 *   settings: Record<string, unknown>,
 *   publish: string | null,
 *   account: string | null | undefined,
 *   scripts: {texts: Record<string, string>, timeout: number} | null,
 * }} Draft
 *   `publish` null = untouched; `account` undefined = untouched, null = unpin;
 *   `scripts` null = untouched.
 * @typedef {{
 *   settings: Record<string, {value: unknown, own: boolean}>,
 *   publish: string,
 *   account: string | null,
 *   scripts: {texts: Record<string, string>, timeout: number},
 * }} Current
 *   what the node holds now — `account` the pinned login or null.
 * @typedef {{set: Record<string, string>, unset: string[]}} GitWrite
 */

/** A draft with nothing in it. */
export function emptyDraft() {
  return { settings: {}, publish: null, account: undefined, scripts: null };
}

export function withSetting(draft, key, value) {
  return { ...draft, settings: { ...draft.settings, [key]: value } };
}

export function withInherit(draft, key) {
  return withSetting(draft, key, INHERIT);
}

/** Forget one key's draft — the row goes back to what the node holds. */
export function withoutSetting(draft, key) {
  const settings = { ...draft.settings };
  delete settings[key];
  return { ...draft, settings };
}

export function withPublish(draft, publish) {
  return { ...draft, publish };
}

export function withAccount(draft, account) {
  return { ...draft, account };
}

export function withScripts(draft, scripts) {
  return { ...draft, scripts };
}

const same = (a, b) => JSON.stringify(a ?? null) === JSON.stringify(b ?? null);

/**
 * The keys of `draft.settings` that would change something: a value the
 * project does not hold yet (a new own value, or a different one), or an
 * `INHERIT` on a key the project owns.
 * @param {Draft} draft @param {Current} current
 */
export function changedSettings(draft, current) {
  return Object.entries(draft.settings)
    .filter(([key, value]) => {
      const now = current.settings[key];
      if (value === INHERIT) return now?.own === true;
      return !(now?.own === true && same(value, now.value));
    })
    .map(([key]) => key);
}

/** The script texts (by their settings key) and the timeout that differ from what the node holds. */
export function changedScripts(draft, current) {
  if (!draft.scripts) return { keys: [], texts: false };
  const keys = [];
  for (const p of PHASES) if ((draft.scripts.texts[p.id] ?? "") !== (current.scripts.texts[p.id] ?? "")) keys.push(p.key);
  const texts = keys.length > 0;
  if (draft.scripts.timeout !== current.scripts.timeout) keys.push(TIMEOUT_KEY);
  return { keys, texts };
}

/**
 * How many changes wait — one per setting key, per script field, per git
 * config key, plus the policy and the pin when they moved.
 * @param {Draft} draft @param {Current} current @param {GitWrite | null} gitWrite the form's write
 */
export function changeCount(draft, current, gitWrite) {
  // The git config form counts on its own: a project read that has not
  // landed must not hide an identity a person typed and believes saved.
  let n = gitWrite ? Object.keys(gitWrite.set).length + gitWrite.unset.length : 0;
  if (!current) return n;
  n += changedSettings(draft, current).length + changedScripts(draft, current).keys.length;
  if (draft.publish !== null && draft.publish !== current.publish) n += 1;
  if (draft.account !== undefined && draft.account !== current.account) n += 1;
  return n;
}

/**
 * What refuses a save: a script git would choke on, a git config value the
 * form refused — each under the thing it is about.
 * @param {Draft} draft @param {Record<string, string>} gitProblems the form's
 */
export function problems(draft, gitProblems = {}) {
  const out = {};
  if (draft.scripts) {
    for (const p of PHASES) {
      const problem = validateScript(draft.scripts.texts[p.id] ?? "");
      if (problem) out[`script:${p.id}`] = problem;
    }
  }
  for (const [key, problem] of Object.entries(gitProblems)) out[`git:${key}`] = problem;
  return out;
}

/**
 * The writes a save makes, in order: the policy on the project record; the
 * settings values in one call (the drafted keys and the changed scripts
 * together); each inherited key unset; the local git config; the scripts
 * approved on this machine when a text changed (you wrote it, you approved
 * it); the account pin last, since it re-reads the connection.
 * @param {Draft} draft @param {Current} current @param {GitWrite | null} gitWrite
 * @returns {({op: "patch_project", publish: string} | {op: "set_settings", values: Record<string, unknown>} | {op: "unset_setting", key: string} | {op: "git_config", write: GitWrite} | {op: "approve_scripts"} | {op: "account", login: string | null})[]}
 */
export function writes(draft, current, gitWrite) {
  const out = [];
  if (draft.publish !== null && draft.publish !== current.publish) out.push({ op: "patch_project", publish: draft.publish });
  const values = {};
  const unset = [];
  for (const key of changedSettings(draft, current)) {
    const value = draft.settings[key];
    if (value === INHERIT) unset.push(key);
    else values[key] = value;
  }
  const scripts = changedScripts(draft, current);
  if (draft.scripts) {
    for (const key of scripts.keys) {
      const phase = PHASES.find((p) => p.key === key);
      values[key] = phase ? (draft.scripts.texts[phase.id] ?? "") : draft.scripts.timeout;
    }
  }
  if (Object.keys(values).length > 0) out.push({ op: "set_settings", values });
  for (const key of unset) out.push({ op: "unset_setting", key });
  if (gitWrite && (Object.keys(gitWrite.set).length > 0 || gitWrite.unset.length > 0)) out.push({ op: "git_config", write: gitWrite });
  if (scripts.texts) out.push({ op: "approve_scripts" });
  if (draft.account !== undefined && draft.account !== current.account) out.push({ op: "account", login: draft.account });
  return out;
}

/**
 * The toolbar's words: the count as a sentence, and the button's. *All
 * saved* is said only when every read landed — a toolbar that said it while
 * a read was still out hid an edit nobody could save.
 * @param {number} count @param {boolean} saving @param {boolean} [whole=true] every read the draft compares against has landed
 */
export function toolbarWords(count, saving, whole = true) {
  return {
    status: count > 0 ? t("work-project-settings-draft-unsaved-change-changes", { count }) : whole ? t("work-project-settings-draft-all-saved") : t("work-project-settings-draft-still-reading-project"),
    save: saving ? t("work-project-settings-draft-saving") : t("work-agent-editor-save"),
  };
}

/** The toast once a save landed. */
export function savedWords(count, approved) {
  const base = t("work-project-settings-draft-saved-change-changes", { count });
  return approved ? t("work-project-settings-draft-scripts-approved-machine", { base }) : `${base}.`;
}

/**
 * Which of the draft's reads an engine fact makes stale. The draft holds
 * only what the person changed and is compared against the live values, so
 * a fact from elsewhere — another window, the CLI — never costs an edit: it
 * moves what the edits are compared with. The project's own record (its
 * publishing policy) moves on `project_changed` for this project and no
 * other; the scripts and their approval on `settings_changed`, which the
 * resolved settings hear through their own store.
 * @param {{type?: string, project?: string} | null | undefined} payload
 * @param {string} pid
 * @returns {("project" | "scripts")[]}
 */
export function staleReads(payload, pid) {
  if (!payload || typeof payload.type !== "string") return [];
  if (payload.type === "project_changed") return payload.project === pid ? ["project", "scripts"] : [];
  if (payload.type === "settings_changed") return ["scripts"];
  return [];
}

