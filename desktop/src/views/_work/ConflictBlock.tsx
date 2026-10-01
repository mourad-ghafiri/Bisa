/**
 * One conflict of a file, as a card (ide/04 §Conflicts, continued): the
 * two versions side by side under their names — *mine* and *theirs* as
 * `conflictSidesModel` names them, git's swap made once there — the base on
 * request, and the four keeps plus *Edit…*. A settled card folds to the
 * text it chose, says which (*kept theirs — feature/login*) and offers
 * *Undo*; an edit opens the editor seeded with both sides. The facts are
 * `conflictBlocksModel.mjs`'s; the card only draws and asks.
 */
import { useState } from "react";
import { Button, Chip, CodeEditor, ICON, Menu, Tooltip, cn } from "../../ui";
import type { MenuItem } from "../../ui";
import { blockWords, textFor } from "./conflictBlocksModel.mjs";
import type { Choice, ConflictBlock as Block, Made } from "./conflictBlocksModel.mjs";
import type { Sides } from "./conflictSidesModel.mjs";
import { SideSwatch } from "./ResolveCard";
import { t } from "../../i18n/l10n.mjs";

/** A run of lines, numbered, as a pane. */
function Pane({ text, tone, label }: { text: string; tone: "accent" | "ok" | "neutral"; label: string }) {
  const lines = text.replace(/\n$/, "").split("\n");
  const empty = text === "";
  return (
    <div className={cn("min-w-0 flex-1 rounded-control border", tone === "accent" ? "border-accent/40 bg-accent-soft/20" : tone === "ok" ? "border-ok/40 bg-ok-soft/20" : "border-border bg-surface-2/40")} aria-label={label}>
      {empty ? (
        <p className="px-2 py-1.5 text-2xs italic text-text-dim">{t("work-conflict-block-nothing-side-has-no-lines-here")}</p>
      ) : (
        <pre className="max-h-64 overflow-auto px-2 py-1.5 font-mono text-2xs leading-5 text-text">
          {lines.map((l, i) => (
            <div key={i} className="flex gap-2">
              <span className="w-6 shrink-0 select-none text-right text-text-dim/60">{i + 1}</span>
              <span className="whitespace-pre">{l || " "}</span>
            </div>
          ))}
        </pre>
      )}
    </div>
  );
}

export function ConflictBlock({
  block,
  index,
  total,
  sides,
  swapped,
  made,
  current,
  path,
  disabled = false,
  onChoose,
  onUnchoose,
  onFocus,
  onAsk,
}: {
  block: Block;
  /** One-based, of `total`, for the card's title. */
  index: number;
  total: number;
  sides: Sides;
  /** Mine is git's theirs — a rebase. */
  swapped: boolean;
  made: Made | null;
  /** The card the keys act on. */
  current: boolean;
  path: string;
  disabled?: boolean;
  onChoose: (choice: Choice, text?: string) => void;
  onUnchoose: () => void;
  onFocus: () => void;
  onAsk: () => void;
}) {
  const [editing, setEditing] = useState<string | null>(null);
  const [showBase, setShowBase] = useState(false);
  const mine = swapped ? block.theirs : block.ours;
  const theirs = swapped ? block.ours : block.theirs;
  const bothItems: MenuItem[] = [
    { label: t("work-conflict-block-mine-first-then-theirs"), onSelect: () => onChoose("both") },
    { label: t("work-conflict-block-theirs-first-then-mine"), onSelect: () => onChoose("both_reversed") },
  ];
  const title = t("work-conflict-block-conflict", { index, total });
  const settled = made !== null && editing === null;
  return (
    <section
      data-conflict-block={block.id}
      tabIndex={0}
      onFocus={onFocus}
      onMouseDown={onFocus}
      aria-label={t("work-conflict-block-words", { title, path })}
      aria-current={current ? "true" : undefined}
      className={cn("flex flex-col gap-1.5 rounded-control border px-2.5 py-2 outline-none", current ? "border-accent/60 ring-1 ring-accent/30" : settled ? "border-border" : "border-warn/40", settled && "bg-surface")}
    >
      <div className="flex flex-wrap items-center gap-2 text-2xs">
        <span className="font-semibold text-text">{title}</span>
        {settled ? (
          <Chip tone="ok" icon={ICON.check}>
            {blockWords(made, sides)}
          </Chip>
        ) : (
          <Chip tone="warn">{t("work-conflict-block-settle")}</Chip>
        )}
        <span className="flex-1" />
        {settled ? (
          <Button size="sm" variant="ghost" disabled={disabled} onClick={onUnchoose}>{t("work-conflict-block-undo")}</Button>
        ) : (
          <>
            {block.base !== null && (
              <Button size="sm" variant="ghost" aria-pressed={showBase} onClick={() => setShowBase((v) => !v)}>
                {showBase ? t("work-conflict-block-hide-base") : t("work-conflict-block-show-base")}
              </Button>
            )}
            <Tooltip label={t("work-conflict-block-ask-agent-explain-conflict-suggest-merged")}>
              <span className="inline-flex">
                <Button size="sm" variant="ghost" onClick={onAsk}>
                  <ICON.agent size={11} aria-hidden />{t("work-conflict-block-ask-agent")}</Button>
              </span>
            </Tooltip>
          </>
        )}
      </div>
      {settled ? (
        <Pane text={textFor(block, made, swapped)} tone="neutral" label={t("work-conflict-block-text-kept")} />
      ) : editing !== null ? (
        <>
          <div className="h-48 overflow-hidden rounded-control border border-border">
            <CodeEditor value={editing} path={path} onChange={setEditing} className="h-full" />
          </div>
          <div className="flex items-center gap-2">
            <Button size="sm" variant="primary" onClick={() => (onChoose("edit", editing), setEditing(null))}>{t("work-conflict-block-keep-text")}</Button>
            <Button size="sm" variant="ghost" onClick={() => setEditing(null)}>{t("work-agent-editor-cancel")}</Button>
            <span className="text-2xs text-text-dim">{t("work-conflict-block-started-from-both-sides")}</span>
          </div>
        </>
      ) : (
        <>
          <div className="flex flex-col gap-1.5 md:flex-row">
            <div className="flex min-w-0 flex-1 flex-col gap-1">
              <div className="text-2xs">
                <SideSwatch side={sides.mine} />
              </div>
              <Pane text={mine} tone="accent" label={t("work-conflict-block-mine", { mine: sides.mine.name })} />
            </div>
            <div className="flex min-w-0 flex-1 flex-col gap-1">
              <div className="text-2xs">
                <SideSwatch side={sides.theirs} />
              </div>
              <Pane text={theirs} tone="ok" label={t("work-conflict-block-theirs", { theirs: sides.theirs.name })} />
            </div>
          </div>
          {showBase && block.base !== null && (
            <div className="flex flex-col gap-1">
              <span className="text-2xs text-text-dim">{t("work-conflict-block-base-what-both-sides-started-from")}</span>
              <Pane text={block.base} tone="neutral" label={t("work-conflict-block-base")} />
            </div>
          )}
          <div className="flex flex-wrap items-center gap-1.5">
            <Tooltip label={t("work-conflict-block-keep-s-lines-here", { mine: sides.mine.name })}>
              <span className="inline-flex">
                <Button size="sm" disabled={disabled} onClick={() => onChoose("mine")}>{t("work-conflict-block-keep-mine")}</Button>
              </span>
            </Tooltip>
            <Tooltip label={t("work-conflict-block-keep-s-lines-here-2", { theirs: sides.theirs.name })}>
              <span className="inline-flex">
                <Button size="sm" disabled={disabled} onClick={() => onChoose("theirs")}>{t("work-conflict-block-keep-theirs")}</Button>
              </span>
            </Tooltip>
            <Menu
              items={bothItems}
              trigger={
                <Button size="sm" disabled={disabled}>{t("work-conflict-block-keep-both")}</Button>
              }
            />
            <Tooltip label={t("work-conflict-block-write-lines-yourself-starting-from-both")}>
              <span className="inline-flex">
                <Button size="sm" variant="ghost" disabled={disabled} onClick={() => setEditing(textFor(block, { choice: "both" }, swapped))}>{t("work-conflict-block-edit")}</Button>
              </span>
            </Tooltip>
          </div>
        </>
      )}
    </section>
  );
}
