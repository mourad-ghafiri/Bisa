/**
 * The conflict document (ide/04 §Conflicts, continued): a conflicted path
 * resolved block by block, in the person's words. A legend names the two
 * sides — *main — mine*, *feature/login — theirs*, git's swap under a
 * rebase made once in `conflictSidesModel` — and says what is happening;
 * then one card per conflict (`ConflictBlock`), the runs everybody agrees
 * on folded between them, *Keep all mine / theirs* for the whole file, a
 * **Review** of the result against either side or the base — editable, an
 * edit there becoming the result — and **Mark resolved**, lit only when
 * every conflict is settled and no marker remains: the composed text saved
 * (a compare-and-swap on the hash it was read at) and the path staged,
 * through `gitOps.markResolved`. A path one side deleted, a file added on
 * both sides, a binary — anything that is a choice, not a merge — is one
 * card with its two explicit choices (`kindChoices`), each a consented
 * `resolve {take}`. *Ask an agent* hands a block to the Agent pane as a
 * context chip with a drafted question; the agent explains and suggests,
 * staging and Continue stay here. The keys (`next_conflict`, `keep_mine`,
 * …) reach the document as `CONFLICT_COMMAND` events while focus is in it.
 * The choices and the edit persist per file hash in the checkout's session
 * (`gitPanelStore`), so a tab switch loses nothing; a file that moved on
 * disk since is a fresh start.
 *
 * *Abort* is not here: the Resolve card above the Git views is the one door
 * back. Two doors would be two states to keep in step.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { COMPOSER_DRAFT } from "../../ui/Composer";
import { api } from "../../api";
import type { GitFileRow, GitInProgress, GitOperationFacts } from "../../types";
import { Button, DiffEditor, EmptyState, ErrorNote, ICON, SegmentedControl, SkeletonRows, Tooltip, useToast } from "../../ui";
import { CONFLICT_COMMAND } from "../../shell/shortcuts";
import { attachContext } from "../_workbench/agentPaneStore";
import { selectionChip } from "../_workbench/contextChips.mjs";
import { ensureConversation } from "../_workbench/conversationsStore";
import { showRightPanel } from "../_workbench/rightPanelStore";
import { rootKey } from "../_workbench/workbenchModel.mjs";
import { FOLD_UNDER, choose, chooseAll, compose, conflictsOf, documentWords, foldWords, hasMarkers, lineCount, nextUnsettled, parseConflicts, previousUnsettled, progress, unchoose } from "./conflictBlocksModel.mjs";
import type { Choice, Choices, Segment } from "./conflictBlocksModel.mjs";
import { agentQuestion, isSwapped, isWholeFileKind, kindChoices, kindWords, sidesOf } from "./conflictSidesModel.mjs";
import { ConflictBlock } from "./ConflictBlock";
import { fileDraftKey } from "./gitPanelModel.mjs";
import * as ops from "./gitOps";
import { useGitSession, useSessionDraft } from "./gitPanelStore";
import { SideSwatch } from "./ResolveCard";
import { useAsync } from "./useAsync";
import { t } from "../../i18n/l10n.mjs";

type Against = "theirs" | "base" | "mine";

/** What the session keeps for one conflicted file: the choices, and the review's edited result when one was made. */
interface Draft {
  choices: Choices;
  final: string | null;
}
const EMPTY_DRAFT: Draft = { choices: {}, final: null };

/** A run everybody agrees on, folded past a few lines. */
function Run({ text, storeKey }: { text: string; storeKey: string }) {
  const [open, setOpen] = useState(false);
  const lines = lineCount(text);
  if (lines === 0) return null;
  if (lines < FOLD_UNDER || open) {
    return (
      <pre className="whitespace-pre-wrap rounded-control bg-surface-2/40 px-2.5 py-1 font-mono text-2xs leading-5 text-text-dim" data-run={storeKey}>
        {text.replace(/\n$/, "")}
        {lines >= FOLD_UNDER && (
          <button type="button" className="anim ml-2 text-text-dim underline-offset-2 hover:text-text hover:underline" onClick={() => setOpen(false)}>{t("work-conflict-view-fold")}</button>
        )}
      </pre>
    );
  }
  return (
    <button type="button" className="anim rounded-control bg-surface-2/40 px-2.5 py-1 text-left font-mono text-2xs text-text-dim hover:bg-surface-2 hover:text-text" onClick={() => setOpen(true)}>
      {foldWords(lines)}
    </button>
  );
}

export function ConflictView({
  wid,
  pid,
  path,
  inProgress = null,
  facts = null,
  onResolved,
  onSettled,
}: {
  wid: string;
  /** The project, for the agent door's conversation. */
  pid: string | null;
  path: string;
  /** What git is in the middle of — the sides read differently under a rebase. */
  inProgress?: GitInProgress | null;
  /** The node's facts about it — the sides' names. */
  facts?: GitOperationFacts | null;
  /** The path is staged from the composed text; the caller's file rows are fresh. */
  onResolved: (files: GitFileRow[]) => void;
  /** The path was settled whole through the store; the session's rows are fresh. */
  onSettled: (path: string) => void;
}) {
  const toast = useToast();
  const scope = rootKey("workstream", wid);
  const session = useGitSession(scope);
  const conflict = useAsync((s) => api.gitConflict(wid, path, s), [wid, path, session.stale]);
  const hash = conflict.data?.hash ?? null;
  const [draft, setDraft] = useSessionDraft<Draft>(fileDraftKey(scope, path, `conflict:${hash ?? "?"}`), EMPTY_DRAFT);
  const [at, setAt] = useState<string | null>(null);
  const [reviewing, setReviewing] = useState(false);
  const [against, setAgainst] = useState<Against>("theirs");

  const sides = useMemo(() => sidesOf(inProgress, facts), [inProgress, facts]);
  const swapped = isSwapped(inProgress);
  const parsed = useMemo(() => parseConflicts(conflict.data?.text ?? ""), [conflict.data?.text]);
  const blocks = useMemo(() => conflictsOf(parsed.segments), [parsed.segments]);
  const p = progress(parsed.segments, draft.choices);
  const composed = useMemo(() => compose(parsed.segments, draft.choices, swapped), [parsed.segments, draft.choices, swapped]);
  const result = draft.final ?? composed;
  const markers = hasMarkers(result);
  const busy = session.busy !== null;
  const ready = !busy && conflict.data != null && !markers && (draft.final !== null || p.settled === p.total);

  // The card the keys act on: the first unsettled one, until the person moves.
  useEffect(() => {
    if (at === null || !blocks.some((b) => b.id === at)) setAt(nextUnsettled(parsed.segments, draft.choices, null));
  }, [at, blocks, parsed.segments, draft.choices]);

  const make = useCallback(
    (id: string, choice: Choice, text?: string) => {
      setDraft((d) => ({ choices: choose(d.choices, id, choice, text), final: null }));
      setAt((cur) => nextUnsettled(parsed.segments, choose(draft.choices, id, choice, text), cur ?? id));
    },
    [draft.choices, parsed.segments, setDraft],
  );
  const undo = (id: string) => setDraft((d) => ({ choices: unchoose(d.choices, id), final: null }));
  const all = (choice: Choice) => setDraft({ choices: chooseAll(parsed.segments, choice), final: null });
  const go = (dir: 1 | -1) => {
    const next = dir === 1 ? nextUnsettled(parsed.segments, draft.choices, at) : previousUnsettled(parsed.segments, draft.choices, at);
    if (next) {
      setAt(next);
      document.querySelector<HTMLElement>(`[data-conflict-block="${next}"]`)?.scrollIntoView({ block: "nearest" });
    }
  };

  const resolve = () => {
    if (!ready) return;
    void ops.markResolved(scope, wid, path, result, hash).then((ran) => {
      if (!ran) return;
      const rows = session.files ?? [];
      onResolved(rows.filter((f) => f.path !== path || !f.conflicted));
    });
  };

  const ask = async (id: string) => {
    const block = blocks.find((b) => b.id === id);
    if (!block) return;
    const where = { at: blocks.indexOf(block) + 1, of: blocks.length };
    const q = agentQuestion(path, block, sides, where);
    // The block's place in the file, for the chip's range: the lines before it.
    let line = 1;
    for (const s of parsed.segments) {
      if (s.kind === "conflict" && s.id === id) break;
      line += lineCount(s.kind === "conflict" ? s.raw : s.text);
    }
    attachContext(selectionChip(path, line, line + lineCount(block.raw) - 1, q.text), `workstream:${wid}`);
    showRightPanel("agents", `workstream:${wid}`);
    if (pid) {
      try {
        const landing = await ensureConversation(wid, pid);
        window.dispatchEvent(new CustomEvent(COMPOSER_DRAFT, { detail: { scope: landing.id, text: q.question } }));
      } catch {
        // No conversation to draft into: the chip is attached all the same.
      }
    }
    toast.ok(t("work-conflict-view-conflict-agent-pane-send-question-write"));
  };

  // The keys: while focus is in the document, the chords reach it as events.
  useEffect(() => {
    const onCommand = (e: Event) => {
      const cmd = (e as CustomEvent<{ command: string }>).detail?.command;
      if (!document.querySelector("[data-conflict-doc]:focus-within")) return;
      switch (cmd) {
        case "next_conflict":
          return go(1);
        case "previous_conflict":
          return go(-1);
        case "keep_mine":
          return at && make(at, "mine");
        case "keep_theirs":
          return at && make(at, "theirs");
        case "keep_both":
          return at && make(at, "both");
        case "mark_resolved":
          return resolve();
        default:
          return undefined;
      }
    };
    window.addEventListener(CONFLICT_COMMAND, onCommand);
    return () => window.removeEventListener(CONFLICT_COMMAND, onCommand);
  });

  if (conflict.loading && !conflict.data) return <SkeletonRows rows={4} />;
  if (conflict.error && !conflict.data) return <ErrorNote error={conflict.error} retry={conflict.reload} />;
  const c = conflict.data;
  if (!c) return null;

  // A choice, not a merge: a side deleted it, a binary, added on both sides
  // with no text to merge (a binary again). Two explicit outcomes.
  const wholeFile = isWholeFileKind(c.kind ?? null) || c.binary;
  if (wholeFile) {
    const kw = kindWords(c.kind ?? null, sides);
    const choices = kindChoices(c.kind ?? null, sides);
    return (
      <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-2" data-conflict-doc="" tabIndex={0}>
        <Legend sides={sides} explain={sides.explain} />
        <EmptyState
          title={c.binary && !isWholeFileKind(c.kind ?? null) ? t("work-conflict-view-binary-file-conflicts") : t("work-conflict-view-file", { short: kw.short })}
          hint={c.binary && !isWholeFileKind(c.kind ?? null) ? t("work-conflict-view-there-no-text-merge-keep-one") : t("work-conflict-view-there-nothing-merge-line-line-choose", { sentence: kw.sentence })}
          className="py-3"
          action={
            <div className="flex flex-wrap gap-1.5">
              {choices.map((ch) => (
                <Tooltip key={ch.id} label={ch.hint}>
                  <span className="inline-flex">
                    <Button size="sm" variant={ch.danger ? "ghost" : "default"} className={ch.danger ? "hover:text-danger" : undefined} disabled={busy} onClick={() => void ops.resolve(scope, wid, path, ch.take, sides).then((ran) => ran && onSettled(path))}>
                      {ch.label}
                    </Button>
                  </span>
                </Tooltip>
              ))}
            </div>
          }
        />
      </div>
    );
  }

  const againstText = against === "theirs" ? (swapped ? c.ours : c.theirs) : against === "mine" ? (swapped ? c.theirs : c.ours) : c.base;

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-2" data-conflict-doc="" tabIndex={0}>
      <Legend sides={sides} explain={sides.explain} />
      <div className="flex flex-wrap items-center gap-2 text-2xs">
        <span className={p.total > 0 && p.settled < p.total ? "text-text" : "text-ok"}>{documentWords(p)}</span>
        {parsed.problem && <span className="text-warn">{parsed.problem}</span>}
        <span className="flex-1" />
        {blocks.length > 1 && (
          <>
            <Tooltip label={t("work-conflict-view-previous-conflict-still-settle-alt-up")}>
              <span className="inline-flex">
                <Button size="sm" variant="ghost" className="h-6" onClick={() => go(-1)}>
                  <ICON.up size={12} aria-hidden />
                </Button>
              </span>
            </Tooltip>
            <Tooltip label={t("work-conflict-view-next-conflict-still-settle-alt-down")}>
              <span className="inline-flex">
                <Button size="sm" variant="ghost" className="h-6" onClick={() => go(1)}>
                  <ICON.down size={12} aria-hidden />
                </Button>
              </span>
            </Tooltip>
            <Tooltip label={t("work-conflict-view-every-conflict-becomes-s-lines", { mine: sides.mine.name })}>
              <span className="inline-flex">
                <Button size="sm" variant="ghost" className="h-6" disabled={busy} onClick={() => all("mine")}>{t("work-conflict-view-keep-all-mine")}</Button>
              </span>
            </Tooltip>
            <Tooltip label={t("work-conflict-view-every-conflict-becomes-s-lines-2", { theirs: sides.theirs.name })}>
              <span className="inline-flex">
                <Button size="sm" variant="ghost" className="h-6" disabled={busy} onClick={() => all("theirs")}>{t("work-conflict-view-keep-all-theirs")}</Button>
              </span>
            </Tooltip>
          </>
        )}
        <Tooltip label={t("work-conflict-view-whole-file-will-saved-against-side")}>
          <span className="inline-flex">
            <Button size="sm" variant="ghost" className="h-6 aria-pressed:bg-selected aria-pressed:text-text" aria-pressed={reviewing} onClick={() => setReviewing((v) => !v)}>
              <ICON.inspect size={12} aria-hidden />{t("work-conflict-view-review")}</Button>
          </span>
        </Tooltip>
        <Tooltip label={ready ? t("work-conflict-view-save-result-stage-path-mod-alt") : markers ? t("work-conflict-view-conflict-markers-still-result-settle-every") : p.total > 0 ? t("work-conflict-view-conflict-conflicts-still-settle", { settled: p.total - p.settled }) : t("work-conflict-view-save-result-stage-path")}>
          <span className="inline-flex">
            <Button size="sm" variant="primary" className="h-6" disabled={!ready} onClick={resolve}>
              {session.busy === "resolve" ? t("work-conflict-view-resolving") : t("work-conflict-view-mark-resolved")}
            </Button>
          </span>
        </Tooltip>
      </div>
      {reviewing ? (
        <div className="flex min-h-0 flex-1 flex-col gap-1.5">
          <div className="flex flex-wrap items-center gap-2 text-2xs text-text-dim">
            <span>{t("work-conflict-view-result-right-will-saved-edit-here")}</span>
            <span className="flex-1" />
            <SegmentedControl
              label={t("work-conflict-view-compare-against")}
              size="sm"
              value={against}
              onChange={(x) => setAgainst(x as Against)}
              options={[
                { id: "theirs", label: t("work-conflict-view-theirs", { theirs: sides.theirs.name }) },
                { id: "base", label: t("work-conflict-view-base") },
                { id: "mine", label: t("work-conflict-view-mine", { mine: sides.mine.name }) },
              ]}
            />
            {draft.final !== null && (
              <Button size="sm" variant="ghost" className="h-6" onClick={() => setDraft((d) => ({ ...d, final: null }))}>{t("work-conflict-view-back-choices")}</Button>
            )}
          </div>
          <DiffEditor original={againstText ?? ""} modified={result} path={path} onModifiedChange={(v) => setDraft((d) => ({ ...d, final: v }))} className="min-h-64 min-w-0 flex-1 rounded-control border border-border" />
        </div>
      ) : (
        <div className="flex min-h-0 flex-1 flex-col gap-1.5 overflow-auto">
          {parsed.segments.map((s: Segment, i) =>
            s.kind === "text" ? (
              <Run key={`t${i}`} text={s.text} storeKey={`${path}:${i}`} />
            ) : (
              <ConflictBlock
                key={s.id}
                block={s}
                index={blocks.indexOf(s) + 1}
                total={blocks.length}
                sides={sides}
                swapped={swapped}
                made={draft.choices[s.id] ?? null}
                current={at === s.id}
                path={path}
                disabled={busy}
                onChoose={(choice, text) => make(s.id, choice, text)}
                onUnchoose={() => undo(s.id)}
                onFocus={() => setAt(s.id)}
                onAsk={() => void ask(s.id)}
              />
            ),
          )}
        </div>
      )}
    </div>
  );
}

/** The two sides, named, and the sentence that says what is happening. */
function Legend({ sides, explain }: { sides: ReturnType<typeof sidesOf>; explain: string }) {
  return (
    <div className="flex flex-col gap-1 rounded-control bg-surface-2/50 px-3 py-2 text-2xs">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1">
        <SideSwatch side={sides.mine} />
        <span className="text-text-dim">{sides.mine.role}</span>
        <span className="text-text-dim">·</span>
        <SideSwatch side={sides.theirs} />
        <span className="text-text-dim">{sides.theirs.role}</span>
      </div>
      <p className="text-text-dim">{explain}</p>
    </div>
  );
}
