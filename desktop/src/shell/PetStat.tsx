/**
 * The footer's Pet read-out: the pet glyph, tinted while a pet shows, the
 * pet's name as the tooltip — and, on a click, the Pet overlay
 * (`PetOverlay`), the way the memory and CPU read-outs open theirs
 * (`ResourceStat`). Controlled, so a tile that opened Settings can close
 * the panel behind it. It used to be a bare toggle; a toggle could show or
 * hide, and nothing else about the pet was within reach from where the pet
 * is.
 */

import { useState } from "react";
import { ICON, Popover, Tooltip, cn } from "../ui";
import { usePet } from "../pet/petStore";
import { PetOverlay } from "./PetOverlay";
import { titleWords } from "./petOverlayModel.mjs";

export function PetStat() {
  const [open, setOpen] = useState(false);
  const { active, pets } = usePet();
  const title = titleWords(active, pets);
  return (
    <Popover
      label={title}
      side="top"
      align="end"
      open={open}
      onOpenChange={setOpen}
      className="w-[calc(var(--type-rem)*40)] max-w-[90vw]"
      trigger={
        <Tooltip label={title}>
          <span
            aria-label={title}
            aria-pressed={!!active}
            role="button"
            className={cn(
              "anim flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-control",
              active ? "bg-accent-soft text-accent-ink" : "text-text-dim hover:bg-surface-2",
            )}
          >
            <ICON.pet size={13} aria-hidden />
          </span>
        </Tooltip>
      }
    >
      {open && <PetOverlay close={() => setOpen(false)} />}
    </Popover>
  );
}
