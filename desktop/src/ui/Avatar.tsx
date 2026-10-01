/**
 * Deterministic identicons: the same pubkey is the same colour and glyph in
 * every surface, so a principal is recognisable before you read the hex.
 *
 * Radix's Avatar is here for one thing — it renders the fallback when an
 * image is slow or broken, and only then. The old version branched on whether
 * a `url` was present, so an avatar whose URL 404'd left a broken-image icon
 * where a recognisable colour used to be; now the identicon comes back.
 */

import * as A from "@radix-ui/react-avatar";
import { cn } from "./cn";
import { usePhotoThumb } from "./photoThumbs";

/**
 * Identicon hues, deliberately skipping 270–330: the app has no purple in it
 * and a stray violet avatar would be the only one on screen.
 */
const HUES = [12, 40, 68, 95, 130, 155, 185, 205, 235, 258];

function hash(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return Math.abs(h);
}

export function principalColor(id: string): string {
  const hue = HUES[hash(id) % HUES.length]!;
  return `oklch(0.62 0.13 ${hue})`;
}

/** Two characters that stay stable for an id — never a slice of a name. */
function initials(id: string, name?: string): string {
  if (name?.trim()) {
    const parts = name.trim().split(/\s+/);
    const a = parts[0]?.[0] ?? "";
    const b = parts.length > 1 ? (parts[parts.length - 1]?.[0] ?? "") : "";
    return (a + b).toUpperCase() || id.slice(0, 2).toUpperCase();
  }
  return id.slice(0, 2).toUpperCase();
}

export function Avatar({
  id,
  name,
  url,
  photo,
  size = 20,
  title,
  className,
}: {
  id: string;
  name?: string;
  /** A picture by URL — a thumbnail already made, or a page's own. */
  url?: string | null;
  /** A photo by content hash (ide/14 §Photos): its shared thumbnail is drawn, `url` first when both are given. */
  photo?: { sha256: string } | null;
  size?: number;
  title?: string;
  className?: string;
}) {
  const label = title ?? name ?? id;
  const thumb = usePhotoThumb(photo?.sha256 ?? null);
  const src = url ?? thumb;
  return (
    <A.Root
      title={label}
      className={cn("inline-flex shrink-0 overflow-hidden rounded-full align-middle", className)}
      style={{ width: size, height: size }}
    >
      {src && <A.Image src={src} alt="" width={size} height={size} decoding="async" loading="lazy" className="h-full w-full object-cover" />}
      <A.Fallback
        // The identicon carries no information the row does not already
        // state, so it is decoration to a screen reader, not a second name.
        aria-hidden
        // Nothing to wait for when there is no image: showing the fallback
        // immediately avoids a frame of empty channel on every list render.
        delayMs={src ? 200 : 0}
        className="flex h-full w-full items-center justify-center font-semibold text-white"
        style={{ background: principalColor(id), fontSize: Math.max(8, Math.round(size * 0.42)) }}
      >
        {initials(id, name)}
      </A.Fallback>
    </A.Root>
  );
}

/** A pubkey rendered short, with its identicon — the standard author line. */
export function PrincipalTag({ id, name }: { id: string; name?: string }) {
  return (
    <span className="inline-flex items-center gap-1.5">
      <Avatar id={id} name={name} size={16} />
      <span className="tnum text-2xs text-text-dim">{name ?? `${id.slice(0, 8)}…`}</span>
    </span>
  );
}
