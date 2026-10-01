/**
 * A modal that behaves like one: focus trapped inside, Escape closes, the
 * page behind doesn't scroll, and focus returns where it came from.
 *
 * All of that is now Radix's rather than ours. The hand-rolled version got it
 * right in the end, but it was 90 lines of focus-trap and scroll-lock that had
 * to be right about `inert`, about pointer events outside the panel, and about
 * what happens when the panel's first focusable control unmounts while it is
 * focused. Radix is right about those, is tested against screen readers, and
 * keeps `aria-modal`, the labelling and the focus guards in sync without us
 * maintaining them.
 *
 * The props are unchanged, deliberately: twenty call sites pass `open` and
 * `onClose` rather than Radix's `onOpenChange`, and a controlled-dialog
 * convention is not worth churning them over.
 *
 * A modal that opens onto a read shows the read, never a blank panel behind
 * the veil: its body provides the immediate indicator beat
 * (`ImmediateIndicators`), so the kit's *reading …* line and rows draw the
 * moment the panel does.
 *
 * **No button is ever out of sight.** The panel is a column whose body is the
 * one scrollport, and a footer is `DialogFooter` — right-aligned, and wrapping
 * a row that cannot fit (`dialogLayout.mjs`, where the two rules are tested).
 * A confirmation that offers several acts is not a longer row of buttons: it
 * is a `ChoiceDialog`.
 */

import * as D from "@radix-ui/react-dialog";
import { useSurface } from "./openSurfaces";
import * as A from "@radix-ui/react-alert-dialog";
import { useEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import { Button } from "./Button";
import { cn } from "./cn";
import { Field, TextInput } from "./Field";
import { ICON } from "./icons";
import { BODY, FOOTER, HEADER, OVERLAY, PANEL, SOLID_DANGER } from "./dialogLayout.mjs";
import { ImmediateIndicators } from "./indicatorBeat";
import { t } from "../i18n/l10n.mjs";

/** A dialog's actions: on the right, and on a second line when one cannot hold them. */
export function DialogFooter({ children }: { children: ReactNode }) {
  return <div className={FOOTER}>{children}</div>;
}

export function Dialog({
  open,
  onClose,
  title,
  description,
  children,
  footer,
  width = "max-w-lg",
  initialFocus,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  description?: string;
  children: ReactNode;
  footer?: ReactNode;
  /** A Tailwind max-width class; the panel is fluid below it. */
  width?: string;
  /**
   * The control that takes focus when the dialog opens — the field a person
   * came to type in — instead of Radix's first tabbable, the header's Close.
   * A ref that is empty at open (the field mounts after a read) is left to
   * the caller, who focuses it once it exists.
   */
  initialFocus?: RefObject<HTMLElement | null>;
}) {
  // A native layer (the browser tab) yields while this is open.
  useSurface(open);
  return (
    <D.Root open={open} onOpenChange={(next) => !next && onClose()}>
      <D.Portal>
        <D.Overlay className={OVERLAY} />
        <D.Content
          data-pane
          className={cn(PANEL, width)}
          onOpenAutoFocus={(e) => {
            const el = initialFocus?.current;
            if (!el) return;
            e.preventDefault();
            el.focus();
          }}
        >
          <header className={cn(HEADER, "flex items-start gap-3")}>
            <div className="min-w-0 flex-1">
              <D.Title className="text-base font-semibold">{title}</D.Title>
              {description && (
                <D.Description className="mt-0.5 text-2xs text-text-dim">
                  {description}
                </D.Description>
              )}
            </div>
            <D.Close
              aria-label={t("ui-dialog-close")}
              className="anim -mr-1 rounded-control p-1 text-text-dim hover:bg-surface-2 hover:text-text"
            >
              <ICON.close size={14} />
            </D.Close>
          </header>
          <div className={BODY}>
            <ImmediateIndicators>{children}</ImmediateIndicators>
          </div>
          {footer && <DialogFooter>{footer}</DialogFooter>}
        </D.Content>
      </D.Portal>
    </D.Root>
  );
}

/**
 * Confirm-then-act, for anything irreversible.
 *
 * This is an AlertDialog rather than a Dialog, which is not a styling choice:
 * `role="alertdialog"` makes a screen reader announce the body before the
 * buttons, and Radix refuses to close it on an outside click. A destructive
 * confirmation that a stray click can dismiss is a confirmation that teaches
 * people to click through it.
 */
export function ConfirmDialog({
  open,
  onClose,
  onConfirm,
  title,
  body,
  confirmLabel = t("ui-dialog-confirm"),
  danger = false,
}: {
  open: boolean;
  onClose: () => void;
  onConfirm: () => void;
  title: string;
  body: ReactNode;
  confirmLabel?: string;
  danger?: boolean;
}) {
  useSurface(open);
  return (
    <A.Root open={open} onOpenChange={(next) => !next && onClose()}>
      <A.Portal>
        <A.Overlay className={OVERLAY} />
        <A.Content data-pane className={cn(PANEL, "max-w-md")}>
          <header className={HEADER}>
            <A.Title className="text-base font-semibold">{title}</A.Title>
          </header>
          <A.Description asChild>
            <div className={cn(BODY, "text-xs text-text-dim")}>
              <ImmediateIndicators>{body}</ImmediateIndicators>
            </div>
          </A.Description>
          <DialogFooter>
            {/* The kit's own buttons, so a confirmation's actions are the size of every other dialog's. */}
            <A.Cancel asChild>
              <Button variant="ghost">{t("ui-choice-dialog-cancel")}</Button>
            </A.Cancel>
            <A.Action asChild onClick={onConfirm}>
              <Button variant="primary" className={danger ? SOLID_DANGER : undefined}>
                {confirmLabel}
              </Button>
            </A.Action>
          </DialogFooter>
        </A.Content>
      </A.Portal>
    </A.Root>
  );
}

/**
 * One value, asked for: a name, a branch, a ref. Enter submits, the field is
 * focused with `selection` selected (a rename selects the stem), `validate`
 * refuses before the round trip. The kit's one prompt, so the explorer, the
 * branches panel and the commit graph do not each hand-roll a dialog.
 */
export function PromptDialog({
  open,
  onClose,
  onSubmit,
  title,
  description,
  label,
  hint,
  initial = "",
  placeholder,
  mono = false,
  selection,
  submitLabel = t("ui-dialog-ok"),
  busy = false,
  validate,
  extra,
}: {
  open: boolean;
  onClose: () => void;
  onSubmit: (value: string) => void;
  title: string;
  description?: string;
  label: string;
  hint?: string;
  initial?: string;
  placeholder?: string;
  mono?: boolean;
  /** `[start, end]` selected on open; the whole value otherwise. */
  selection?: [number, number];
  submitLabel?: string;
  busy?: boolean;
  /** A sentence refusing the value, or null when it may be submitted. */
  validate?: (value: string) => string | null;
  /** One control under the field — a checkbox that changes what the name does. */
  extra?: ReactNode;
}) {
  const [value, setValue] = useState(initial);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (!open) return;
    setValue(initial);
    // The dialog focuses the field on open (`initialFocus`); the selection is ours.
    const el = input.current;
    if (!el) return;
    const [a, b] = selection ?? [0, initial.length];
    el.setSelectionRange(a, b);
    // The opening and the initial value are the resets; `selection` is a
    // tuple a caller may write fresh each render, and following it would
    // put the initial value back over what the person typed.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, initial]);
  const problem = value.trim() ? (validate?.(value.trim()) ?? null) : t("ui-dialog-name-needed");
  const submit = () => {
    if (busy || problem) return;
    onSubmit(value.trim());
  };
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={title}
      description={description}
      width="max-w-md"
      initialFocus={input}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>{t("ui-choice-dialog-cancel")}</Button>
          <Button variant="primary" disabled={busy || !!problem} onClick={submit}>
            {busy ? "…" : submitLabel}
          </Button>
        </>
      }
    >
      <Field label={label} hint={value.trim() && problem ? problem : hint}>
        <TextInput
          ref={input}
          value={value}
          placeholder={placeholder}
          className={mono ? "font-mono" : undefined}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              submit();
            }
          }}
        />
      </Field>
      {extra && <div className="mt-3">{extra}</div>}
    </Dialog>
  );
}
