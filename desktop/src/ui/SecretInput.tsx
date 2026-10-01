/**
 * The one field a secret is typed into (ide/13 §Secret fields): hidden by
 * default, an eye to show and hide what was typed, the value kept in the box
 * for as long as the window lives — and, once nothing is typed and the node
 * says one is stored, the mask, dim, with the eye out of work: the node never
 * reads a secret back (11 — Security, I49), so there is nothing to show; a
 * focus selects the mask and typing replaces it. The reveal is per mount and
 * never remembered; nothing here touches storage. The rules are
 * `secretInputModel.mjs`'s. A PEM block has `SecretTextArea`: folded to one
 * masked line while hidden, the text area when shown.
 */

import { useState, type InputHTMLAttributes } from "react";
import { Button } from "./Button";
import { TextArea, TextInput } from "./Field";
import { ICON } from "./icons";
import { Tooltip } from "./Tooltip";
import { cn } from "./cn";
import { foldedWords, replaceOnFocus, secretView } from "./secretInputModel.mjs";
import { t } from "../i18n/l10n.mjs";

type Shared = {
  value: string;
  onChange: (next: string) => void;
  /** The node keeps one already: the box shows the mask until something is typed. */
  stored?: boolean | null;
  /** The word the eye uses — key, token, value, secret. */
  what?: string;
  disabled?: boolean;
  className?: string;
};

function Eye({ view, onToggle, disabled }: { view: ReturnType<typeof secretView>; onToggle: () => void; disabled?: boolean }) {
  const Glyph = view.eye.pressed ? ICON.hidden : ICON.inspect;
  return (
    <Tooltip label={view.eye.title}>
      <span className="absolute inset-y-0 right-1 flex items-center">
        <Button size="icon" variant="ghost" aria-label={view.eye.label} aria-pressed={view.eye.pressed} disabled={disabled || !view.canReveal} onClick={onToggle}>
          <Glyph size={12} aria-hidden />
        </Button>
      </span>
    </Tooltip>
  );
}

export function SecretInput({
  value,
  onChange,
  stored = false,
  what = t("ui-secret-input-what-secret"),
  disabled,
  className,
  ...rest
}: Shared & Omit<InputHTMLAttributes<HTMLInputElement>, "value" | "onChange" | "type" | "disabled" | "className">) {
  const [revealed, setRevealed] = useState(false);
  const view = secretView({ draft: value, stored, revealed, what });
  return (
    <span className={cn("relative block", className)}>
      <TextInput
        {...rest}
        type={view.inputType}
        autoComplete="off"
        spellCheck={false}
        disabled={disabled}
        value={view.value}
        placeholder={rest.placeholder ?? view.placeholder}
        className={cn("pr-8 font-mono", view.dim && "text-text-dim")}
        onFocus={(e) => {
          if (replaceOnFocus(value, stored)) e.currentTarget.select();
          rest.onFocus?.(e);
        }}
        onChange={(e) => onChange(e.target.value)}
      />
      <Eye view={view} disabled={disabled} onToggle={() => setRevealed((r) => !r)} />
    </span>
  );
}

/** A multiline secret — a PEM block: one masked line while hidden, the text area when shown. */
export function SecretTextArea({ value, onChange, stored = false, what = t("ui-secret-input-what-key"), disabled, className, rows = 6 }: Shared & { rows?: number }) {
  const [revealed, setRevealed] = useState(false);
  const view = secretView({ draft: value, stored, revealed, what });
  const folded = view.shows === "draft" && !revealed;
  return (
    <span className={cn("relative block", className)}>
      {folded || view.shows !== "draft" ? (
        <button
          type="button"
          disabled={disabled}
          aria-label={view.shows === "draft" ? t("ui-secret-input-hidden-press-type", { what }) : view.placeholder || t("ui-secret-input-what-stored", { what })}
          className={cn("anim w-full rounded-control border border-border bg-surface px-2 py-1.5 pr-8 text-left font-mono text-2xs", view.shows === "empty" ? "text-text-dim" : "text-text-dim")}
          onClick={() => setRevealed(true)}
        >
          {view.shows === "draft" ? foldedWords(value) : view.shows === "stored" ? view.value : view.placeholder}
        </button>
      ) : (
        <TextArea rows={rows} autoComplete="off" spellCheck={false} disabled={disabled} className="pr-8 font-mono text-2xs" value={value} onChange={(e) => onChange(e.target.value)} />
      )}
      <Eye view={{ ...view, canReveal: view.shows !== "stored" }} disabled={disabled} onToggle={() => setRevealed((r) => !r)} />
    </span>
  );
}
