/**
 * *New drawing* (19 — Drawings): one dialog instead of a menu of rows. A
 * gallery of the templates — each a tile with a preview drawn from its own
 * skeleton, its name and a line on when to reach for it — a title the
 * template proposes and a hand may change, and, only when the tab offers
 * several places, *Where* the drawing is filed (the place you stand on
 * first). *Create* makes it and opens it; Enter in the title does the same.
 * The rules are `newDrawingModel.mjs`'s; the words are the catalog's.
 */

import { useEffect, useRef, useState } from "react";
import { Button, Dialog, Field, Select, TextInput, Tile } from "../ui";
import type { DrawTarget, OwnerScope } from "./drawModel.mjs";
import { canCreate, defaultTitle, firstTarget, targetKey, titleFollowsTemplate } from "./newDrawingModel.mjs";
import { TEMPLATES, templateOf } from "./templates/index.mjs";
import { TemplatePreview } from "./TemplatePreview";
import { t } from "../i18n/l10n.mjs";

export function NewDrawingDialog({
  open,
  onClose,
  targets,
  onCreate,
}: {
  open: boolean;
  onClose: () => void;
  /** Where a new drawing may be filed under the current tab, the *here* one first. */
  targets: readonly DrawTarget[];
  onCreate: (scope: OwnerScope, template: string, title: string) => void;
}) {
  const untitled = t("draw-overlay-untitled-drawing");
  const [template, setTemplate] = useState("empty");
  const [title, setTitle] = useState(untitled);
  const [where, setWhere] = useState<string>("");
  const titleBox = useRef<HTMLInputElement>(null);

  // Every opening starts afresh: *Empty*, its title, the place you stand on.
  useEffect(() => {
    if (!open) return;
    setTemplate("empty");
    setTitle(untitled);
    const first = firstTarget(targets);
    setWhere(first ? targetKey(first.scope) : "");
  }, [open, targets, untitled]);

  const pick = (id: string) => {
    const previous = defaultTitle(templateOf(template), untitled);
    setTemplate(id);
    if (titleFollowsTemplate(title, previous)) setTitle(defaultTitle(templateOf(id), untitled));
  };
  const target = targets.find((x) => targetKey(x.scope) === where) ?? firstTarget(targets);
  const ready = canCreate(title) && target !== null;
  const submit = () => {
    if (!ready || !target) return;
    onCreate(target.scope, template, title.trim());
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={t("draw-overlay-new-drawing")}
      description={t("draw-new-description")}
      width="max-w-3xl"
      initialFocus={titleBox}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("draw-new-cancel")}
          </Button>
          <Button variant="primary" disabled={!ready} onClick={submit}>
            {t("draw-new-create")}
          </Button>
        </>
      }
    >
      <div role="radiogroup" aria-label={t("draw-new-template")} className="grid gap-3 sm:grid-cols-3 lg:grid-cols-4">
        {TEMPLATES.map((tpl) => (
          <Tile
            key={tpl.id}
            role="radio"
            active={tpl.id === template}
            onSelect={() => pick(tpl.id)}
            name={tpl.label}
            blurb={tpl.blurb}
            previewClass="block h-24 bg-surface-2 p-1.5"
            preview={<TemplatePreview skeleton={tpl.skeleton} />}
          />
        ))}
      </div>
      <div className="mt-4 grid gap-3 sm:grid-cols-2">
        <Field label={t("draw-new-name")} htmlFor="new-drawing-title">
          <TextInput
            id="new-drawing-title"
            ref={titleBox}
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                submit();
              }
            }}
            className="h-8 w-full text-xs"
          />
        </Field>
        {targets.length > 1 && (
          <Field label={t("draw-new-where")} htmlFor="new-drawing-where">
            <Select id="new-drawing-where" value={where} onChange={(e) => setWhere(e.target.value)} className="h-8 w-full text-xs">
              {targets.map((x) => (
                <option key={targetKey(x.scope)} value={targetKey(x.scope)}>
                  {x.here ? t("draw-overlay-here", { t: x.label }) : x.label}
                </option>
              ))}
            </Select>
          </Field>
        )}
      </div>
    </Dialog>
  );
}
