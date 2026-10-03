/**
 * Form controls.
 *
 * The text controls stay native. A styled `<input>` is already accessible,
 * already handles IME composition, autofill, spellcheck and the platform's
 * own text menus, and nothing in this app needs behaviour a native input does
 * not have. The checkbox is the exception: its box is drawn rather than
 * native, so it comes from Radix, which keeps the real input's semantics
 * underneath the drawing.
 */

import * as C from "@radix-ui/react-checkbox";
import type {
  InputHTMLAttributes,
  ReactNode,
  SelectHTMLAttributes,
  TextareaHTMLAttributes,
} from "react";
import { cloneElement, isValidElement, useEffect, useId, useState } from "react";
import { copyText } from "./clipboard";
import { cn } from "./cn";
import { ICON } from "./icons";
import { parseBounded } from "./numberInputModel.mjs";
import { t as tr } from "../i18n/l10n.mjs";

const CONTROL =
  "anim w-full rounded-control border border-border bg-surface px-2.5 py-1.5 text-xs text-text placeholder:text-text-dim hover:border-text-dim/40 focus:border-accent focus:outline-none disabled:cursor-not-allowed disabled:opacity-60";

/**
 * A labelled control. The hint and the error sit *beside* the `<label>`, not
 * inside it: words inside a label become the control's name, and a screen
 * reader would read a whole hint as what the field is called. They reach the
 * control as its description instead — `aria-describedby` on the one element
 * a Field wraps — and an error marks it `aria-invalid`.
 *
 * `error` is for a value the field refused or put back: a reason in the
 * danger ink, said where the value was typed, never only in a tooltip.
 *
 * `action` is a control that works on the field itself — a *Suggest* that
 * drafts its value — drawn at the end of the label's line and kept outside
 * the `<label>`, so it is never part of the field's name and never a button
 * inside a label.
 */
export function Field({
  label,
  hint,
  error,
  action,
  children,
  htmlFor,
}: {
  label: string;
  hint?: ReactNode;
  /** Why the value was refused or reverted — shown in danger ink and announced. */
  error?: ReactNode;
  /** A small control on the label's line — keep it `h-6` so the line does not grow. */
  action?: ReactNode;
  children: ReactNode;
  htmlFor?: string;
}) {
  const id = useId();
  const hintId = hint ? `${id}-hint` : undefined;
  const errorId = error ? `${id}-error` : undefined;
  const described = [errorId, hintId].filter(Boolean).join(" ");
  const control =
    described && isValidElement<{ "aria-describedby"?: string; "aria-invalid"?: boolean }>(children)
      ? cloneElement(children, {
          "aria-describedby": [children.props["aria-describedby"], described].filter(Boolean).join(" "),
          ...(error ? { "aria-invalid": true } : {}),
        })
      : children;
  return (
    <div className={action ? "relative block" : "block"}>
      <label className="block" htmlFor={htmlFor}>
        <span className={cn("mb-1 block text-2xs font-medium text-text-dim", action && "pr-28")}>{label}</span>
        {control}
      </label>
      {action && <div className="absolute -top-1 right-0 flex items-center">{action}</div>}
      {error && (
        <span id={errorId} role="alert" className="mt-1 block text-2xs text-danger">
          {error}
        </span>
      )}
      {hint && (
        <span id={hintId} className="mt-1 block text-2xs text-text-dim">
          {hint}
        </span>
      )}
    </div>
  );
}

/**
 * A calendar day, `YYYY-MM-DD` or empty — the platform's own native date
 * control, one wrapper so no screen reaches for `type="date"` itself. A day
 * carries no time zone: what is typed is what is stored.
 */
export function DateInput({
  value,
  onChange,
  className,
  ...rest
}: Omit<InputHTMLAttributes<HTMLInputElement>, "value" | "onChange" | "type"> & {
  value: string;
  onChange: (value: string) => void;
}) {
  return <input type="date" value={value} onChange={(e) => onChange(e.target.value)} className={cn(CONTROL, "tnum", className)} {...rest} />;
}

/** `ref` is an ordinary prop on a function component in React 19; it reaches the `<input>`. */
export function TextInput({ className, ...rest }: InputHTMLAttributes<HTMLInputElement> & { ref?: React.Ref<HTMLInputElement> }) {
  // The webview must never alter typed text: WKWebView's default is
  // `autocapitalize="sentences"`, which forces a capital first letter. Both
  // defaults sit before `{...rest}` so a caller can still opt back in.
  return <input autoCapitalize="off" autoCorrect="off" className={cn(CONTROL, className)} {...rest} />;
}

/**
 * An integer, typed as text and committed on blur or Enter — never per
 * keystroke, which would turn "1" into 1 before "12" is typed and clamp ""
 * to the minimum while a person is still deleting. The rule that turns the
 * draft into a value is `numberInputModel.mjs`.
 */
export function NumberInput({
  value,
  onCommit,
  min,
  max,
  className,
  disabled,
  "aria-label": ariaLabel,
  "aria-describedby": describedBy,
  "aria-invalid": invalid,
}: {
  value: number;
  onCommit: (next: number) => void;
  min?: number;
  max?: number;
  className?: string;
  disabled?: boolean;
  "aria-label"?: string;
  /** A `Field`'s hint and error reach the input through these. */
  "aria-describedby"?: string;
  "aria-invalid"?: boolean;
}) {
  const [draft, setDraft] = useState(String(value));
  useEffect(() => setDraft(String(value)), [value]);
  const commit = () => {
    const next = parseBounded(draft, { min, max, fallback: value });
    setDraft(String(next));
    if (next !== value) onCommit(next);
  };
  return (
    <input
      inputMode="numeric"
      className={cn(CONTROL, className)}
      value={draft}
      disabled={disabled}
      aria-label={ariaLabel}
      aria-describedby={describedBy}
      aria-invalid={invalid}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          commit();
        }
      }}
    />
  );
}

export function TextArea({
  className,
  rows = 4,
  ...rest
}: TextareaHTMLAttributes<HTMLTextAreaElement>) {
  // See TextInput: never force a capital or auto-correct typed text.
  return <textarea autoCapitalize="off" autoCorrect="off" rows={rows} className={cn(CONTROL, "resize-y", className)} {...rest} />;
}

export function Select({
  className,
  children,
  ...rest
}: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <select className={cn(CONTROL, className)} {...rest}>
      {children}
    </select>
  );
}

/**
 * A value you are entering, not a switch you are flipping — see
 * {@link Switch} for the other one.
 *
 * The label is a sibling rather than a wrapper so a click on the hint text
 * does not toggle the box: a hint often explains a consequence, and reading
 * it should not be the same gesture as accepting it.
 */
export function Checkbox({
  label,
  checked,
  onChange,
  hint,
  disabled,
}: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
  hint?: string;
  disabled?: boolean;
}) {
  const id = useId();
  const hintId = hint ? `${id}-hint` : undefined;
  return (
    <div className="flex items-start gap-2">
      <C.Root
        id={id}
        checked={checked}
        disabled={disabled}
        aria-describedby={hintId}
        // Radix reports "indeterminate" for a tri-state box. This one is
        // binary, so anything that is not true is false rather than a third
        // value leaking out into the caller's boolean.
        onCheckedChange={(next) => onChange(next === true)}
        className={cn(
          "anim mt-0.5 flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded border border-border bg-surface outline-none",
          "data-[state=checked]:border-accent data-[state=checked]:bg-accent",
          "disabled:pointer-events-none disabled:opacity-45",
        )}
      >
        <C.Indicator className="flex text-accent-contrast">
          <ICON.check size={11} strokeWidth={3} />
        </C.Indicator>
      </C.Root>
      <div className="text-xs">
        <label htmlFor={id}>{label}</label>
        {hint && (
          <span id={hintId} className="mt-0.5 block text-2xs text-text-dim">
            {hint}
          </span>
        )}
      </div>
    </div>
  );
}

/** What a copy said, for the two seconds the button shows it. */
const COPY_FEEDBACK_MS = 2000;

/**
 * Copy-to-clipboard for keys and ids, which are always too long to retype.
 * A copy answers whether it happened (`ui/clipboard.ts`), and the button
 * says so — *Copied.* or *The clipboard refused.* — for a moment, then goes
 * back to its label; a copy nobody can see happen is a copy nobody trusts.
 */
export function CopyText({ value, label, onCopied }: { value: string; label?: string; onCopied?: (ok: boolean) => void }) {
  const [said, setSaid] = useState<"copied" | "refused" | null>(null);
  useEffect(() => {
    if (!said) return;
    const t = setTimeout(() => setSaid(null), COPY_FEEDBACK_MS);
    return () => clearTimeout(t);
  }, [said]);
  return (
    <button
      type="button"
      title={tr("ui-field-copy")}
      onClick={() => {
        void copyText(value).then((ok) => {
          setSaid(ok ? "copied" : "refused");
          onCopied?.(ok);
        });
      }}
      className={cn(
        "anim inline-flex max-w-full items-center gap-1.5 rounded-control border bg-surface-2 px-2 py-1 text-left font-mono text-2xs hover:border-text-dim/40",
        said === "refused" ? "border-danger/50 text-danger" : said === "copied" ? "border-ok/50 text-ok" : "border-border",
      )}
      aria-live="polite"
    >
      <span className="truncate">{said === "copied" ? tr("ui-field-copied") : said === "refused" ? tr("ui-mermaid-view-clipboard-refused") : (label ?? value)}</span>
      <ICON.copy size={11} aria-hidden className="shrink-0 text-text-dim" />
    </button>
  );
}
