/**
 * Monaco's models for the documents' tabs (ide/03 §Editing features), over
 * `monacoModelsModel.mjs`: a tab's editor acquires its model by the tab's
 * key (`editorKey`, `<root>|<tab id>`) and leaves it when it unmounts; the
 * workbench lets the model go when the tab closes (`workbenchStore`'s `set`,
 * beside the buffer and the place). The undo history is the model's, so it
 * survives a tab switch, a mode switch and the review lens.
 *
 * A value pushed into a model from outside — a reload after `file_changed`,
 * a format on save, *Take theirs*, a replace from a rendering — goes in as
 * **one undoable step** (`pushValue`): `pushEditOperations` between two undo
 * stops, never `setValue`, which destroys the history and flushes the caret.
 */

import type { Monaco } from "./monaco";
import { KeptModels } from "./monacoModelsModel.mjs";

type MonacoModule = typeof import("monaco-editor");

const kept = new KeptModels<Monaco.editor.ITextModel>();

/**
 * The file's own URI when no model claims it yet — a second view of the
 * same file (a conflict beside the document, a split showing it twice) gets
 * a model with no address rather than a throw from Monaco.
 */
export function freeFileUri(m: MonacoModule, path: string | undefined): Monaco.Uri | undefined {
  if (!path) return undefined;
  const uri = m.Uri.file(path);
  return m.editor.getModel(uri) ? undefined : uri;
}

/**
 * The model kept for a tab — made now with `value` when the tab has none yet,
 * relabelled when the language changed (a kept model keeps its history
 * through that, where a new mount would not).
 */
export function acquireModel(m: MonacoModule, key: string, value: string, language: string | undefined, path: string | undefined): Monaco.editor.ITextModel {
  const model = kept.acquire(key, () => m.editor.createModel(value, language, freeFileUri(m, path)));
  if (language && model.getLanguageId() !== language) m.editor.setModelLanguage(model, language);
  return model;
}

/** The tabs closed: their models go with them. */
export function releaseModels(keys: readonly string[]): void {
  kept.release(keys);
}

/**
 * Push `value` into the model as one undoable step, keeping the editor's
 * view — scroll, caret, selection — through it: a format on save or a
 * reload under a clean buffer leaves the person where they were, and ⌘Z
 * brings the text before it back. `pushing` is held while the model's own
 * change event fires, so the owner's `onChange` can leave the echo alone:
 * the buffer already holds the text, and an echo on a saving buffer would
 * read as a keystroke. An equal text is no step at all.
 */
export function pushValue(ed: Monaco.editor.ICodeEditor, model: Monaco.editor.ITextModel, value: string, pushing: { current: boolean }): void {
  if (model.getValue() === value) return;
  const view = ed.saveViewState();
  pushing.current = true;
  try {
    model.pushStackElement();
    model.pushEditOperations(ed.getSelections(), [{ range: model.getFullModelRange(), text: value }], () => null);
    model.pushStackElement();
  } finally {
    pushing.current = false;
  }
  if (view) ed.restoreViewState(view);
}
