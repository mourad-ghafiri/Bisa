/**
 * The eighteen step kinds (`STEP_KINDS`) in four headed groups — **Events**
 * (Start · Wait · Emit signal · End), **Gateways** (Decide · If · Switch ·
 * Judge · Parallel), **Loops** (For each · While), **Tasks** (Agent · Human ·
 * Approval · Check · Connector · Notify · Spawn) — ready to drag onto the
 * canvas or click to add.
 *
 * HTML5 drag-and-drop with our own MIME type (`STEP_MIME`), because the
 * target is the flow canvas, which takes native drops of its own; the rest of
 * the app drags through `ui/dnd`. A drop zone that accepted `text/plain`
 * would also accept a URL from another window, and a step kind is not text.
 *
 * Each item is a `div` with the button role, not a `<button>`: WebKit is
 * unreliable about starting a drag from a form control, and the desktop's
 * webview takes no drops of its own (`dragDropEnabled: false`) so
 * the DOM sees this one. Enter and Space add, like a button would.
 */

import { STEP_KIND_ICON, Tooltip, cn } from "../../ui";
import { FAMILIES, FAMILY_LABEL, STEP_KINDS, STEP_MIME, type KindDef, type StepKindName } from "./stepKinds.mjs";
import { familyInk } from "./familyInk";
import { t } from "../../i18n/l10n.mjs";

function PaletteItem({ k, onAdd, disabled, compact }: { k: KindDef; onAdd: (kind: StepKindName) => void; disabled?: boolean; compact?: boolean }) {
  const Icon = STEP_KIND_ICON[k.kind];
  return (
    <li>
      {/* Folded to its glyph, the item's name moves into its tooltip and its accessible name. */}
      <Tooltip label={compact ? t("workflow-palette-named-drag-onto-canvas", { label: k.label, explain: k.explain }) : t("workflow-palette-drag-onto-canvas-press-enter-add", { explain: k.explain })} side="right">
        <div
          role="button"
          aria-label={compact ? k.label : undefined}
          tabIndex={disabled ? -1 : 0}
          aria-disabled={disabled || undefined}
          draggable={!disabled}
          style={{ WebkitUserDrag: disabled ? "none" : "element" } as React.CSSProperties}
          onDragStart={(e) => {
            if (disabled) {
              e.preventDefault();
              return;
            }
            e.dataTransfer.setData(STEP_MIME, k.kind);
            e.dataTransfer.effectAllowed = "copy";
          }}
          onClick={() => !disabled && onAdd(k.kind)}
          onKeyDown={(e) => {
            if (disabled) return;
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              onAdd(k.kind);
            }
          }}
          className={cn(
            "anim flex h-row w-full select-none items-center gap-2 rounded-control text-left text-xs outline-none focus-visible:ring-1 focus-visible:ring-accent",
            compact ? "justify-center" : "px-2",
            disabled ? "cursor-not-allowed opacity-50" : "cursor-grab hover:bg-surface-2 active:cursor-grabbing",
          )}
        >
          <Icon size={14} aria-hidden className={`shrink-0 ${familyInk(k.kind)}`} />
          {!compact && k.label}
        </div>
      </Tooltip>
    </li>
  );
}

export function Palette({
  onAdd,
  disabled,
  compact = false,
  className,
}: {
  onAdd: (kind: StepKindName) => void;
  disabled?: boolean;
  /** A narrow window's palette: glyphs only, each group parted by a hairline, its heading kept for a screen reader. */
  compact?: boolean;
  className?: string;
}) {
  return (
    <div aria-label={t("workflow-palette-step-kinds")} className={cn("flex flex-col", compact ? "gap-2" : "gap-5", className)}>
      {FAMILIES.map((family, i) => (
        <section key={family} aria-labelledby={`palette-${family}`} className={cn(compact && i > 0 && "border-t border-hairline pt-2")}>
          <h4 id={`palette-${family}`} className={compact ? "sr-only" : "px-2 pb-1 text-2xs font-semibold text-text-dim"}>
            {FAMILY_LABEL[family]}
          </h4>
          <ul className="flex flex-col gap-0.5">
            {STEP_KINDS.filter((k) => k.family === family).map((k) => (
              <PaletteItem key={k.kind} k={k} onAdd={onAdd} disabled={disabled} compact={compact} />
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
