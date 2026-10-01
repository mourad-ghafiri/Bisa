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
import { useEffect, useId, useState } from "react";
import { copyText } from "./clipboard";
import { cn } from "./cn";
import { ICON } from "./icons";
import { parseBounded } from "./numberInputModel.mjs";
import { t as tr } from "../i18n/l10n.mjs";

const CONTROL =
  "anim w-full rounded-control border border-border bg-surface px-2 py-1.5 text-xs text-text placeholder:text-text-dim focus:border-accent focus:outline-none";

export function Field({
  label,
  hint,
  children,
  htmlFor,
}: {
  label: string;
  hint?: ReactNode;
  children: ReactNode;
  htmlFor?: string;
}) {
  return (
    <label className="block" htmlFor={htmlFor}>
      <span className="mb-1 block text-2xs font-medium text-text-dim">{label}</span>
      {children}
      {hint && <span className="mt-1 block text-2xs text-text-dim">{hint}</span>}
    </label>
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
}: {
  value: number;
  onCommit: (next: number) => void;
  min?: number;
  max?: number;
  className?: string;
  disabled?: boolean;
  "aria-label"?: string;
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
  return (
    <div className="flex items-start gap-2">
      <C.Root
        id={id}
        checked={checked}
        disabled={disabled}
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
      <label htmlFor={id} className="text-xs">
        {label}
        {hint && <span className="mt-0.5 block text-2xs text-text-dim">{hint}</span>}
      </label>
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
        "anim inline-flex max-w-full items-center gap-1.5 rounded-control border bg-surface-2 px-2 py-1 text-left font-mono text-2xs hover:border-accent/40",
        said === "refused" ? "border-danger/50 text-danger" : said === "copied" ? "border-ok/50 text-ok" : "border-border",
      )}
      aria-live="polite"
    >
      <span className="truncate">{said === "copied" ? tr("ui-field-copied") : said === "refused" ? tr("ui-mermaid-view-clipboard-refused") : (label ?? value)}</span>
      <ICON.copy size={11} aria-hidden className="shrink-0 text-text-dim" />
    </button>
  );
}
