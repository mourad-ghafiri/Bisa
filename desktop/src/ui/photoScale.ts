/**
 * The drawing behind a photo (ide/14 §Photos): a picked picture decoded
 * once and drawn as a `PHOTO_EDGE` square before it is uploaded, so the
 * camera's file never leaves the machine; and any stored photo's bytes
 * drawn as a `THUMB_EDGE` square for every draw site to share. The facts —
 * the edges, the cover-crop, the names, the refusals — are
 * `photoModel.mjs`'s; this is the canvas.
 */

import { PHOTO_MIME, PHOTO_PROFILES, THUMB_EDGE, coverCrop, fitsProfile, isPhotoType, photoName, photoRefusal } from "./photoModel.mjs";
import type { PhotoProfile } from "./photoModel.mjs";

/** Decode, or say why not. */
async function decode(source: Blob): Promise<ImageBitmap> {
  try {
    return await createImageBitmap(source);
  } catch {
    throw new Error(photoRefusal("decode"));
  }
}

/** The square of `edge` (or the source's own square when smaller), drawn from the centre of `image`. */
function square(image: ImageBitmap, edge: number): HTMLCanvasElement {
  const crop = coverCrop(image.width, image.height, edge);
  const canvas = document.createElement("canvas");
  canvas.width = crop.dw;
  canvas.height = crop.dh;
  const ctx = canvas.getContext("2d");
  if (!ctx || crop.dw === 0) throw new Error(photoRefusal("decode"));
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = "high";
  ctx.drawImage(image, crop.sx, crop.sy, crop.sw, crop.sh, 0, 0, crop.dw, crop.dh);
  return canvas;
}

function encode(canvas: HTMLCanvasElement, mime: string = PHOTO_MIME, quality?: number): Promise<Blob> {
  return new Promise((resolve, reject) => {
    canvas.toBlob((blob) => (blob ? resolve(blob) : reject(new Error(photoRefusal("encode")))), mime, quality);
  });
}

/**
 * The picked picture as the photo that is kept: the profile's square in the
 * profile's encoding, named after the file — a picture as a 256 px PNG, a
 * face as a 96 px JPEG tried at each of the profile's qualities in turn
 * until it fits the cap. Refuses, in words, a file that is not a picture,
 * will not decode, or will not fit.
 */
export async function scalePhoto(file: File, profile: PhotoProfile = PHOTO_PROFILES.picture): Promise<File> {
  if (!isPhotoType(file.type)) throw new Error(photoRefusal("type"));
  const image = await decode(file);
  try {
    const canvas = square(image, profile.edge);
    const qualities: readonly (number | undefined)[] = profile.qualities.length ? profile.qualities : [undefined];
    for (const quality of qualities) {
      const blob = await encode(canvas, profile.mime, quality);
      if (fitsProfile(blob.size, profile)) return new File([blob], photoName(file.name, profile), { type: profile.mime });
    }
    throw new Error(photoRefusal("size", profile));
  } finally {
    image.close();
  }
}

/**
 * A stored photo's bytes as the small square every draw site shares — an
 * object URL the caller owns (`URL.revokeObjectURL` when it is let go).
 */
export async function thumbOf(url: string, edge: number = THUMB_EDGE): Promise<string> {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`the photo could not be fetched (${res.status})`);
  const image = await decode(await res.blob());
  try {
    return URL.createObjectURL(await encode(square(image, edge)));
  } finally {
    image.close();
  }
}
