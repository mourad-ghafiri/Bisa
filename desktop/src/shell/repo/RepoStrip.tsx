/**
 * A folder repository's strip — the notes' and the drawings' (ide/04, *The
 * notes repository*; 19 — Drawings) — at the foot of a list: one line — what
 * changed, what origin lacks, when the last commit was — with *Commit…* and a
 * ⋮ menu holding the outward verbs. *Commit…* unfolds a composer in place:
 * the message, *Suggest*, *Commit* (⌘Enter), one reason line when it is off.
 *
 * One component for both folders: the caller hands it the routes as a
 * `RepoApi` (`api.notesRepo`, `api.drawingsRepo`), the folder's word for a
 * commit subject, and whether the folder offers a pull — the drawings' does
 * not, since a drawing's record arrives by sync, and the menu says so.
 *
 * Nothing here nags. A record that was never committed is a count on a line,
 * not a banner; the editor has no git chrome at all. The strip reads the
 * status when it mounts and after each act, and whenever the overlay says a
 * record changed (`tick`), so the count is honest without polling.
 *
 * The verbs are the IDE's (`gitWords`), the suggestion's shape is the
 * IDE's (`gitFiles.suggestionOutcome`), and the one thing that is not the
 * IDE's is a gate: these folders belong to no project and no goal, so a push
 * is your own act from a button.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { ApiError } from "../../api";
import type { RepoApi } from "../../api";
import type { FolderRepo } from "../../types";
import { Button, Dialog, Field, KeyHint, Menu, PromptDialog, TextArea, TextInput, WorkingDot, toaster } from "../../ui";
import { ICON } from "../../ui/icons";
import { suggestionOutcome } from "../../views/_work/gitFiles.mjs";
import { VERB } from "../../views/_work/gitWords.mjs";
import { ReasonLine } from "../../views/_work/ReasonLine";
import {
  commitBlockedReason,
  defaultMessage,
  identityRefusal,
  menuVerbs,
  nobodyToCommit,
  pullOutcomeWords,
  pushOutcomeWords,
  remoteRefusal,
  stripLine,
} from "./repoStripModel.mjs";
import { t } from "../../i18n/l10n.mjs";

type Busy = "suggest" | "commit" | "push" | "fetch" | "pull" | "remote" | "identity";

const BUSY_WORD: Record<Busy, string> = {
  suggest: t("notes-notes-git-strip-asking"),
  commit: t("notes-notes-git-strip-committing"),
  push: t("shell-repo-strip-pushing"),
  fetch: t("shell-repo-strip-fetching"),
  pull: t("shell-repo-strip-pulling"),
  remote: t("notes-notes-git-strip-setting-origin"),
  identity: t("notes-notes-git-strip-setting-who-commits"),
};

function said(e: unknown, fallback: string): string {
  return e instanceof ApiError ? e.message : fallback;
}

export function RepoStrip({
  repo,
  subject,
  pulls,
  tick,
}: {
  /** The folder's routes — `api.notesRepo` or `api.drawingsRepo`. */
  repo: RepoApi;
  /** The folder's word for a commit subject nobody wrote: `Notes` · `Drawings`. Content, never translated. */
  subject: string;
  /** Whether the folder offers a pull; the drawings' does not. */
  pulls: boolean;
  /** Bumped by the overlay after every record write, so the count re-reads. */
  tick: number;
}) {
  const [status, setStatus] = useState<FolderRepo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [composing, setComposing] = useState(false);
  const [message, setMessage] = useState("");
  const [note, setNote] = useState<string | null>(null);
  const [busy, setBusy] = useState<Busy | null>(null);
  const [originOpen, setOriginOpen] = useState(false);
  const [whoOpen, setWhoOpen] = useState(false);
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  const load = useCallback(async () => {
    try {
      const s = await repo.status();
      if (!alive.current) return;
      setStatus(s);
      setError(null);
    } catch (e) {
      if (!alive.current) return;
      setError(said(e, t("notes-notes-git-strip-could-not-read-notes-repository")));
    }
  }, [repo]);
  useEffect(() => {
    void load();
  }, [load, tick]);

  /** One act at a time; the status after it is the truth, whatever it said. */
  const act = useCallback(
    async (kind: Busy, run: () => Promise<void>) => {
      if (busy !== null) return;
      setBusy(kind);
      try {
        await run();
      } catch (e) {
        toaster.error(said(e, t("notes-notes-git-strip-could-not", { kind })));
      } finally {
        if (alive.current) setBusy(null);
        await load();
      }
    },
    [busy, load],
  );

  const commit = () =>
    act("commit", async () => {
      const c = await repo.commit(message.trim() === "" ? defaultMessage(undefined, subject) : message);
      toaster.ok(t("notes-notes-git-strip-committed", { short: c.short, subject: c.subject }));
      setMessage("");
      setNote(null);
      setComposing(false);
    });
  const suggest = () =>
    act("suggest", async () => {
      const out = suggestionOutcome(await repo.suggest());
      if (out.message !== null) setMessage(out.message);
      setNote(out.note);
    });
  const push = () =>
    act("push", async () => {
      toaster.ok(pushOutcomeWords(await repo.push()));
    });
  const fetch = () =>
    act("fetch", async () => {
      toaster.info(t("notes-notes-git-strip-fetched", { notesGitFetch: stripLine(await repo.fetch()) }));
    });
  const pull = () =>
    act("pull", async () => {
      const before = status;
      const outcome = await repo.pull();
      toaster.ok(before ? pullOutcomeWords(before, outcome.status) : t("notes-notes-git-strip-pulled"));
    });
  const setRemote = (url: string) =>
    act("remote", async () => {
      await repo.setRemote(url.trim());
      toaster.ok(t("notes-notes-git-strip-origin", { url: url.trim() }));
      setOriginOpen(false);
    });
  const setIdentity = (name: string, email: string) =>
    act("identity", async () => {
      await repo.setIdentity(name.trim(), email.trim());
      toaster.ok(t("notes-notes-git-strip-commits-notes", { trim: name.trim() }));
      setWhoOpen(false);
    });

  if (error) {
    return (
      <footer className="shrink-0 border-t border-border px-3 py-1.5">
        <ReasonLine tone="warn" door={{ label: t("notes-notes-git-strip-retry"), onClick: () => void load() }}>
          {error}
        </ReasonLine>
      </footer>
    );
  }
  if (!status) return null;

  const verbs = menuVerbs(status, pulls);
  const nobody = nobodyToCommit(status);
  const blocked = commitBlockedReason(status, message.trim() === "" ? defaultMessage(undefined, subject) : message);
  /**
   * An off verb stays selectable: choosing it says why, in a toast. A disabled
   * item is a reason nobody reads.
   */
  const verb = (label: string, state: { on: boolean; reason: string | null }, run: () => void) => ({
    label,
    onSelect: () => (state.on ? run() : toaster.info(state.reason ?? t("notes-notes-git-strip-off", { label }))),
  });
  const items = [
    verb(VERB.push, verbs.push, () => void push()),
    verb(VERB.fetch, verbs.fetch, () => void fetch()),
    // A folder with no pull keeps the row: choosing it says why, like any off verb.
    verb(VERB.pull, verbs.pull, () => void pull()),
    { label: status.remote === null ? t("notes-notes-git-strip-set-origin") : t("notes-notes-git-strip-change-origin"), onSelect: () => setOriginOpen(true), separatorBefore: true },
    { label: t("notes-notes-git-strip-who-commits"), onSelect: () => setWhoOpen(true) },
  ];

  return (
    <footer aria-label={t("notes-notes-git-strip-notes-repository")} className="flex shrink-0 flex-col gap-1.5 border-t border-border px-3 py-1.5">
      <div className="flex items-center gap-1.5">
        <ICON.branch size={12} aria-hidden className="shrink-0 text-text-dim" />
        <span className="min-w-0 flex-1 truncate text-2xs text-text-dim" title={status.upstream ?? status.branch ?? undefined}>
          {stripLine(status)}
        </span>
        {busy !== null && busy !== "commit" && busy !== "suggest" && <WorkingDot title={BUSY_WORD[busy]} />}
        {!composing && (
          <Button size="sm" variant="ghost" disabled={busy !== null} onClick={() => setComposing(true)}>
            {t("notes-git-strip-verb-ellipsis", { verb: VERB.commit })}
          </Button>
        )}
        <Menu
          label={t("notes-notes-git-strip-more-notes-repository")}
          items={items}
          trigger={
            <button
              type="button"
              aria-label={t("notes-notes-git-strip-more-notes-repository")}
              className="anim shrink-0 rounded-control p-1 text-text-dim hover:bg-surface-2 hover:text-text"
            >
              <ICON.more size={13} aria-hidden />
            </button>
          }
        />
      </div>

      {composing && (
        <div className="flex flex-col gap-1.5">
          <TextArea
            value={message}
            rows={2}
            placeholder={defaultMessage(undefined, subject)}
            aria-label={t("notes-notes-git-strip-commit-message-notes")}
            autoFocus
            onChange={(e) => setMessage(e.target.value)}
            onKeyDown={(e) => {
              if ((e.metaKey || e.ctrlKey) && e.key === "Enter" && blocked === null && busy === null) void commit();
              if (e.key === "Escape") setComposing(false);
            }}
          />
          {note && (
            <p className="text-2xs text-text-dim" role="status">
              {note}
            </p>
          )}
          <div className="flex items-center gap-1.5">
            <Button size="sm" variant="ghost" disabled={busy !== null || status.changed === 0} onClick={() => void suggest()} title={t("notes-notes-git-strip-draft-message-from-what-changed-read")}>
              <ICON.agent size={12} aria-hidden />
              {busy === "suggest" ? t("notes-notes-git-strip-asking") : t("notes-notes-git-strip-suggest")}
            </Button>
            {(busy === "suggest" || busy === "commit") && <WorkingDot title={BUSY_WORD[busy]} />}
            <span className="flex-1" />
            <Button size="sm" variant="ghost" disabled={busy !== null} onClick={() => setComposing(false)}>{t("notes-notes-git-strip-cancel")}</Button>
            <KeyHint combo="Mod+Enter" />
            <Button size="sm" variant="primary" disabled={blocked !== null || busy !== null} onClick={() => void commit()}>
              {busy === "commit" ? t("notes-notes-git-strip-committing") : VERB.commit}
            </Button>
          </div>
          {blocked && (
            <ReasonLine door={nobody ? { label: t("notes-notes-git-strip-who-commits"), onClick: () => setWhoOpen(true) } : null}>{blocked}</ReasonLine>
          )}
        </div>
      )}

      <PromptDialog
        open={originOpen}
        onClose={() => setOriginOpen(false)}
        onSubmit={(v) => void setRemote(v)}
        title={status.remote === null ? t("notes-notes-git-strip-set-origin-2") : t("notes-notes-git-strip-change-origin-2")}
        description={t("notes-notes-git-strip-where-notes-push-code-host-url")}
        label={t("notes-notes-git-strip-repository")}
        initial={status.remote ?? ""}
        placeholder="git@host:you/notes.git" // for the machine
        mono
        submitLabel={status.remote === null ? t("notes-notes-git-strip-set") : t("notes-notes-git-strip-change")}
        busy={busy === "remote"}
        validate={remoteRefusal}
      />
      <WhoCommitsDialog open={whoOpen} status={status} busy={busy === "identity"} onClose={() => setWhoOpen(false)} onSubmit={(n, e) => void setIdentity(n, e)} />
    </footer>
  );
}

/**
 * The pair the notes repository commits as, set on the repository itself so
 * it holds whatever the global identity does. Prefilled from the global pair
 * when there is one, so pinning it is a click.
 */
function WhoCommitsDialog({
  open,
  status,
  busy,
  onClose,
  onSubmit,
}: {
  open: boolean;
  status: FolderRepo;
  busy: boolean;
  onClose: () => void;
  onSubmit: (name: string, email: string) => void;
}) {
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  useEffect(() => {
    if (!open) return;
    setName(status.identity.name ?? status.identity.global?.name ?? "");
    setEmail(status.identity.email ?? status.identity.global?.email ?? "");
  }, [open, status]);
  const refusal = identityRefusal(name, email);
  const submit = () => {
    if (refusal === null && !busy) onSubmit(name, email);
  };
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={t("notes-notes-git-strip-who-commits-notes")}
      description={
        status.identity.source === "global"
          ? t("notes-notes-git-strip-commits-signed-global-git-identity-setting")
          : t("notes-notes-git-strip-set-notes-repository-only-project-s")
      }
      width="max-w-sm"
      footer={
        <>
          <Button size="sm" variant="ghost" onClick={onClose} disabled={busy}>{t("notes-notes-git-strip-cancel")}</Button>
          <Button size="sm" variant="primary" onClick={submit} disabled={busy || refusal !== null}>
            {busy ? t("notes-notes-git-strip-setting") : t("notes-notes-git-strip-set")}
          </Button>
        </>
      }
    >
      <form
        className="flex flex-col gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <Field label={t("notes-notes-git-strip-name")}>
          <TextInput value={name} onChange={(e) => setName(e.target.value)} autoFocus />
        </Field>
        <Field label={t("notes-notes-git-strip-email")} hint={refusal ?? undefined}>
          <TextInput type="email" value={email} onChange={(e) => setEmail(e.target.value)} />
        </Field>
      </form>
    </Dialog>
  );
}
