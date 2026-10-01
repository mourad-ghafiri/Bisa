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
import { t } from "../../i18n/l10n.mjs";

function PaletteItem({ k, onAdd, disabled }: { k: KindDef; onAdd: (kind: StepKindName) => void; disabled?: boolean }) {
  const Icon = STEP_KIND_ICON[k.kind];
  return (
    <li>
      <Tooltip label={t("workflow-palette-drag-onto-canvas-press-enter-add", { explain: k.explain })} side="right">
        <div
          role="button"
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
            "anim flex h-row w-full select-none items-center gap-2 rounded-control px-2 text-left text-xs outline-none focus-visible:ring-1 focus-visible:ring-accent",
            disabled ? "cursor-not-allowed opacity-50" : "cursor-grab hover:bg-surface-2 active:cursor-grabbing",
          )}
        >
          <Icon size={14} aria-hidden className="shrink-0 text-text-dim" />
          {k.label}
        </div>
      </Tooltip>
    </li>
  );
}

export function Palette({
  onAdd,
  disabled,
  className,
}: {
  onAdd: (kind: StepKindName) => void;
  disabled?: boolean;
  className?: string;
}) {
  return (
    <div aria-label={t("workflow-palette-step-kinds")} className={cn("flex flex-col gap-2", className)}>
      {FAMILIES.map((family) => (
        <section key={family} aria-labelledby={`palette-${family}`}>
          <h4 id={`palette-${family}`} className="px-2 pb-0.5 text-3xs font-semibold tracking-wide text-text-dim uppercase">
            {FAMILY_LABEL[family]}
          </h4>
          <ul className="flex flex-col gap-0.5">
            {STEP_KINDS.filter((k) => k.family === family).map((k) => (
              <PaletteItem key={k.kind} k={k} onAdd={onAdd} disabled={disabled} />
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
