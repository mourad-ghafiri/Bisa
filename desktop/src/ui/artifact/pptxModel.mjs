/**
 * A deck's outline (ide/12): which entries of a `.pptx` are slides and in
 * what order, what each says, which pictures it carries, and its notes —
 * read off the zip's XML as strings, so a deck shows as its slides without a
 * renderer nobody publishes. Fidelity is *Open with the default app*'s.
 */

import { t as tr } from "../../i18n/l10n.mjs";

/** The slide entries of a deck, in slide order. */
export function slidePaths(names) {
  return names
    .map((n) => {
      const m = /^ppt\/slides\/slide(\d+)\.xml$/.exec(n);
      return m ? { name: n, index: Number(m[1]) } : null;
    })
    .filter(Boolean)
    .sort((a, b) => a.index - b.index)
    .map((s) => s.name);
}

/** `ppt/slides/slide3.xml` → `ppt/slides/_rels/slide3.xml.rels`. */
export function slideRelsPath(slidePath) {
  const at = slidePath.lastIndexOf("/");
  return `${slidePath.slice(0, at)}/_rels/${slidePath.slice(at + 1)}.rels`;
}

/** The notes entry a slide's rels name, resolved against the slide's folder. */
export function notesPath(relsXml, slidePath) {
  const target = [...String(relsXml ?? "").matchAll(/<Relationship\b[^>]*>/g)]
    .map((m) => m[0])
    .filter((r) => /Type="[^"]*\/notesSlide"/.test(r))
    .map((r) => /Target="([^"]+)"/.exec(r)?.[1])
    .find(Boolean);
  return target ? resolveEntry(slidePath, target) : null;
}

function decodeXml(s) {
  return s
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&apos;/g, "'")
    .replace(/&#(\d+);/g, (_, n) => String.fromCodePoint(Number(n)))
    .replace(/&amp;/g, "&");
}

/** The runs of one paragraph joined; a break is a space. */
function paragraphText(pXml) {
  const runs = [...pXml.matchAll(/<a:t>([\s\S]*?)<\/a:t>/g)].map((m) => decodeXml(m[1]));
  return runs.join("").replace(/\s+/g, " ").trim();
}

/**
 * What a slide says: its title (the shape typed `title` or `ctrTitle`, else
 * the first paragraph) and every other paragraph in order.
 */
export function slideText(xml) {
  const src = String(xml ?? "");
  const shapes = [...src.matchAll(/<p:sp>([\s\S]*?)<\/p:sp>/g)].map((m) => m[1]);
  let title = "";
  const paragraphs = [];
  for (const shape of shapes) {
    const isTitle = /<p:ph\b[^>]*type="(ctrTitle|title)"/.test(shape);
    const lines = [...shape.matchAll(/<a:p>([\s\S]*?)<\/a:p>/g)]
      .map((m) => paragraphText(m[1]))
      .filter(Boolean);
    if (isTitle && !title) {
      title = lines.join(" ");
      continue;
    }
    paragraphs.push(...lines);
  }
  if (!title && paragraphs.length > 0) title = paragraphs.shift();
  return { title, paragraphs };
}

/** The pictures a slide's rels name, as zip entries under `ppt/media/`. */
export function slideImages(relsXml, slidePath) {
  return [...String(relsXml ?? "").matchAll(/<Relationship\b[^>]*>/g)]
    .map((m) => m[0])
    .filter((r) => /Type="[^"]*\/image"/.test(r))
    .map((r) => /Target="([^"]+)"/.exec(r)?.[1])
    .filter(Boolean)
    .map((t) => resolveEntry(slidePath, t));
}

/** A rels target (`../media/image1.png`) against the slide's own folder. */
export function resolveEntry(fromPath, target) {
  if (target.startsWith("/")) return target.slice(1);
  const parts = fromPath.split("/").slice(0, -1);
  for (const seg of target.split("/")) {
    if (seg === "..") parts.pop();
    else if (seg !== "." && seg !== "") parts.push(seg);
  }
  return parts.join("/");
}

/** The speaker notes, joined. */
export function notesText(xml) {
  return [...String(xml ?? "").matchAll(/<a:p>([\s\S]*?)<\/a:p>/g)]
    .map((m) => paragraphText(m[1]))
    .filter(Boolean)
    .join("\n");
}

/** *12 slides*, *1 slide*. */
export function slideWords(count) {
  return count === 1 ? "1 slide" : tr("ui-pptx-slides", { count });
}
