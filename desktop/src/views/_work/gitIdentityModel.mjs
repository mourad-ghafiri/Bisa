import { bornWords } from "./projectOriginModel.mjs";
import { t } from "../../i18n/l10n.mjs";
/**
 * Who commits here, as the desktop reasons about it. The node answers
 * `GET /workstreams/{wid}/git/identity` with the effective `user.name` /
 * `user.email`, where they come from (`local` · `global` · `none`) and the
 * global pair; this module turns that into the three states the card and the
 * commit box act on, and mirrors the vcs crate's validation so the form can
 * refuse before the round trip. The vocabulary is checked against the Rust
 * `IdentitySource` in the test.
 *
 * The ask: a `committer_needed` frame carries why a project is
 * asking, and `committerReasonSentence` says it; the config itself is
 * `gitConfigModel.mjs`'s.
 */

export const IDENTITY_SOURCES = Object.freeze(["local", "global", "none"]);

/** Why a project asks who commits — the engine's `CommitterReason`, in its order. */
export const COMMITTER_REASONS = Object.freeze(["created", "commit_refused", "settlement_refused", "unresolved"]);

/**
 * `local` — set in this repository; `inherited` — resolved from the global
 * config, so every repository on this machine commits as the same person;
 * `missing` — a commit would fail. An unknown or absent view is `missing`: the
 * card asks rather than assumes.
 */
export function identityState(view) {
  if (!view || typeof view !== "object") return "missing";
  if (view.source === "local") return "local";
  if (view.source === "global") return "inherited";
  return "missing";
}

/**
 * The engine frames after which who commits in a repository may have
 * changed — so every surface that shows it re-reads on the same ones:
 * the answer (`committer_set`), the question (`committer_needed`), and the
 * person's git setup moving (`git_setup_changed` — a profile by
 * organization or the global identity gives a repository an author without
 * anyone touching the repository).
 */
const IDENTITY_FRAMES = Object.freeze(["committer_set", "committer_needed", "git_setup_changed"]);

/** Whether an engine frame of this type may have moved who commits. */
export function identityMoved(type) {
  return IDENTITY_FRAMES.includes(type);
}

/** Whether git would accept a commit here. Unknown (`null`) is not a block. */
export function canCommit(view) {
  if (view == null) return true;
  return identityState(view) !== "missing";
}

/**
 * The sentence the commit box shows when identity blocks it, else `null`.
 * `where` names the place the fix lives from the reader's seat — the commit
 * box points at About › Checkout, a rail row at the dialog.
 */
export function identityBlockedReason(view, where = t("work-git-identity-under-about-checkout")) {
  if (canCommit(view)) return null;
  return t("work-git-identity-nobody-set-commit-repository-set-who", { where });
}

/**
 * The door a connected account offers a repository nobody commits in: the
 * verb the card's button wears and the sentence under it, from the view's
 * `suggested` — `null` with an identity, or without an account to suggest.
 * Offered, never assumed: the click is what writes.
 */
export function suggestionWords(view) {
  const s = view?.suggested;
  if (!s || identityState(view) !== "missing" || !s.name || !s.email || !s.login) return null;
  return {
    button: t("work-git-identity-commit", { s: s.name }),
    sentence: t("work-git-identity-account-would-commit-one-click-sets", { login: s.login, s: s.name, email: s.email }),
    ident: { name: String(s.name), email: String(s.email) },
  };
}

/** The global pair can be pinned when there is one and it is not already local. */
export function pinnable(view) {
  return !!view && identityState(view) !== "local" && !!view.global?.name && !!view.global?.email;
}

/**
 * The one click About › Checkout offers on an inherited identity: the verb
 * and the pair it writes into the repository's own config, so the
 * repository keeps its author even if the global one changes. `null` when
 * there is nothing to pin, or it is pinned already.
 */
export function pinOffer(view) {
  if (!pinnable(view)) return null;
  return {
    button: t("work-git-identity-pin-global-pair-here"),
    ident: { name: String(view.global.name), email: String(view.global.email) },
  };
}

/**
 * Why the question is being asked, for the dialog's first line. `origin` is
 * the project's `ProjectOrigin` (`{origin: "workspace" | "goal" | "step", …}`).
 */
export function committerReasonSentence(reason, slug, origin) {
  const born = bornWords(origin).replace(` ${t("work-git-identity-from-goal")}`, ` ${t("work-git-identity-goal")}`);
  switch (reason) {
    case "commit_refused":
      return t("work-git-identity-commit-refused-nobody-set-commit-repository", { slug });
    case "settlement_refused":
      return t("work-git-identity-agent-finished-work-but-nobody-set", { slug });
    case "unresolved":
      return t("work-git-identity-nobody-set-commit-s-repository-git", { slug });
    default:
      return t("work-git-identity-just-created-nobody-set-commit-repository", { slug, born });
  }
}

/**
 * What the card says about where the identity comes from. A profile by
 * organization resolves outside the repository too, so it reads as
 * `inherited` — and the sentence names the profile when the view carries it.
 */
export function identitySentence(view) {
  switch (identityState(view)) {
    case "local":
      return t("work-git-identity-set-repository-every-commit-made-here");
    case "inherited":
      return view?.profile
        ? t("work-git-identity-from-profile-author-organization-s-repositories", { profile: view.profile })
        : t("work-git-identity-inherited-from-global-git-config-pin");
    default:
      return t("work-git-identity-nobody-set-commit-here-git-refuses");
  }
}
