/**
 * The keymap (ide/15): a preset and your overrides, both Machine settings, so
 * it is a file you can copy. Every command with its scope and chord, one
 * table per scope with a filter over labels, ids and chords; record a new
 * chord on a row — a bare `Enter`, `F2` or `Delete` included — and a chord
 * another command already holds in that scope is refused by name, never
 * stored twice.
 */

import { useEffect, useState } from "react";
import { api } from "../../api";
import { Button, ErrorNote, Field, KeyHint, Pending, Select, TextInput, failureText, useToast } from "../../ui";
import { isMac } from "../../ui/KeyHint";
import { PRESETS, WHENS, bindingsIn, chordFromEvent, conflictFor, filterBindings, resolveKeymap } from "../../shell/keymapModel.mjs";
import type { Binding, Keymap, When } from "../../shell/keymapModel.mjs";
import { reloadKeymap } from "../../shell/shortcuts";
import { useAsync } from "../_work/useAsync";
import { pendingRows, phase } from "./loadModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** What each scope means, beside its table. */
const WHEN_BLURB: Record<When, string> = {
  global: t("settings-keymap-panel-everywhere"),
  workbench: t("settings-keymap-panel-project-workstream-goal-open-ide"),
  tabs: t("settings-keymap-panel-while-ide-s-tab-strip-screen"),
  files: t("settings-keymap-panel-files-tree-focused-narrowest-scope-wins"),
  editor: t("settings-keymap-panel-editor-focused"),
  document: t("settings-keymap-panel-editor-rendered-document-focused-find-document"),
  terminal: t("settings-keymap-panel-terminal-focused-chords-shell-could-mean"),
  browser: t("settings-keymap-panel-browser-tab-s-body-focused-ide"),
};

export function KeymapPanel() {
  const toast = useToast();
  const settings = useAsync((s) => api.settingsResolved(null, s), []);
  const get = (k: string) => settings.data?.settings.find((x) => x.key === k)?.value;
  const preset = typeof get("keymap.preset") === "string" ? (get("keymap.preset") as string) : "default";
  const overrides = (get("keymap.overrides") as Record<string, string> | undefined) ?? {};
  const [keymap, setKeymap] = useState<Keymap>(() => resolveKeymap(preset, overrides, isMac));
  const [recording, setRecording] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [filter, setFilter] = useState("");
  // `settings.data` is the one fact: `preset` and `overrides` are read out of
  // it each render, and `overrides` is a fresh object while the setting is
  // unset — following it would resolve the keymap on every render.
  useEffect(() => setKeymap(resolveKeymap(preset, overrides, isMac)), [settings.data]); // eslint-disable-line react-hooks/exhaustive-deps

  const save = async (values: Record<string, unknown>, did: string) => {
    if (busy) return;
    setBusy(true);
    try {
      await api.setSettings("machine", values, null);
      toast.ok(did);
      settings.reload();
      void reloadKeymap();
    } catch (e) {
      toast.error(failureText("settings", "keymap-panel-failed", e));
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    if (!recording) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        setRecording(null);
        return;
      }
      const chord = chordFromEvent(e, isMac);
      if (!chord) return;
      const other = conflictFor(keymap, recording, chord);
      if (other) {
        toast.error(t("settings-keymap-panel-already-means-there-unbind-first-pick", { chord, other: keymap.bindings.find((b) => b.id === other)?.label ?? other }));
        setRecording(null);
        return;
      }
      setRecording(null);
      void save({ "keymap.overrides": { ...overrides, [recording]: chord } }, t("settings-keymap-panel-now-means", { chord, recording: keymap.bindings.find((b) => b.id === recording)?.label ?? recording }));
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
    // `save` is a plain function remade every render; the listener is re-hung on the facts it reads.
  }, [recording, keymap, overrides]); // eslint-disable-line react-hooks/exhaustive-deps

  const unbind = (b: Binding) => void save({ "keymap.overrides": { ...overrides, [b.id]: "" } }, t("settings-keymap-panel-unbound", { b: b.label }));
  const reset = (b: Binding) => {
    const next = { ...overrides };
    delete next[b.id];
    void save({ "keymap.overrides": next }, t("settings-keymap-panel-back-preset", { b: b.label }));
  };

  const row = (b: Binding) => (
    <tr key={b.id} className="border-t border-hairline">
      <td className="py-1.5 text-text">
        {b.label}
        {b.note && <span className="ml-1 text-text-dim">({b.note})</span>}
        <span className="ml-2 font-mono text-text-dim">{b.id}</span>
      </td>
      <td className="py-1.5">
        {recording === b.id ? (
          <span className="text-accent-ink">{t("settings-keymap-panel-press-new-chord-esc-cancels")}</span>
        ) : b.chord ? (
          <span className="inline-flex items-center gap-1">
            <KeyHint combo={b.chord} />
            {b.source === "override" && <span className="text-text-dim">{t("settings-keymap-panel-override")}</span>}
          </span>
        ) : (
          <span className="text-text-dim">{t("settings-keymap-panel-chord-unbound", { flag: b.source === "override" ? "yes" : "no" })}</span>
        )}
      </td>
      <td className="py-1.5 text-right">
        {!b.always && (
          <span className="inline-flex gap-1">
            <Button size="sm" variant="ghost" disabled={busy || recording !== null} onClick={() => setRecording(b.id)}>{t("settings-keymap-panel-record")}</Button>
            {b.chord && (
              <Button size="sm" variant="ghost" disabled={busy} onClick={() => unbind(b)}>{t("settings-keymap-panel-unbind")}</Button>
            )}
            {b.source === "override" && (
              <Button size="sm" variant="ghost" disabled={busy} onClick={() => reset(b)}>{t("settings-appearance-panel-reset")}</Button>
            )}
          </span>
        )}
      </td>
    </tr>
  );

  const groups = WHENS.map((when) => ({ when, rows: filterBindings(bindingsIn(keymap, when), filter) })).filter((g) => g.rows.length > 0);

  // The table waits for the resolved settings: a preset drawn and then
  // rewritten chord by chord is worse than a moment of *reading the keymap…*.
  const state = phase(settings);
  if (state === "pending") return <Pending what={t("settings-load-keymap")} rows={pendingRows(t("settings-load-keymap"))} className="mb-4" />;
  if (state === "failed") return <ErrorNote error={settings.error ?? t("settings-keymap-panel-settings-read-refused")} retry={settings.reload} />;

  return (
    <section className="mb-4 rounded-card border border-border bg-surface p-3 shadow-sm">
      <div className="flex flex-wrap items-end gap-3">
        <Field label={t("settings-keymap-panel-preset")} hint={t("settings-keymap-panel-default-keeps-what-app-has-always")}>
          <Select value={preset} onChange={(e) => void save({ "keymap.preset": e.target.value }, t("settings-keymap-panel-preset-2", { target: e.target.value }))}>
            {PRESETS.map((p) => (
              <option key={p} value={p}>
                {p}
              </option>
            ))}
          </Select>
        </Field>
        <Field label={t("settings-keymap-panel-find-command")} hint={t("settings-keymap-panel-what-does-id-chord")}>
          <TextInput value={filter} onChange={(e) => setFilter(e.target.value)} placeholder={t("settings-keymap-panel-rename-close-tab-mod-shift-f")} aria-label={t("settings-keymap-panel-filter-keymap")} />
        </Field>
        {Object.keys(overrides).length > 0 && (
          <Button size="sm" variant="ghost" disabled={busy} onClick={() => void save({ "keymap.overrides": {} }, t("settings-keymap-panel-overrides-cleared"))}>{t("settings-keymap-panel-clear-all-overrides")}</Button>
        )}
      </div>
      <p className="mt-2 max-w-measure text-2xs leading-relaxed text-text-dim">{t("settings-keymap-panel-chord-needs-modifier-except-enter-space")}</p>
      {keymap.warnings.length > 0 && (
        <ul className="mt-2 text-2xs text-warn">
          {keymap.warnings.map((w) => (
            <li key={w}>{w}</li>
          ))}
        </ul>
      )}
      {groups.length === 0 && <p className="mt-3 text-2xs text-text-dim">{t("settings-keymap-panel-command-matches", { filter })}</p>}
      {groups.map((g) => (
        <div key={g.when} className="mt-5">
          {/* An `h3`: the panel's title is the `h2`, and a level skipped is a heading a screen reader's outline cannot place. */}
          <h3 className="text-xs font-semibold text-text">
            <span className="font-mono">{g.when}</span>
            <span className="ml-2 text-2xs font-normal text-text-dim">{WHEN_BLURB[g.when]}</span>
          </h3>
          <table className="mt-1 w-full text-2xs">
            <thead>
              <tr className="text-left text-text-dim">
                <th className="py-1 font-semibold">{t("settings-keymap-panel-command")}</th>
                <th className="py-1 font-semibold">{t("settings-keymap-panel-chord")}</th>
                <th className="py-1" />
              </tr>
            </thead>
            <tbody>{g.rows.map(row)}</tbody>
          </table>
        </div>
      ))}
    </section>
  );
}
