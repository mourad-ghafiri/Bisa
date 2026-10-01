/**
 * What the notes overlay does, and how much of it is on screen.
 *
 * Every control here is window state on *this machine* — the same test that
 * keeps notes themselves off the wire. Nothing in this panel reaches the node,
 * the index or the protocol, which is why it has no loading state and no error
 * path: there is nothing to fail.
 *
 * # Why notes has a panel at all
 *
 * Because the alternative was leaving these as constants. The dock was always
 * on screen with no way to reclaim the corner, a note always opened in Write,
 * the agent box was always at the foot of the editor, and the autosave delay
 * was a number in `NoteEditor.tsx`. Those are preferences wearing the costume
 * of implementation details, and a person disagreeing with one of them had
 * nowhere to go.
 */

import { Button, Card, Section, SegmentedControl, Slider, Switch } from "../../ui";
import {
  NOTE_VIEWS,
  SAVE_MAX_MS,
  SAVE_MIN_MS,
  SAVE_STEP_MS,
  type NoteView,
} from "../../notes/notesModel.mjs";
import {
  dockMoved,
  resetDockPosition,
  setCountBadge,
  setDefaultView,
  setNotesMaximized,
  setDockVisible,
  setSaveDelay,
  useNotesOverlay,
} from "../../notes/notesStore";
import { t } from "../../i18n/l10n.mjs";

const VIEW_LABELS: Record<NoteView, string> = {
  write: t("settings-notes-panel-write"),
  split: t("settings-notes-panel-split"),
  read: t("settings-notes-panel-read"),
};

const VIEW_OPTIONS = NOTE_VIEWS.map((id) => ({ id, label: VIEW_LABELS[id] }));

export function NotesPanel() {
  const { dock, dockVisible, countBadge, defaultView, saveDelay, maximized } = useNotesOverlay();
  const moved = dockMoved(dock);

  return (
    <>
      <Section title={t("settings-notes-panel-screen")}>
        <Card>
          <div className="flex flex-col gap-3">
            <Switch
              checked={dockVisible}
              onChange={setDockVisible}
              label={t("settings-notes-panel-show-floating-dock")}
              // Said plainly, because a switch that removed the only way into
              // a feature would be a trap rather than a setting.
              hint={t("settings-notes-panel-button-corner-alt-n-still-opens")}
            />
            <Switch
              checked={countBadge}
              onChange={setCountBadge}
              // Disabled rather than hidden: a control that has vanished
              // leaves the reader wondering whether it ever existed, and a
              // greyed one says why it does nothing.
              disabled={!dockVisible}
              label={t("settings-notes-panel-show-note-count-dock")}
              hint={
                dockVisible
                  ? t("settings-notes-panel-small-number-corner-button")
                  : t("settings-notes-panel-nothing-put-count-while-dock-hidden")
              }
            />
            <Switch
              checked={maximized}
              onChange={setNotesMaximized}
              label={t("settings-notes-panel-open-maximized")}
              hint={t("settings-notes-panel-maximized-hint")}
            />
          </div>
        </Card>
      </Section>

      <Section title={t("settings-notes-panel-editing")}>
        <Card>
          <div className="flex flex-col gap-4">
            <div>
              <span className="mb-1 block text-2xs font-medium text-text-dim">{t("settings-notes-panel-notes-open")}</span>
              <SegmentedControl
                options={VIEW_OPTIONS}
                value={defaultView}
                onChange={setDefaultView}
                label={t("settings-notes-panel-default-view")}
                size="sm"
              />
              <span className="mt-1 block text-2xs text-text-dim">{t("settings-notes-panel-switching-view-while-reading-note-only")}</span>
            </div>
            <Slider
              label={t("settings-notes-panel-save-after-stop-typing")}
              value={saveDelay}
              min={SAVE_MIN_MS}
              max={SAVE_MAX_MS}
              step={SAVE_STEP_MS}
              onChange={setSaveDelay}
              format={(ms) => `${(ms / 1000).toFixed(1)}s`}
              hint={t("settings-notes-panel-shorter-saves-sooner-asks-node-more")}
            />
          </div>
        </Card>
      </Section>

      <Section title={t("settings-notes-panel-layout")}>
        <Card>
          <div className="flex items-center justify-between gap-3">
            <p className="text-2xs text-text-dim">
              {moved
                ? t("settings-notes-panel-dock-has-been-dragged-from-corner")
                : t("settings-notes-panel-dock-where-started")}{" "}
              {t("settings-notes-panel-size-resets-double-click")}
            </p>
            <Button size="sm" variant="ghost" disabled={!moved} onClick={resetDockPosition}>{t("settings-notes-panel-reset-position")}</Button>
          </div>
        </Card>
      </Section>
    </>
  );
}
