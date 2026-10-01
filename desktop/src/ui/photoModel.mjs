/**
 * A photo, as facts (ide/14 §Photos): what is kept — a square scaled once
 * from the picked picture before it is uploaded, so a camera's file never
 * leaves the machine — in one of two **profiles**: a **picture** (a
 * project's, a group's, an agent's, a team's: 256 px, PNG, kept on this
 * machine) or a **face** (a person's: 96 px, JPEG, small enough to travel
 * with their profile to every workspace they are a member of,
 * 14-collaboration); what is drawn — a `THUMB_EDGE` square made once per
 * machine from whatever the stored bytes are — the centred cover-crop that
 * turns any picture into a square, the names, the refusals, and the order
 * the thumbnail cache lets go of what it holds. Pure, so `node --test`
 * reads it; `photoScale.ts` draws, `photoThumbs.ts` keeps, `PhotoField.tsx`
 * picks.
 */

import { t } from "../i18n/l10n.mjs";

/**
 * The two profiles: the square kept, what it is encoded as, the most bytes
 * it may hold (the node's `MAX_PHOTO_BYTES` and `MAX_FACE_BYTES`), and the
 * JPEG qualities tried in turn when a face must fit.
 */
export const PHOTO_PROFILES = Object.freeze({
  picture: Object.freeze({ kind: "picture", edge: 256, mime: "image/png", maxBytes: 512 * 1024, qualities: Object.freeze([]) }),
  face: Object.freeze({ kind: "face", edge: 96, mime: "image/jpeg", maxBytes: 16 * 1024, qualities: Object.freeze([0.85, 0.7, 0.5]) }),
});

/** The square a picture is kept at: crisp at 32 px on a 3× display, a few dozen kilobytes as PNG. */
export const PHOTO_EDGE = PHOTO_PROFILES.picture.edge;
/** The square every draw site shares: the largest a photo is drawn at is 32 px, on a 2× display 64. */
export const THUMB_EDGE = 64;
/** What a scaled picture is encoded as — lossless, alpha kept, every engine decodes it. */
export const PHOTO_MIME = PHOTO_PROFILES.picture.mime;
/** How many thumbnails a run keeps before the least recently drawn are let go. */
export const THUMB_CACHE_MAX = 200;

/** The picked file types the pickers accept. */
export const PHOTO_TYPES = Object.freeze(["image/png", "image/jpeg", "image/gif", "image/webp"]);

/** Whether a picked file's type is one the pickers accept. */
export function isPhotoType(mime) {
  return PHOTO_TYPES.includes(String(mime ?? "").toLowerCase());
}

/**
 * The centred square of a `width` × `height` source, and the size it is
 * drawn at: `edge`, or the source's own square when that is smaller — a
 * picture is never scaled up. All zero for an empty source.
 * @param {number} width
 * @param {number} height
 * @param {number} edge
 * @returns {{sx: number, sy: number, sw: number, sh: number, dw: number, dh: number}}
 */
export function coverCrop(width, height, edge) {
  const w = Math.max(0, Math.floor(width));
  const h = Math.max(0, Math.floor(height));
  if (w === 0 || h === 0 || !(edge > 0)) return { sx: 0, sy: 0, sw: 0, sh: 0, dw: 0, dh: 0 };
  const side = Math.min(w, h);
  const out = Math.min(Math.floor(edge), side);
  return { sx: Math.floor((w - side) / 2), sy: Math.floor((h - side) / 2), sw: side, sh: side, dw: out, dh: out };
}

/** The file extension a profile's encoding is written with. */
function extensionOf(mime) {
  return mime === "image/jpeg" ? "jpg" : "png";
}

/**
 * The name a scaled photo is kept under: the picked file's stem, marked as
 * the photo it became, in the profile's encoding — `logo.jpg` →
 * `logo-photo.png` for a picture, `me.png` → `me-face.jpg` for a face.
 * @param {string} original
 * @param {{kind: string, mime: string}} [profile]
 */
export function photoName(original, profile = PHOTO_PROFILES.picture) {
  const base = String(original ?? "").split("/").pop() ?? "";
  const dot = base.lastIndexOf(".");
  const stem = (dot > 0 ? base.slice(0, dot) : base).trim() || (profile.kind === "face" ? "face" : "photo");
  const mark = profile.kind === "face" ? "face" : "photo";
  return `${stem}-${mark}.${extensionOf(profile.mime)}`;
}

/**
 * The sentence a picture that cannot become a photo gets.
 * @param {"type" | "decode" | "encode" | "size"} reason
 * @param {{kind: string, maxBytes: number}} [profile]
 */
export function photoRefusal(reason, profile = PHOTO_PROFILES.picture) {
  switch (reason) {
    case "type":
      return t("ui-photo-picture-png-jpeg-gif-webp");
    case "encode":
      return t("ui-photo-picture-could-scaled-here-try-another");
    case "size":
      return t("ui-photo-still-over-after-scaling", { kind: profile.kind, kb: Math.round(profile.maxBytes / 1024) });
    default:
      return t("ui-photo-picture-could-read-may-damaged-what");
  }
}

/**
 * Whether an encoding fits its profile's cap.
 * @param {number} bytes @param {{maxBytes: number}} profile
 */
export function fitsProfile(bytes, profile) {
  return bytes <= profile.maxBytes;
}

/** The cache's key for a photo's thumbnail at an edge. */
export function thumbKey(sha, edge) {
  return `${sha}@${edge}`;
}

/**
 * The keys a cache over `keep` entries lets go of: the least recently
 * drawn first, as many as it is over by. Nothing when it fits.
 * @param {readonly {key: string, at: number}[]} entries
 * @param {number} keep
 * @returns {string[]}
 */
export function evictOrder(entries, keep) {
  const all = [...(entries ?? [])];
  const over = all.length - Math.max(0, Math.floor(keep));
  if (over <= 0) return [];
  all.sort((a, b) => a.at - b.at || (a.key < b.key ? -1 : a.key > b.key ? 1 : 0));
  return all.slice(0, over).map((e) => e.key);
}

/**
 * The face a principal wears where it is drawn: their own row's, an agent's,
 * or none — one lookup for every avatar of a pubkey (the owner's row is a
 * member too).
 * @param {readonly {pubkey: string, photo?: {sha256: string} | null}[]} members
 * @param {readonly {pubkey: string, photo?: {sha256: string} | null}[]} agents
 * @param {string} pubkey
 * @returns {{sha256: string} | null}
 */
export function photoOfPrincipal(members, agents, pubkey) {
  const hit = (members ?? []).find((m) => m.pubkey === pubkey) ?? (agents ?? []).find((a) => a.pubkey === pubkey);
  return hit?.photo ?? null;
}
