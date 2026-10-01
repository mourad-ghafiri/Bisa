/**
 * *Initialise a repository* — the one offer every surface makes for a plain
 * folder (ide/04 the Git panel, ide/07 the Workstreams panel, ide/13 About ›
 * Checkout): the sentence that says what a plain folder is, whether the
 * button belongs here, what the click will do to *whose* folder, and what
 * to say once it is done. Facts in, words out; the card draws them.
 */

import { t } from "../../i18n/l10n.mjs";

/** The one home of the sentence — every surface reads it from here. */
export const PLAIN_FOLDER = t("work-init-repository-plain-folder-no-repository-work-here");

export const INIT_LABEL = t("work-init-repository-initialise-repository");

/**
 * Whether the offer belongs on this checkout: only a project's own root
 * (`primary`) that is on disk and not a repository. A `copy` workstream has
 * no project-level thing to turn on — its project's primary does — and a
 * `worktree` is a repository already.
 */
export function initOffer({ kind, exists, git }) {
  return { shown: kind === "primary" && exists === true && git === false };
}

/**
 * What the click does, in the person's words. Adopt writes nothing into a
 * folder Bisa did not make — so for an adopted folder this is the one write,
 * asked about first; a managed folder is Bisa's own and needs no second ask.
 */
export function initConsequence({ adopted, path }) {
  const where = adopted && path ? t("work-init-repository-into-path", { path }) : t("work-init-repository-here");
  const then = t("work-init-repository-existing-copy-workstreams-keep-working-next");
  if (adopted) {
    return {
      needsConfirm: true,
      body: t("work-init-repository-writes-git-folder-folder-bisa-did", { where, then }),
    };
  }
  return { needsConfirm: false, body: t("work-init-repository-work-then-branches-instead-copying", { then }) };
}

/** The toast once it is done: the fact, then what the committer policy did, when it said anything. */
export function initDoneWords(committer) {
  const said = typeof committer === "string" ? committer.trim() : "";
  if (!said) return t("work-init-repository-repository-initialised");
  return t("work-init-repository-repository-initialised-2", { said });
}
