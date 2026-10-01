/**
 * A confirmation that offers **several acts**, not a yes or a no.
 *
 * Three verbs in a footer is a row of buttons whose meaning lives in a
 * paragraph above them, and one more is a row that does not fit. Here each
 * act is a row of its own: its name, and one sentence saying what it does,
 * the destructive one last and toned as such. The footer holds Cancel alone.
 *
 * An `alertdialog`, like `ConfirmDialog`, for the same reason: a stray click
 * outside must not dismiss a question about removing something. The focus
 * opens on the first act that destroys nothing (`choiceDialogModel.mjs`);
 * Up and Down move between the acts, Enter and Space take the focused one,
 * Escape cancels. An act that cannot be taken here is still shown, held, with
 * its reason in place of its sentence — a missing option reads as a bug, a
 * held one as a rule.
 *
 * Taking an act does not close the dialog: the caller does, by `open`. An act
 * may lead to a second question before anything runs, and a dialog that shut
 * itself first would take that question's subject away with it.
 */

import * as A from "@radix-ui/react-alert-dialog";
import { useRef, type KeyboardEvent, type ReactNode } from "react";
import { Button } from "./Button";
import { isOpen, openingChoice, stepChoice } from "./choiceDialogModel.mjs";
import { cn } from "./cn";
import { DialogFooter } from "./Dialog";
import { BODY, HEADER, OVERLAY, PANEL } from "./dialogLayout.mjs";
import { ICON } from "./icons";
import { ImmediateIndicators } from "./indicatorBeat";
import { useSurface } from "./openSurfaces";
import { t } from "../i18n/l10n.mjs";

export interface Choice {
  id: string;
  label: string;
  /** One sentence: what taking it does. */
  description: string;
  /** `danger` for the act that destroys something. */
  tone?: "default" | "danger";
  /** Why it cannot be taken here; shown in place of the description. */
  disabled?: string | null;
  onSelect: () => void;
}

export function ChoiceDialog({
  open,
  onClose,
  title,
  body,
  choices,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  /** What is being asked about, above the choices. */
  body?: ReactNode;
  choices: readonly Choice[];
}) {
  useSurface(open);
  const rows = useRef(new Map<string, HTMLButtonElement>());
  const focus = (id: string | null) => {
    if (id !== null) rows.current.get(id)?.focus();
  };
  const onKey = (e: KeyboardEvent<HTMLDivElement>, id: string) => {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    focus(stepChoice(choices, id, e.key === "ArrowDown" ? 1 : -1));
  };
  return (
    <A.Root open={open} onOpenChange={(next) => !next && onClose()}>
      <A.Portal>
        <A.Overlay className={OVERLAY} />
        <A.Content
          data-pane
          className={cn(PANEL, "max-w-md")}
          onOpenAutoFocus={(e) => {
            const first = openingChoice(choices);
            if (first === null) return;
            e.preventDefault();
            focus(first);
          }}
        >
          <header className={HEADER}>
            <A.Title className="text-base font-semibold">{title}</A.Title>
          </header>
          <div className={BODY}>
            <ImmediateIndicators>
              {body ? (
                <A.Description asChild>
                  <div className="mb-3 text-xs text-text-dim">{body}</div>
                </A.Description>
              ) : (
                <A.Description className="sr-only">{t("ui-choice-dialog-choose-what-do-cancel")}</A.Description>
              )}
              <div role="group" aria-label={title} className="flex flex-col gap-1.5">
                {choices.map((choice) => {
                  const held = !isOpen(choice);
                  const danger = choice.tone === "danger";
                  return (
                    <div key={choice.id} onKeyDown={(e) => onKey(e, choice.id)}>
                      <button
                        type="button"
                        ref={(el) => {
                          if (el) rows.current.set(choice.id, el);
                          else rows.current.delete(choice.id);
                        }}
                        disabled={held}
                        onClick={choice.onSelect}
                        className={cn(
                          "anim flex w-full items-center gap-3 rounded-control border border-border px-3 py-2 text-left outline-none focus-visible:ring-2 focus-visible:ring-accent",
                          held ? "cursor-not-allowed opacity-60" : danger ? "hover:border-danger hover:bg-danger-soft" : "hover:bg-surface-2",
                        )}
                      >
                        <span className="min-w-0 flex-1">
                          <span className={cn("block text-sm font-medium", danger && !held && "text-danger")}>{choice.label}</span>
                          <span className="mt-0.5 block text-2xs text-text-dim">{held ? choice.disabled : choice.description}</span>
                        </span>
                        {!held && <ICON.collapsed size={14} className="shrink-0 text-text-dim" aria-hidden />}
                      </button>
                    </div>
                  );
                })}
              </div>
            </ImmediateIndicators>
          </div>
          <DialogFooter>
            <A.Cancel asChild>
              <Button variant="ghost">{t("ui-choice-dialog-cancel")}</Button>
            </A.Cancel>
          </DialogFooter>
        </A.Content>
      </A.Portal>
    </A.Root>
  );
}
